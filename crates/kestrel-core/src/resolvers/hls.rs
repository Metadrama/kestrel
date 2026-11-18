use super::{LinkResolver, ResolvedMedia};
use async_trait::async_trait;
use kestrel_proto::{KestrelError, MediaVariant, MimeCategory};
use reqwest::header::HeaderMap;
use url::Url;

pub struct HlsDashResolver {
    client: reqwest::Client,
}

impl HlsDashResolver {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::limited(10))
                .build()
                .unwrap_or_default(),
        }
    }
}

impl Default for HlsDashResolver {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl LinkResolver for HlsDashResolver {
    fn priority(&self) -> u8 {
        90
    }

    fn can_handle(&self, url: &Url) -> bool {
        let path = url.path().to_ascii_lowercase();
        path.ends_with(".m3u8") || path.ends_with(".mpd")
    }

    async fn resolve(&self, url: &Url, headers: &HeaderMap) -> Result<ResolvedMedia, KestrelError> {
        let mut req = self.client.get(url.as_str());
        for (k, v) in headers.iter() {
            req = req.header(k, v);
        }

        let res = req.send().await
            .map_err(|e| KestrelError::Network(format!("Failed to fetch HLS manifest: {}", e)))?;

        let body = res.text().await
            .map_err(|e| KestrelError::Network(format!("Failed to read HLS manifest body: {}", e)))?;

        // Parse with m3u8-rs
        if let Ok(playlist) = m3u8_rs::parse_playlist_res(body.as_bytes()) {
            match playlist {
                m3u8_rs::Playlist::MasterPlaylist(master) => {
                    let mut variants = Vec::new();
                    for (i, stream) in master.variants.iter().enumerate() {
                        let res_str = stream.resolution.as_ref().map(|r| format!("{}x{}", r.width, r.height));
                        let bw = stream.bandwidth;
                        let label = match &res_str {
                            Some(r) => format!("{} ({} kbps)", r, bw / 1000),
                            None => format!("Stream {} ({} kbps)", i + 1, bw / 1000),
                        };

                        let direct_uri = url.join(&stream.uri)
                            .map(|u| u.to_string())
                            .unwrap_or_else(|_| stream.uri.clone());

                        variants.push(MediaVariant {
                            id: format!("variant_{}", i),
                            label,
                            format_id: format!("hls_{}", bw),
                            resolution: res_str,
                            filesize_estimate: None,
                            direct_url: Some(direct_uri),
                            audio_url: None,
                        });
                    }

                    return Ok(ResolvedMedia {
                        title: "HLS Stream".to_string(),
                        mime_category: MimeCategory::Video,
                        variants,
                        direct_download_url: None,
                        audio_url: None,
                        is_hls_stream: true,
                        hls_segments: Vec::new(),
                    });
                }
                m3u8_rs::Playlist::MediaPlaylist(media) => {
                    let mut segments = Vec::new();
                    for seg in media.segments {
                        let seg_url = url.join(&seg.uri)
                            .map(|u| u.to_string())
                            .unwrap_or(seg.uri);
                        segments.push(seg_url);
                    }

                    return Ok(ResolvedMedia {
                        title: "HLS Media".to_string(),
                        mime_category: MimeCategory::Video,
                        variants: Vec::new(),
                        direct_download_url: Some(url.to_string()),
                        audio_url: None,
                        is_hls_stream: true,
                        hls_segments: segments,
                    });
                }
            }
        }

        Err(KestrelError::Protocol("Could not parse HLS/M3U8 playlist".into()))
    }
}
