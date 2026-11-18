//! Slint GPU UI view-model binding and desktop capsule renderer for Kestrel.

use kestrel_proto::{DownloadCardSnapshot, DownloadStatus, IpcRequest};

/// Segment data for the 8-thread vertical equalizer visualization.
#[derive(Debug, Clone, PartialEq)]
pub struct SegmentViewModel {
    pub index: i32,
    pub fill_ratio: f32,
    pub active: bool,
}

/// Download item model representing either an active or completed card.
#[derive(Debug, Clone, PartialEq)]
pub struct DownloadItemViewModel {
    pub id: String,
    pub title: String,
    pub dest_path: String,
    pub status: String,
    pub progress: f32,
    pub telemetry: String,
    pub is_active: bool,
    pub is_expanded: bool,
    pub is_completed: bool,
    pub sha256_verified: bool,
    pub segments: Vec<SegmentViewModel>,
}

impl DownloadItemViewModel {
    /// Constructs a view model from a proto snapshot matching the desktop capsule design.
    pub fn from_snapshot(snapshot: &DownloadCardSnapshot) -> Self {
        let is_completed = matches!(snapshot.status, DownloadStatus::Done);
        let is_active = snapshot.status.is_in_progress();

        let progress = match snapshot.total_bytes {
            Some(total) if total > 0 => (snapshot.downloaded_bytes as f32 / total as f32).clamp(0.0, 1.0),
            _ => {
                if is_completed {
                    1.0
                } else {
                    0.0
                }
            }
        };

        // Format telemetry: e.g. "142.8 MB/s · 18s left · 4.8 GB"
        let telemetry = if is_completed {
            format!("Completed · {}", snapshot.formatted_size())
        } else {
            let speed = snapshot.formatted_speed();
            let eta = snapshot.formatted_eta();
            let size = snapshot.formatted_size();
            format!("{} · {} · {}", speed, eta, size)
        };

        // Normalize or populate 8 thread segments for vertical equalizer columns
        let mut segments = Vec::with_capacity(8);
        if !snapshot.segments.is_empty() {
            for (i, slice) in snapshot.segments.iter().enumerate().take(8) {
                segments.push(SegmentViewModel {
                    index: i as i32,
                    fill_ratio: slice.fill_ratio(),
                    active: slice.active_worker,
                });
            }
        }
        // Ensure 8 segments are present for the 8-thread UI layout
        while segments.len() < 8 {
            let idx = segments.len() as i32;
            let ratio = if is_completed { 1.0 } else { 0.0 };
            segments.push(SegmentViewModel {
                index: idx,
                fill_ratio: ratio,
                active: is_active,
            });
        }

        Self {
            id: snapshot.id.clone(),
            title: snapshot.title.clone(),
            dest_path: snapshot.dest_path.clone(),
            status: format!("{:?}", snapshot.status),
            progress,
            telemetry,
            is_active,
            is_expanded: is_active,
            is_completed,
            sha256_verified: snapshot.sha256_verified.unwrap_or(false),
            segments,
        }
    }
}

/// State controller for the floating capsule desktop UI.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct DesktopUiState {
    pub items: Vec<DownloadItemViewModel>,
    pub url_input: String,
}

impl DesktopUiState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Synchronizes UI state with daemon snapshots.
    pub fn update_from_snapshots(&mut self, snapshots: &[DownloadCardSnapshot]) {
        self.items = snapshots.iter().map(DownloadItemViewModel::from_snapshot).collect();
    }

    /// Handles toggling pause/resume on a download item.
    pub fn toggle_pause(&mut self, id: &str) -> Option<IpcRequest> {
        if let Some(item) = self.items.iter_mut().find(|it| it.id == id) {
            item.is_active = !item.is_active;
            if item.is_active {
                Some(IpcRequest::Resume { id: id.to_string() })
            } else {
                Some(IpcRequest::Pause { id: id.to_string() })
            }
        } else {
            None
        }
    }

    /// Toggles the expandable Segments section for an active download.
    pub fn toggle_expand(&mut self, id: &str) {
        if let Some(item) = self.items.iter_mut().find(|it| it.id == id) {
            item.is_expanded = !item.is_expanded;
        }
    }

    /// Enqueues a new download URL from the header pill input.
    pub fn enqueue_url(&mut self, url: String) -> Option<IpcRequest> {
        let trimmed = url.trim();
        if trimmed.is_empty() {
            return None;
        }
        self.url_input.clear();
        Some(IpcRequest::EnqueueUrl {
            url: trimmed.to_string(),
            headers: Vec::new(),
            dest_dir: None,
            auto_start: true,
        })
    }
}

/// Slint desktop renderer entrypoint.
pub fn render_ui(snapshots: &[DownloadCardSnapshot]) {
    let mut state = DesktopUiState::new();
    state.update_from_snapshots(snapshots);
    tracing::info!("Rendered Slint desktop UI with {} items", state.items.len());
}

#[cfg(test)]
mod tests {
    use super::*;
    use kestrel_proto::{MimeCategory, SegmentSlice};

    #[test]
    fn test_active_card_view_model_mapping() {
        let snapshot = DownloadCardSnapshot {
            id: "01ARZ3NDEKTSV4RRFFQ69G5FAV".to_string(),
            title: "ubuntu-26.04-desktop-amd64.iso".to_string(),
            dest_path: "~/Downloads".to_string(),
            mime_category: MimeCategory::Binary,
            status: DownloadStatus::Active,
            total_bytes: Some(4_800_000_000),
            downloaded_bytes: 2_000_000_000,
            speed_bps: 142_800_000,
            eta_seconds: Some(18),
            sha256_verified: Some(true),
            quality_options: vec![],
            selected_variant_id: None,
            segments: vec![
                SegmentSlice {
                    index: 0,
                    start_byte: 0,
                    end_byte: 600_000_000,
                    filled_bytes: 550_000_000,
                    active_worker: true,
                },
                SegmentSlice {
                    index: 1,
                    start_byte: 600_000_000,
                    end_byte: 1_200_000_000,
                    filled_bytes: 190_000_000,
                    active_worker: true,
                },
            ],
            error_summary: None,
        };

        let vm = DownloadItemViewModel::from_snapshot(&snapshot);
        assert_eq!(vm.title, "ubuntu-26.04-desktop-amd64.iso");
        assert!(vm.is_active);
        assert!(!vm.is_completed);
        assert!(vm.sha256_verified);
        assert_eq!(vm.segments.len(), 8);
        assert!(vm.telemetry.contains("MB/s"));
        assert!(vm.telemetry.contains("18s left"));
    }

    #[test]
    fn test_completed_card_view_model_mapping() {
        let snapshot = DownloadCardSnapshot {
            id: "01ARZ3NDEKTSV4RRFFQ69G5FB1".to_string(),
            title: "Ableton_Live_12_Suite.dmg".to_string(),
            dest_path: "~/Downloads".to_string(),
            mime_category: MimeCategory::Binary,
            status: DownloadStatus::Done,
            total_bytes: Some(2_400_000_000),
            downloaded_bytes: 2_400_000_000,
            speed_bps: 0,
            eta_seconds: None,
            sha256_verified: Some(true),
            quality_options: vec![],
            selected_variant_id: None,
            segments: vec![],
            error_summary: None,
        };

        let vm = DownloadItemViewModel::from_snapshot(&snapshot);
        assert_eq!(vm.title, "Ableton_Live_12_Suite.dmg");
        assert!(!vm.is_active);
        assert!(vm.is_completed);
        assert_eq!(vm.progress, 1.0);
    }

    #[test]
    fn test_desktop_ui_state_interactions() {
        let mut state = DesktopUiState::new();
        let snapshot = DownloadCardSnapshot {
            id: "dl-123".to_string(),
            title: "ubuntu.iso".to_string(),
            dest_path: "~/Downloads".to_string(),
            mime_category: MimeCategory::Binary,
            status: DownloadStatus::Active,
            total_bytes: Some(1000),
            downloaded_bytes: 500,
            speed_bps: 100,
            eta_seconds: Some(5),
            sha256_verified: Some(true),
            quality_options: vec![],
            selected_variant_id: None,
            segments: vec![],
            error_summary: None,
        };

        state.update_from_snapshots(&[snapshot]);
        assert_eq!(state.items.len(), 1);

        // Toggle pause
        let req = state.toggle_pause("dl-123");
        assert!(matches!(req, Some(IpcRequest::Pause { id }) if id == "dl-123"));

        // Toggle expand
        state.toggle_expand("dl-123");
        assert!(!state.items[0].is_expanded);

        // Enqueue URL
        let req = state.enqueue_url("https://example.com/file.zip".to_string());
        assert!(matches!(req, Some(IpcRequest::EnqueueUrl { url, .. }) if url == "https://example.com/file.zip"));
    }
}
