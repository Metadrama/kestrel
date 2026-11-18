use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MimeCategory {
    Archive,
    Video,
    Audio,
    Document,
    Binary,
    Image,
    Other,
}

impl MimeCategory {
    pub fn from_mime_or_ext(mime: Option<&str>, ext: Option<&str>) -> Self {
        if let Some(m) = mime {
            let m_lower = m.to_ascii_lowercase();
            if m_lower.starts_with("video/") {
                return MimeCategory::Video;
            }
            if m_lower.starts_with("audio/") {
                return MimeCategory::Audio;
            }
            if m_lower.starts_with("image/") {
                return MimeCategory::Image;
            }
            if m_lower.contains("pdf") || m_lower.contains("document") || m_lower.contains("text/") {
                return MimeCategory::Document;
            }
            if m_lower.contains("zip") || m_lower.contains("tar") || m_lower.contains("gzip") || m_lower.contains("7z") || m_lower.contains("rar") {
                return MimeCategory::Archive;
            }
        }

        if let Some(e) = ext {
            let e_lower = e.trim_start_matches('.').to_ascii_lowercase();
            match e_lower.as_str() {
                "mp4" | "mkv" | "webm" | "mov" | "avi" | "flv" | "ts" | "m4v" => MimeCategory::Video,
                "mp3" | "m4a" | "aac" | "flac" | "wav" | "ogg" | "opus" => MimeCategory::Audio,
                "jpg" | "jpeg" | "png" | "gif" | "webp" | "svg" | "bmp" => MimeCategory::Image,
                "pdf" | "doc" | "docx" | "txt" | "epub" | "rtf" | "odt" | "csv" => MimeCategory::Document,
                "zip" | "tar" | "gz" | "bz2" | "xz" | "7z" | "rar" | "zst" | "tgz" => MimeCategory::Archive,
                "exe" | "msi" | "apk" | "dmg" | "iso" | "bin" | "deb" | "rpm" | "AppImage" => MimeCategory::Binary,
                _ => MimeCategory::Other,
            }
        } else {
            MimeCategory::Other
        }
    }

    pub fn badge_symbol(&self) -> &'static str {
        match self {
            MimeCategory::Video => "VID",
            MimeCategory::Audio => "AUD",
            MimeCategory::Archive => "ZIP",
            MimeCategory::Document => "DOC",
            MimeCategory::Binary => "BIN",
            MimeCategory::Image => "IMG",
            MimeCategory::Other => "FILE",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DownloadStatus {
    Resolving,
    Queued,
    Active,
    Paused,
    Muxing,
    Verifying,
    Done,
    Error,
}

impl DownloadStatus {
    pub fn is_terminal(&self) -> bool {
        matches!(self, DownloadStatus::Done | DownloadStatus::Error)
    }

    pub fn is_in_progress(&self) -> bool {
        matches!(self, DownloadStatus::Active | DownloadStatus::Resolving | DownloadStatus::Muxing | DownloadStatus::Verifying)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaVariant {
    pub id: String,
    pub label: String,
    pub format_id: String,
    pub resolution: Option<String>,
    pub filesize_estimate: Option<u64>,
    pub direct_url: Option<String>,
    pub audio_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SegmentSlice {
    pub index: u32,
    pub start_byte: u64,
    pub end_byte: u64,
    pub filled_bytes: u64,
    pub active_worker: bool,
}

impl SegmentSlice {
    pub fn fill_ratio(&self) -> f32 {
        let total = self.end_byte.saturating_sub(self.start_byte);
        if total == 0 {
            1.0
        } else {
            (self.filled_bytes as f32 / total as f32).clamp(0.0, 1.0)
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadCardSnapshot {
    pub id: String,                      // ULID
    pub title: String,                   // Sanitized display filename
    pub dest_path: String,
    pub mime_category: MimeCategory,     // Archive | Video | Audio | Document | Binary | Image | Other
    pub status: DownloadStatus,          // Resolving | Queued | Active | Paused | Muxing | Verifying | Done | Error
    pub total_bytes: Option<u64>,
    pub downloaded_bytes: u64,
    pub speed_bps: u64,                  // Exponentially smoothed (alpha = 0.25)
    pub eta_seconds: Option<u64>,
    pub sha256_verified: Option<bool>,
    pub quality_options: Vec<MediaVariant>, // Populated for YT/X/IG/HLS links (e.g., "1080p MP4 · 42 MB")
    pub selected_variant_id: Option<String>,
    /// Ordered contiguous byte-range slices for the linear multi-segment bar (0.0..=1.0 fill each)
    pub segments: Vec<SegmentSlice>,
    pub error_summary: Option<String>,   // Concise 1-line human-readable error
}

impl DownloadCardSnapshot {
    pub fn formatted_speed(&self) -> String {
        let bps = self.speed_bps as f64;
        if bps >= 1024.0 * 1024.0 * 1024.0 {
            format!("{:.1} GB/s", bps / (1024.0 * 1024.0 * 1024.0))
        } else if bps >= 1024.0 * 1024.0 {
            format!("{:.1} MB/s", bps / (1024.0 * 1024.0))
        } else if bps >= 1024.0 {
            format!("{:.1} KB/s", bps / 1024.0)
        } else {
            format!("{} B/s", self.speed_bps)
        }
    }

    pub fn formatted_size(&self) -> String {
        let cur = self.downloaded_bytes as f64;
        match self.total_bytes {
            Some(tot) => {
                let tot_f = tot as f64;
                if tot_f >= 1024.0 * 1024.0 * 1024.0 {
                    format!("{:.1} / {:.1} GB", cur / 1024.0 / 1024.0 / 1024.0, tot_f / 1024.0 / 1024.0 / 1024.0)
                } else if tot_f >= 1024.0 * 1024.0 {
                    format!("{:.1} / {:.1} MB", cur / 1024.0 / 1024.0, tot_f / 1024.0 / 1024.0)
                } else {
                    format!("{:.0} / {:.0} KB", cur / 1024.0, tot_f / 1024.0)
                }
            }
            None => {
                if cur >= 1024.0 * 1024.0 {
                    format!("{:.1} MB", cur / 1024.0 / 1024.0)
                } else {
                    format!("{:.0} KB", cur / 1024.0)
                }
            }
        }
    }

    pub fn formatted_eta(&self) -> String {
        match self.eta_seconds {
            Some(sec) if sec > 0 => {
                if sec >= 3600 {
                    format!("{}h {}m left", sec / 3600, (sec % 3600) / 60)
                } else if sec >= 60 {
                    format!("{}m {}s left", sec / 60, sec % 60)
                } else {
                    format!("{}s left", sec)
                }
            }
            _ => "--".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IpcRequest {
    EnqueueUrl {
        url: String,
        headers: Vec<(String, String)>,  // Cookies, Referer, User-Agent from browser extension
        dest_dir: Option<String>,
        auto_start: bool,
    },
    Pause { id: String },
    Resume { id: String },
    Cancel { id: String, delete_file: bool },
    SelectMediaVariant { id: String, variant_id: String },
    InstallSidecarYtDlp,
    SubscribeSnapshots,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IpcResponse {
    Ok,
    Enqueued { id: String },
    Snapshots(Vec<DownloadCardSnapshot>),
    Error(String),
}

#[derive(thiserror::Error, Debug, Clone, Serialize, Deserialize)]
pub enum KestrelError {
    #[error("I/O error: {0}")]
    Io(String),

    #[error("Network error: {0}")]
    Network(String),

    #[error("Protocol error: {0}")]
    Protocol(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Muxer error: {0}")]
    Muxer(String),

    #[error("Sidecar error: {0}")]
    Sidecar(String),

    #[error("Store error: {0}")]
    Store(String),

    #[error("Source file changed on server: {0}")]
    SourceChanged(String),

    #[error("Cancelled")]
    Cancelled,
}
