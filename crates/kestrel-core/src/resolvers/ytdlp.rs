use super::{LinkResolver, ResolvedMedia};
use async_trait::async_trait;
use kestrel_proto::{KestrelError, MediaVariant, MimeCategory};
use reqwest::header::HeaderMap;
use std::path::PathBuf;
use tokio::process::Command;
use url::Url;

pub struct YtDlpResolver {
    custom_bin_path: Option<PathBuf>,
}

impl YtDlpResolver {
    pub fn new() -> Self {
        Self {
            custom_bin_path: None,
        }
    }
}

impl Default for YtDlpResolver {
    fn default() -> Self {
        Self::new()
    }
}

impl YtDlpResolver {

    pub fn with_bin_path(path: PathBuf) -> Self {
        Self {
            custom_bin_path: Some(path),
        }
    }

    fn get_binary_path(&self) -> String {
        if let Some(path) = &self.custom_bin_path {
            return path.to_string_lossy().to_string();
        }

        // Check local share path or PATH
        if let Some(home) = directories::UserDirs::new() {
            let local_bin = home.home_dir().join(".local/share/kestrel/bin/yt-dlp");
            if local_bin.exists() {
                return local_bin.to_string_lossy().to_string();
            }
        }

        "yt-dlp".to_string()
    }
}

#[async_trait]
impl LinkResolver for YtDlpResolver {
    fn priority(&self) -> u8 {
        50 // Fallback resolver
    }

    fn can_handle(&self, url: &Url) -> bool {
        let host = url.host_str().unwrap_or("").to_ascii_lowercase();
        host.contains("youtube.com") || host.contains("youtu.be") ||
        host.contains("vimeo.com") || host.contains("dailymotion.com") ||
        host.contains("twitch.tv") || host.contains("bilibili.com") ||
        host.contains("facebook.com")
    }

    async fn resolve(&self, url: &Url, _headers: &HeaderMap) -> Result<ResolvedMedia, KestrelError> {
        let bin = self.get_binary_path();
        
        let output = Command::new(&bin)
            .arg("--dump-single-json")
            .arg("--no-warnings")
            .arg("--no-playlist")
            .arg(url.as_str())
            .output()
            .await
            .map_err(|e| KestrelError::Sidecar(format!("Failed to execute yt-dlp: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(KestrelError::Sidecar(format!("yt-dlp error: {}", stderr)));
        }

        let json_val: serde_json::Value = serde_json::from_slice(&output.stdout)
            .map_err(|e| KestrelError::Protocol(format!("Invalid yt-dlp JSON output: {}", e)))?;

        let title = json_val.get("title")
            .and_then(|t| t.as_str())
            .unwrap_or("Media Download")
            .to_string();

        let mut variants = Vec::new();
        if let Some(formats) = json_val.get("formats").and_then(|f| f.as_array()) {
            for f in formats {
                let f_id = f.get("format_id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let ext = f.get("ext").and_then(|v| v.as_str()).unwrap_or("mp4");
                let direct_url = f.get("url").and_then(|v| v.as_str()).map(|s| s.to_string());
                let resolution = f.get("resolution").and_then(|v| v.as_str()).map(|s| s.to_string());
                let filesize = f.get("filesize").and_then(|v| v.as_u64())
                    .or_else(|| f.get("filesize_approx").and_then(|v| v.as_u64()));
                let vcodec = f.get("vcodec").and_then(|v| v.as_str()).unwrap_or("none");
                let acodec = f.get("acodec").and_then(|v| v.as_str()).unwrap_or("none");

                if direct_url.is_none() {
                    continue;
                }

                let is_video = vcodec != "none";
                let _is_audio = acodec != "none";

                let size_str = filesize.map(|s| format!(" · {:.1} MB", s as f64 / 1024.0 / 1024.0)).unwrap_or_default();
                let res_str = resolution.clone().unwrap_or_else(|| if is_video { "Video".into() } else { "Audio".into() });
                let label = format!("{} {} ({}){}", res_str, ext.to_uppercase(), f_id, size_str);

                variants.push(MediaVariant {
                    id: format!("{}_{}", f_id, ext),
                    label,
                    format_id: f_id,
                    resolution,
                    filesize_estimate: filesize,
                    direct_url,
                    audio_url: None,
                });
            }
        }

        let direct = variants.last().and_then(|v| v.direct_url.clone());

        Ok(ResolvedMedia {
            title,
            mime_category: MimeCategory::Video,
            variants,
            direct_download_url: direct,
            audio_url: None,
            is_hls_stream: false,
            hls_segments: Vec::new(),
        })
    }
}
