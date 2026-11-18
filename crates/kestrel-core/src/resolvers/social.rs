use super::{LinkResolver, ResolvedMedia};
use async_trait::async_trait;
use kestrel_proto::{KestrelError, MediaVariant, MimeCategory};
use regex::Regex;
use reqwest::header::{HeaderMap, USER_AGENT};
use url::Url;

pub struct SocialNativeResolver {
    client: reqwest::Client,
}

impl SocialNativeResolver {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::limited(10))
                .build()
                .unwrap_or_default(),
        }
    }
}

impl Default for SocialNativeResolver {
    fn default() -> Self {
        Self::new()
    }
}

impl SocialNativeResolver {

    async fn resolve_reddit(&self, url: &Url) -> Result<ResolvedMedia, KestrelError> {
        let mut json_url = url.clone();
        let path = json_url.path().trim_end_matches('/').to_string() + ".json";
        json_url.set_path(&path);

        let res = self.client.get(json_url.as_str())
            .header(USER_AGENT, "Mozilla/5.0 (Windows NT 10.0; Win64; x64) Kestrel/1.0")
            .send()
            .await
            .map_err(|e| KestrelError::Network(format!("Reddit API error: {}", e)))?;

        let json_val: serde_json::Value = res.json().await
            .map_err(|e| KestrelError::Protocol(format!("Invalid Reddit JSON: {}", e)))?;

        // Extract media object
        if let Some(arr) = json_val.as_array() {
            if let Some(first) = arr.first() {
                if let Some(children) = first.pointer("/data/children") {
                    if let Some(post) = children.get(0) {
                        let title = post.pointer("/data/title")
                            .and_then(|v| v.as_str())
                            .unwrap_or("Reddit Video")
                            .to_string();

                        if let Some(media) = post.pointer("/data/media/reddit_video") {
                            let fallback = media.get("fallback_url").and_then(|v| v.as_str());
                            let hls = media.get("hls_url").and_then(|v| v.as_str());
                            let width = media.get("width").and_then(|v| v.as_u64());
                            let height = media.get("height").and_then(|v| v.as_u64());

                            let mut variants = Vec::new();
                            let res_label = match (width, height) {
                                (Some(w), Some(h)) => format!("{}x{}", w, h),
                                _ => "Best Quality".to_string(),
                            };

                            // Reddit separates video & audio in fallback_url
                            // e.g. fallback_url = https://v.redd.it/xyz/DASH_1080.mp4
                            // audio is usually https://v.redd.it/xyz/DASH_audio.mp4
                            let (video_url, audio_url) = if let Some(fb) = fallback {
                                let a_url = fb.rfind('/').map(|idx| format!("{}/DASH_audio.mp4", &fb[..idx]));
                                (Some(fb.to_string()), a_url)
                            } else {
                                (hls.map(|s| s.to_string()), None)
                            };

                            variants.push(MediaVariant {
                                id: "reddit_best".to_string(),
                                label: format!("Reddit Video ({})", res_label),
                                format_id: "reddit_video".to_string(),
                                resolution: Some(res_label),
                                filesize_estimate: None,
                                direct_url: video_url.clone(),
                                audio_url: audio_url.clone(),
                            });

                            return Ok(ResolvedMedia {
                                title,
                                mime_category: MimeCategory::Video,
                                variants,
                                direct_download_url: video_url,
                                audio_url,
                                is_hls_stream: hls.is_some() && fallback.is_none(),
                                hls_segments: Vec::new(),
                            });
                        }
                    }
                }
            }
        }

        Err(KestrelError::NotFound("No video media found in Reddit post".into()))
    }

    async fn resolve_x_twitter(&self, url: &Url) -> Result<ResolvedMedia, KestrelError> {
        // Extract tweet ID
        let segments: Vec<&str> = url.path_segments().map(|c| c.collect()).unwrap_or_default();
        let status_idx = segments.iter().position(|&s| s == "status");
        let tweet_id = match status_idx {
            Some(idx) if idx + 1 < segments.len() => segments[idx + 1],
            _ => return Err(KestrelError::Protocol("Invalid X/Twitter status URL".into())),
        };

        // Query syndication CDN API
        let syn_url = format!("https://cdn.syndication.twimg.com/tweet-result?id={}&token=x", tweet_id);
        let res = self.client.get(&syn_url)
            .header(USER_AGENT, "Mozilla/5.0 (Windows NT 10.0; Win64; x64) Kestrel/1.0")
            .send()
            .await
            .map_err(|e| KestrelError::Network(format!("X/Twitter API error: {}", e)))?;

        let val: serde_json::Value = res.json().await
            .map_err(|e| KestrelError::Protocol(format!("Invalid X syndication response: {}", e)))?;

        let title = val.get("text").and_then(|t| t.as_str()).unwrap_or("X Video").to_string();

        if let Some(media_arr) = val.get("mediaDetails").and_then(|m| m.as_array()) {
            for m in media_arr {
                if let Some(variants_arr) = m.pointer("/video_info/variants").and_then(|v| v.as_array()) {
                    let mut variants = Vec::new();
                    for (i, v) in variants_arr.iter().enumerate() {
                        let ct = v.get("content_type").and_then(|s| s.as_str()).unwrap_or("");
                        let v_url = v.get("url").and_then(|s| s.as_str());
                        let bitrate = v.get("bitrate").and_then(|b| b.as_u64());

                        if ct == "video/mp4" && v_url.is_some() {
                            let label = match bitrate {
                                Some(b) => format!("MP4 ({} kbps)", b / 1000),
                                None => format!("MP4 Variant {}", i + 1),
                            };
                            variants.push(MediaVariant {
                                id: format!("x_{}", i),
                                label,
                                format_id: format!("x_{}", bitrate.unwrap_or(0)),
                                resolution: None,
                                filesize_estimate: None,
                                direct_url: v_url.map(|s| s.to_string()),
                                audio_url: None,
                            });
                        }
                    }

                    if !variants.is_empty() {
                        let direct = variants.first().and_then(|v| v.direct_url.clone());
                        return Ok(ResolvedMedia {
                            title,
                            mime_category: MimeCategory::Video,
                            variants,
                            direct_download_url: direct,
                            audio_url: None,
                            is_hls_stream: false,
                            hls_segments: Vec::new(),
                        });
                    }
                }
            }
        }

        Err(KestrelError::NotFound("No video found in X/Twitter post".into()))
    }

    async fn resolve_opengraph(&self, url: &Url) -> Result<ResolvedMedia, KestrelError> {
        let res = self.client.get(url.as_str())
            .header(USER_AGENT, "Mozilla/5.0 (Windows NT 10.0; Win64; x64) Kestrel/1.0")
            .send()
            .await
            .map_err(|e| KestrelError::Network(format!("Failed to fetch page: {}", e)))?;

        let body = res.text().await
            .map_err(|e| KestrelError::Network(format!("Failed to read page HTML: {}", e)))?;

        // Extract <meta property="og:video" content="..."> or <meta property="og:video:url" content="...">
        let og_regex = Regex::new(r#"<meta\s+property=["']og:video(?::url)?["']\s+content=["']([^"']+)["']"#).unwrap();
        let title_regex = Regex::new(r#"<meta\s+property=["']og:title["']\s+content=["']([^"']+)["']"#).unwrap();

        let title = title_regex.captures(&body)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().to_string())
            .unwrap_or_else(|| "Video".to_string());

        if let Some(cap) = og_regex.captures(&body) {
            if let Some(vid_url) = cap.get(1) {
                let resolved_url = url.join(vid_url.as_str())
                    .map(|u| u.to_string())
                    .unwrap_or_else(|_| vid_url.as_str().to_string());

                let is_hls = resolved_url.contains(".m3u8");

                return Ok(ResolvedMedia {
                    title,
                    mime_category: MimeCategory::Video,
                    variants: vec![MediaVariant {
                        id: "og_video".to_string(),
                        label: "Direct Video".to_string(),
                        format_id: "og_video".to_string(),
                        resolution: None,
                        filesize_estimate: None,
                        direct_url: Some(resolved_url.clone()),
                        audio_url: None,
                    }],
                    direct_download_url: Some(resolved_url),
                    audio_url: None,
                    is_hls_stream: is_hls,
                    hls_segments: Vec::new(),
                });
            }
        }

        Err(KestrelError::NotFound("No OpenGraph video tag found".into()))
    }
}

#[async_trait]
impl LinkResolver for SocialNativeResolver {
    fn priority(&self) -> u8 {
        80
    }

    fn can_handle(&self, url: &Url) -> bool {
        let host = url.host_str().unwrap_or("").to_ascii_lowercase();
        host.contains("reddit.com") || host.contains("redd.it") ||
        host.contains("twitter.com") || host.contains("x.com") ||
        host.contains("tiktok.com") || host.contains("instagram.com")
    }

    async fn resolve(&self, url: &Url, _headers: &HeaderMap) -> Result<ResolvedMedia, KestrelError> {
        let host = url.host_str().unwrap_or("").to_ascii_lowercase();
        if host.contains("reddit.com") || host.contains("redd.it") {
            return self.resolve_reddit(url).await;
        }
        if host.contains("twitter.com") || host.contains("x.com") {
            return self.resolve_x_twitter(url).await;
        }

        // Fallback to OpenGraph parsing
        self.resolve_opengraph(url).await
    }
}
