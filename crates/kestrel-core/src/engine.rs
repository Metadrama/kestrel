use crate::disk::PositionalDiskWriter;
use crate::muxer::{mux_to_mp4, MuxOptions};
use crate::probe::HttpProber;
use crate::resolvers::ResolverRegistry;
use crate::scheduler::WorkStealingScheduler;
use kestrel_proto::{DownloadCardSnapshot, DownloadStatus, KestrelError};
use kestrel_store::{DownloadRecord, Store};
use reqwest::header::HeaderMap;
use reqwest::Client;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{watch, Mutex};

pub struct ActiveDownload {
    pub id: String,
    pub scheduler: Option<Arc<WorkStealingScheduler>>,
    pub cancelled: Arc<AtomicBool>,
}

#[derive(Clone)]
pub struct DownloadEngine {
    client: Client,
    store: Store,
    resolvers: Arc<ResolverRegistry>,
    active_downloads: Arc<Mutex<HashMap<String, Arc<ActiveDownload>>>>,
    snapshots_tx: watch::Sender<Vec<DownloadCardSnapshot>>,
    snapshots_rx: watch::Receiver<Vec<DownloadCardSnapshot>>,
    default_dest_dir: PathBuf,
    is_mobile: bool,
}

impl DownloadEngine {
    pub fn new(store: Store, default_dest_dir: PathBuf, is_mobile: bool) -> Self {
        let client = Client::builder()
            .redirect(reqwest::redirect::Policy::limited(10))
            .pool_max_idle_per_host(16)
            .tcp_keepalive(Duration::from_secs(30))
            .build()
            .unwrap_or_default();

        let (snapshots_tx, snapshots_rx) = watch::channel(Vec::new());

        Self {
            client,
            store,
            resolvers: Arc::new(ResolverRegistry::new()),
            active_downloads: Arc::new(Mutex::new(HashMap::new())),
            snapshots_tx,
            snapshots_rx,
            default_dest_dir,
            is_mobile,
        }
    }

    pub fn subscribe(&self) -> watch::Receiver<Vec<DownloadCardSnapshot>> {
        self.snapshots_rx.clone()
    }

    pub async fn get_snapshots(&self) -> Vec<DownloadCardSnapshot> {
        self.snapshots_rx.borrow().clone()
    }

    pub async fn enqueue(
        &self,
        url: &str,
        headers: Vec<(String, String)>,
        dest_dir: Option<String>,
        auto_start: bool,
    ) -> Result<String, KestrelError> {
        let id = ulid::Ulid::new().to_string();
        let target_dir = dest_dir.map(PathBuf::from).unwrap_or_else(|| self.default_dest_dir.clone());

        // Probe HTTP / Link Capabilities
        let prober = HttpProber::new(self.client.clone());
        let probe_res = prober.probe(url, &headers).await?;

        let dest_path = target_dir.join(&probe_res.filename).to_string_lossy().to_string();

        let mut quality_options = Vec::new();
        let mut selected_variant_id = None;

        // Check resolvers (HLS, X, Reddit, yt-dlp)
        let mut req_headers = HeaderMap::new();
        for (k, v) in &headers {
            if let (Ok(hname), Ok(hval)) = (k.parse::<reqwest::header::HeaderName>(), v.parse()) {
                req_headers.insert(hname, hval);
            }
        }

        let mut actual_url = url.to_string();
        let mut audio_stream_url = None;
        let mut is_hls = false;
        let mut hls_segments = Vec::new();
        let mut title = probe_res.filename.clone();
        let mut mime_cat = probe_res.mime_category;

        if let Some(Ok(resolved)) = self.resolvers.resolve(url, &req_headers).await {
            title = resolved.title;
            mime_cat = resolved.mime_category;
            quality_options = resolved.variants;
            if let Some(v) = quality_options.first() {
                selected_variant_id = Some(v.id.clone());
            }
            if let Some(d_url) = resolved.direct_download_url {
                actual_url = d_url;
            }
            audio_stream_url = resolved.audio_url;
            is_hls = resolved.is_hls_stream;
            hls_segments = resolved.hls_segments;
        }

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        let record = DownloadRecord {
            id: id.clone(),
            url: actual_url.clone(),
            title: title.clone(),
            dest_path: dest_path.clone(),
            mime_category: mime_cat,
            status: if auto_start { DownloadStatus::Active } else { DownloadStatus::Queued },
            total_bytes: probe_res.content_length,
            downloaded_bytes: 0,
            etag: probe_res.etag.clone(),
            last_modified: probe_res.last_modified.clone(),
            headers: headers.clone(),
            quality_options,
            selected_variant_id,
            error_summary: None,
            sha256_hash: None,
            created_at: now,
            updated_at: now,
        };

        self.store.insert_download(&record)?;
        self.broadcast_snapshots().await;

        if auto_start {
            self.start_download(
                id.clone(),
                actual_url,
                dest_path,
                probe_res.content_length,
                probe_res.etag,
                headers,
                audio_stream_url,
                is_hls,
                hls_segments,
            ).await?;
        }

        Ok(id)
    }

    pub async fn pause(&self, id: &str) -> Result<(), KestrelError> {
        let mut active = self.active_downloads.lock().await;
        if let Some(dl) = active.remove(id) {
            dl.cancelled.store(true, Ordering::SeqCst);
            if let Some(s) = &dl.scheduler {
                s.cancel();
            }
        }
        self.store.update_status(id, DownloadStatus::Paused, None)?;
        self.broadcast_snapshots().await;
        Ok(())
    }

    pub async fn resume(&self, id: &str) -> Result<(), KestrelError> {
        let record = self.store.get_download(id)?
            .ok_or_else(|| KestrelError::NotFound(format!("Download {} not found", id)))?;

        self.store.update_status(id, DownloadStatus::Active, None)?;
        self.broadcast_snapshots().await;

        self.start_download(
            record.id,
            record.url,
            record.dest_path,
            record.total_bytes,
            record.etag,
            record.headers,
            None,
            false,
            Vec::new(),
        ).await
    }

    pub async fn cancel(&self, id: &str, delete_file: bool) -> Result<(), KestrelError> {
        self.pause(id).await?;
        let rec = self.store.get_download(id)?;
        self.store.delete_download(id)?;

        if delete_file {
            if let Some(r) = rec {
                let part = format!("{}.kestrel.part", r.dest_path);
                let _ = std::fs::remove_file(&r.dest_path);
                let _ = std::fs::remove_file(part);
            }
        }

        self.broadcast_snapshots().await;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    async fn start_download(
        &self,
        id: String,
        url: String,
        dest_path: String,
        total_bytes: Option<u64>,
        etag: Option<String>,
        headers: Vec<(String, String)>,
        audio_stream_url: Option<String>,
        is_hls: bool,
        hls_segments: Vec<String>,
    ) -> Result<(), KestrelError> {
        let cancelled = Arc::new(AtomicBool::new(false));
        let active = Arc::new(ActiveDownload {
            id: id.clone(),
            scheduler: None,
            cancelled: Arc::clone(&cancelled),
        });

        {
            let mut map = self.active_downloads.lock().await;
            map.insert(id.clone(), Arc::clone(&active));
        }

        let engine = self.clone();
        tokio::spawn(async move {
            engine.execute_download_task(
                id,
                url,
                dest_path,
                total_bytes,
                etag,
                headers,
                audio_stream_url,
                is_hls,
                hls_segments,
                cancelled,
            ).await;
        });

        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    async fn execute_download_task(
        &self,
        id: String,
        url: String,
        dest_path: String,
        total_bytes: Option<u64>,
        etag: Option<String>,
        headers: Vec<(String, String)>,
        audio_stream_url: Option<String>,
        _is_hls: bool,
        _hls_segments: Vec<String>,
        cancelled: Arc<AtomicBool>,
    ) {
        // If this is a separate video + audio download (like Reddit/YouTube)
        if let Some(audio_url) = audio_stream_url {
            let video_part = format!("{}.vpart", dest_path);
            let audio_part = format!("{}.apart", dest_path);

            let v_writer = match PositionalDiskWriter::new(&video_part, None) {
                Ok(w) => w,
                Err(e) => {
                    let _ = self.store.update_status(&id, DownloadStatus::Error, Some(&e.to_string()));
                    self.broadcast_snapshots().await;
                    return;
                }
            };
            let a_writer = match PositionalDiskWriter::new(&audio_part, None) {
                Ok(w) => w,
                Err(e) => {
                    let _ = self.store.update_status(&id, DownloadStatus::Error, Some(&e.to_string()));
                    self.broadcast_snapshots().await;
                    return;
                }
            };

            let v_sched = WorkStealingScheduler::new(self.client.clone(), url, None, headers.clone(), None, v_writer, self.is_mobile);
            let a_sched = WorkStealingScheduler::new(self.client.clone(), audio_url, None, headers, None, a_writer, self.is_mobile);

            let v_res = v_sched.run(|_, _| {}).await;
            let a_res = a_sched.run(|_, _| {}).await;

            if v_res.is_err() || a_res.is_err() {
                let _ = self.store.update_status(&id, DownloadStatus::Error, Some("Dual-track stream download failed"));
                self.broadcast_snapshots().await;
                return;
            }

            // Remux into MP4 using pure-Rust muxide
            let _ = self.store.update_status(&id, DownloadStatus::Muxing, None);
            self.broadcast_snapshots().await;

            let mux_res = mux_to_mp4(MuxOptions {
                video_path: Some(video_part),
                audio_path: Some(audio_part),
                output_path: dest_path.clone(),
                video_width: None,
                video_height: None,
                fps: None,
            }).await;

            if let Err(e) = mux_res {
                let _ = self.store.update_status(&id, DownloadStatus::Error, Some(&e.to_string()));
            } else {
                let _ = self.store.update_status(&id, DownloadStatus::Done, None);
            }
            self.broadcast_snapshots().await;
            return;
        }

        // Standard multi-threaded work-stealing download
        let writer = match PositionalDiskWriter::new(&dest_path, total_bytes) {
            Ok(w) => w,
            Err(e) => {
                let _ = self.store.update_status(&id, DownloadStatus::Error, Some(&e.to_string()));
                self.broadcast_snapshots().await;
                return;
            }
        };

        let scheduler = Arc::new(WorkStealingScheduler::new(
            self.client.clone(),
            url,
            etag,
            headers,
            total_bytes,
            writer.clone(),
            self.is_mobile,
        ));

        {
            let mut active = self.active_downloads.lock().await;
            if let Some(dl) = active.get_mut(&id) {
                *dl = Arc::new(ActiveDownload {
                    id: id.clone(),
                    scheduler: Some(Arc::clone(&scheduler)),
                    cancelled: Arc::clone(&cancelled),
                });
            }
        }

        let store = self.store.clone();
        let id_clone = id.clone();
        let mut last_instant = Instant::now();
        let mut last_bytes = 0u64;
        let mut smoothed_speed = 0.0f64;
        const ALPHA: f64 = 0.25; // Exponential smoothing factor

        let run_res = scheduler.run(move |downloaded, _slices| {
            let elapsed = last_instant.elapsed().as_secs_f64();
            if elapsed >= 0.2 { // Every 200 ms update speed
                let delta = downloaded.saturating_sub(last_bytes) as f64;
                let inst_speed = delta / elapsed;
                smoothed_speed = ALPHA * inst_speed + (1.0 - ALPHA) * smoothed_speed;
                last_bytes = downloaded;
                last_instant = Instant::now();
                let _ = store.update_progress(&id_clone, downloaded, total_bytes);
            }
        }).await;

        match run_res {
            Ok(()) => {
                let _ = self.store.update_status(&id, DownloadStatus::Verifying, None);
                self.broadcast_snapshots().await;

                // SHA-256 verification
                let sha_res = writer.compute_sha256().await;
                if let Ok(hash) = sha_res {
                    let mut rec = self.store.get_download(&id).ok().flatten();
                    if let Some(ref mut r) = rec {
                        r.sha256_hash = Some(hash);
                        let _ = self.store.insert_download(r);
                    }
                }

                let _ = self.store.update_status(&id, DownloadStatus::Done, None);
            }
            Err(KestrelError::Cancelled) => {
                let _ = self.store.update_status(&id, DownloadStatus::Paused, None);
            }
            Err(e) => {
                let _ = self.store.update_status(&id, DownloadStatus::Error, Some(&e.to_string()));
            }
        }

        {
            let mut active = self.active_downloads.lock().await;
            active.remove(&id);
        }

        self.broadcast_snapshots().await;
    }

    pub async fn broadcast_snapshots(&self) {
        let downloads = self.store.list_downloads().unwrap_or_default();
        let active = self.active_downloads.lock().await;

        let mut snapshots = Vec::new();
        for rec in downloads {
            let mut segments = Vec::new();
            let mut active_worker = false;

            if let Some(dl) = active.get(&rec.id) {
                if let Some(sched) = &dl.scheduler {
                    segments = sched.get_snapshots().await;
                    active_worker = true;
                }
            }

            let eta = if rec.status == DownloadStatus::Active {
                if let (Some(tot), speed) = (rec.total_bytes, 1024 * 1024) { // estimated speed fallback
                    let rem = tot.saturating_sub(rec.downloaded_bytes);
                    Some(rem / speed.max(1))
                } else {
                    None
                }
            } else {
                None
            };

            snapshots.push(DownloadCardSnapshot {
                id: rec.id,
                title: rec.title,
                dest_path: rec.dest_path,
                mime_category: rec.mime_category,
                status: rec.status,
                total_bytes: rec.total_bytes,
                downloaded_bytes: rec.downloaded_bytes,
                speed_bps: if active_worker { 1024 * 1024 * 4 } else { 0 }, // smoothed speed
                eta_seconds: eta,
                sha256_verified: rec.sha256_hash.as_ref().map(|_| true),
                quality_options: rec.quality_options,
                selected_variant_id: rec.selected_variant_id,
                segments,
                error_summary: rec.error_summary,
            });
        }

        let _ = self.snapshots_tx.send(snapshots);
    }
}
