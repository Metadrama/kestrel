use bytes::Bytes;
use http_body_util::combinators::BoxBody;
use http_body_util::{BodyExt, Full};
use hyper::header::{ACCEPT_RANGES, CONTENT_LENGTH, CONTENT_RANGE, CONTENT_TYPE, ETAG, RANGE};
use hyper::service::service_fn;
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use hyper_util::server::conn::auto;
use sha2::{Digest, Sha256};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;

pub struct MockHttpServer {
    pub addr: SocketAddr,
    pub payload: Vec<u8>,
    pub etag: String,
    pub simulate_stall: Arc<AtomicBool>,
    pub request_count: Arc<AtomicUsize>,
}

impl MockHttpServer {
    pub async fn start(payload_size: usize) -> Arc<Self> {
        let mut payload = Vec::with_capacity(payload_size);
        for i in 0..payload_size {
            payload.push((i % 251) as u8);
        }

        let mut hasher = Sha256::new();
        hasher.update(&payload);
        let etag = format!("{:x}", hasher.finalize());

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server = Arc::new(Self {
            addr,
            payload,
            etag,
            simulate_stall: Arc::new(AtomicBool::new(false)),
            request_count: Arc::new(AtomicUsize::new(0)),
        });

        let server_clone = Arc::clone(&server);
        tokio::spawn(async move {
            loop {
                if let Ok((stream, _)) = listener.accept().await {
                    let s_ref = Arc::clone(&server_clone);
                    tokio::spawn(async move {
                        let io = TokioIo::new(stream);
                        let service = service_fn(move |req| {
                            let s = Arc::clone(&s_ref);
                            async move { s.handle_request(req).await }
                        });
                        let _ = auto::Builder::new(hyper_util::rt::TokioExecutor::new())
                            .serve_connection(io, service)
                            .await;
                    });
                }
            }
        });

        server
    }

    pub fn url(&self) -> String {
        format!("http://{}/testfile.bin", self.addr)
    }

    async fn handle_request(
        &self,
        req: Request<hyper::body::Incoming>,
    ) -> Result<Response<BoxBody<Bytes, std::convert::Infallible>>, std::convert::Infallible> {
        self.request_count.fetch_add(1, Ordering::Relaxed);

        if self.simulate_stall.load(Ordering::Relaxed) {
            tokio::time::sleep(Duration::from_millis(500)).await;
        }

        let total_len = self.payload.len();

        if req.method() == Method::HEAD {
            let res = Response::builder()
                .status(StatusCode::OK)
                .header(CONTENT_LENGTH, total_len.to_string())
                .header(ACCEPT_RANGES, "bytes")
                .header(ETAG, format!("\"{}\"", self.etag))
                .header(CONTENT_TYPE, "application/octet-stream")
                .header("Content-Disposition", "attachment; filename=\"testfile.bin\"")
                .body(Full::new(Bytes::new()).boxed())
                .unwrap();
            return Ok(res);
        }

        if let Some(range_header) = req.headers().get(RANGE) {
            if let Ok(r_str) = range_header.to_str() {
                if let Some(spec) = r_str.strip_prefix("bytes=") {
                    let parts: Vec<&str> = spec.split('-').collect();
                    let start = parts[0].parse::<usize>().unwrap_or(0);
                    let end = if parts.len() > 1 && !parts[1].is_empty() {
                        parts[1].parse::<usize>().unwrap_or(total_len - 1)
                    } else {
                        total_len - 1
                    };

                    let clamped_end = end.min(total_len - 1);
                    if start <= clamped_end {
                        let slice = &self.payload[start..=clamped_end];
                        let content_range = format!("bytes {}-{}/{}", start, clamped_end, total_len);

                        let res = Response::builder()
                            .status(StatusCode::PARTIAL_CONTENT)
                            .header(CONTENT_LENGTH, slice.len().to_string())
                            .header(CONTENT_RANGE, content_range)
                            .header(ACCEPT_RANGES, "bytes")
                            .header(ETAG, format!("\"{}\"", self.etag))
                            .body(Full::new(Bytes::copy_from_slice(slice)).boxed())
                            .unwrap();
                        return Ok(res);
                    }
                }
            }
        }

        // Full GET fallback
        let res = Response::builder()
            .status(StatusCode::OK)
            .header(CONTENT_LENGTH, total_len.to_string())
            .header(ACCEPT_RANGES, "bytes")
            .header(ETAG, format!("\"{}\"", self.etag))
            .body(Full::new(Bytes::copy_from_slice(&self.payload)).boxed())
            .unwrap();
        Ok(res)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kestrel_core::disk::PositionalDiskWriter;
    use kestrel_core::muxer::{mux_to_mp4, MuxOptions};
    use kestrel_core::probe::HttpProber;
    use kestrel_core::scheduler::WorkStealingScheduler;
    use kestrel_proto::{DownloadStatus, MimeCategory};
    use kestrel_store::{DownloadRecord, Store};

    #[tokio::test]
    async fn test_http_probe() {
        let server = MockHttpServer::start(1024 * 1024).await;
        let client = reqwest::Client::new();
        let prober = HttpProber::new(client);

        let probe = prober.probe(&server.url(), &[]).await.unwrap();
        assert_eq!(probe.content_length, Some(1024 * 1024));
        assert!(probe.accept_ranges);
        assert_eq!(probe.filename, "testfile.bin");
        assert_eq!(probe.mime_category, MimeCategory::Binary);
    }

    #[tokio::test]
    async fn test_work_stealing_scheduler_download() {
        let payload_size = 4 * 1024 * 1024; // 4 MB
        let server = MockHttpServer::start(payload_size).await;

        let temp_dir = tempfile::tempdir().unwrap();
        let target_file = temp_dir.path().join("downloaded.bin");

        let writer = PositionalDiskWriter::new(&target_file, Some(payload_size as u64)).unwrap();
        let client = reqwest::Client::new();

        let scheduler = WorkStealingScheduler::new(
            client,
            server.url(),
            Some(server.etag.clone()),
            vec![],
            Some(payload_size as u64),
            writer.clone(),
            false,
        );

        let res = scheduler.run(|_downloaded, _slices| {}).await;
        assert!(res.is_ok());

        let sha = writer.compute_sha256().await.unwrap();
        assert_eq!(sha, server.etag);
    }

    #[tokio::test]
    async fn test_sqlite_crash_recovery() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("test_store.db");
        let store = Store::open(&db_path).unwrap();

        let record = DownloadRecord {
            id: "01JREC0VER0000000000000001".to_string(),
            url: "http://example.com/file.zip".to_string(),
            title: "file.zip".to_string(),
            dest_path: temp_dir.path().join("file.zip").to_string_lossy().to_string(),
            mime_category: MimeCategory::Archive,
            status: DownloadStatus::Active,
            total_bytes: Some(10_000_000),
            downloaded_bytes: 4_000_000,
            etag: Some("test_etag".to_string()),
            last_modified: None,
            headers: vec![],
            quality_options: vec![],
            selected_variant_id: None,
            error_summary: None,
            sha256_hash: None,
            created_at: 1000,
            updated_at: 1000,
        };

        store.insert_download(&record).unwrap();

        // Simulate crash recovery
        let recovered = store.recover_on_startup().unwrap();
        assert_eq!(recovered, 1);

        let rec = store.get_download("01JREC0VER0000000000000001").unwrap().unwrap();
        assert_eq!(rec.status, DownloadStatus::Paused);
    }

    #[tokio::test]
    async fn test_pure_rust_mp4_muxing() {
        let temp_dir = tempfile::tempdir().unwrap();
        let v_file = temp_dir.path().join("test.h264");
        let a_file = temp_dir.path().join("test.aac");
        let out_file = temp_dir.path().join("out.mp4");

        // Write small placeholder elementary frames
        std::fs::write(&v_file, &[0x00, 0x00, 0x00, 0x01, 0x67, 0x42, 0x00, 0x1f]).unwrap();
        std::fs::write(&a_file, &[0xff, 0xf1, 0x50, 0x80, 0x00, 0x1f, 0xfc]).unwrap();

        let res = mux_to_mp4(MuxOptions {
            video_path: Some(v_file.to_string_lossy().to_string()),
            audio_path: Some(a_file.to_string_lossy().to_string()),
            output_path: out_file.to_string_lossy().to_string(),
            video_width: Some(1280),
            video_height: Some(720),
            fps: Some(30.0),
        }).await;

        assert!(res.is_ok());
        assert!(out_file.exists());
    }
}
