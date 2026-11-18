use crate::disk::{PositionalDiskWriter, CHUNK_COALESCE_SIZE};
use futures_util::StreamExt;
use kestrel_proto::{KestrelError, SegmentSlice};
use reqwest::header::{HeaderMap, HeaderValue, IF_RANGE, RANGE};
use reqwest::Client;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

const MIN_STEAL_SPAN: u64 = 512 * 1024; // 512 KB
const PAGE_ALIGN: u64 = 4096;           // 4 KB page alignment
const MAX_RETRIES: u32 = 8;
const BASE_BACKOFF_MS: u64 = 200;
const MAX_BACKOFF_MS: u64 = 10000;

#[derive(Debug)]
pub struct WorkerRange {
    pub index: u32,
    pub start_byte: u64,
    pub end_byte: Arc<AtomicU64>,
    pub current_byte: Arc<AtomicU64>,
    pub active: Arc<AtomicBool>,
}

impl WorkerRange {
    pub fn new(index: u32, start: u64, end: u64, current: u64) -> Self {
        Self {
            index,
            start_byte: start,
            end_byte: Arc::new(AtomicU64::new(end)),
            current_byte: Arc::new(AtomicU64::new(current)),
            active: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn to_slice(&self) -> SegmentSlice {
        let cur = self.current_byte.load(Ordering::Relaxed);
        let end = self.end_byte.load(Ordering::Relaxed);
        let filled = cur.saturating_sub(self.start_byte);
        SegmentSlice {
            index: self.index,
            start_byte: self.start_byte,
            end_byte: end,
            filled_bytes: filled,
            active_worker: self.active.load(Ordering::Relaxed),
        }
    }
}

pub struct WorkStealingScheduler {
    client: Client,
    url: String,
    etag: Option<String>,
    headers: Vec<(String, String)>,
    total_bytes: Option<u64>,
    workers: Arc<Mutex<Vec<Arc<WorkerRange>>>>,
    writer: PositionalDiskWriter,
    is_mobile: bool,
    cancelled: Arc<AtomicBool>,
}

impl WorkStealingScheduler {
    pub fn new(
        client: Client,
        url: String,
        etag: Option<String>,
        headers: Vec<(String, String)>,
        total_bytes: Option<u64>,
        writer: PositionalDiskWriter,
        is_mobile: bool,
    ) -> Self {
        Self {
            client,
            url,
            etag,
            headers,
            total_bytes,
            workers: Arc::new(Mutex::new(Vec::new())),
            writer,
            is_mobile,
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }

    pub async fn get_snapshots(&self) -> Vec<SegmentSlice> {
        let workers = self.workers.lock().await;
        workers.iter().map(|w| w.to_slice()).collect()
    }

    pub async fn run<F>(&self, mut progress_cb: F) -> Result<(), KestrelError>
    where
        F: FnMut(u64, Vec<SegmentSlice>) + Send + 'static,
    {
        let total = match self.total_bytes {
            Some(t) if t > 0 => t,
            _ => {
                // Fallback to single-stream resilient writer
                return self.run_single_stream(progress_cb).await;
            }
        };

        // Determine initial number of segments N
        let max_n = if self.is_mobile { 4 } else { 8 };
        let n = total.div_ceil(2 * 1024 * 1024).clamp(1, max_n) as u32;

        let seg_size = total / (n as u64);
        let mut initial_ranges = Vec::new();

        for i in 0..n {
            let start = i as u64 * seg_size;
            let end = if i == n - 1 { total } else { (i as u64 + 1) * seg_size };
            initial_ranges.push(Arc::new(WorkerRange::new(i, start, end, start)));
        }

        {
            let mut w = self.workers.lock().await;
            *w = initial_ranges.clone();
        }

        let mut join_set = tokio::task::JoinSet::new();

        for worker in initial_ranges {
            let client = self.client.clone();
            let url = self.url.clone();
            let etag = self.etag.clone();
            let custom_headers = self.headers.clone();
            let writer = self.writer.clone();
            let all_workers = Arc::clone(&self.workers);
            let cancelled = Arc::clone(&self.cancelled);

            join_set.spawn(async move {
                run_worker_loop(
                    client,
                    url,
                    etag,
                    custom_headers,
                    worker,
                    writer,
                    all_workers,
                    cancelled,
                ).await
            });
        }

        // Periodic progress reporter
        let workers_clone = Arc::clone(&self.workers);
        let cancelled_clone = Arc::clone(&self.cancelled);
        let ticker = tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(16)); // ~60 Hz
            loop {
                interval.tick().await;
                if cancelled_clone.load(Ordering::Relaxed) {
                    break;
                }
                let w = workers_clone.lock().await;
                let mut downloaded = 0u64;
                let mut slices = Vec::with_capacity(w.len());
                let mut all_done = true;
                for item in w.iter() {
                    let cur = item.current_byte.load(Ordering::Relaxed);
                    let end = item.end_byte.load(Ordering::Relaxed);
                    downloaded += cur.saturating_sub(item.start_byte);
                    slices.push(item.to_slice());
                    if cur < end {
                        all_done = false;
                    }
                }
                progress_cb(downloaded, slices);
                if all_done {
                    break;
                }
            }
        });

        // Await all workers
        let mut first_error = None;
        while let Some(res) = join_set.join_next().await {
            match res {
                Ok(Ok(())) => {}
                Ok(Err(e)) => {
                    if first_error.is_none() {
                        first_error = Some(e);
                    }
                }
                Err(e) => {
                    if first_error.is_none() {
                        first_error = Some(KestrelError::Network(e.to_string()));
                    }
                }
            }
        }

        let _ = ticker.await;

        if let Some(err) = first_error {
            return Err(err);
        }

        if self.cancelled.load(Ordering::SeqCst) {
            return Err(KestrelError::Cancelled);
        }

        // Finalize disk write
        self.writer.finalize().await?;
        Ok(())
    }

    async fn run_single_stream<F>(&self, mut progress_cb: F) -> Result<(), KestrelError>
    where
        F: FnMut(u64, Vec<SegmentSlice>) + Send + 'static,
    {
        let mut req_headers = HeaderMap::new();
        for (k, v) in &self.headers {
            if let (Ok(hname), Ok(hval)) = (k.parse::<reqwest::header::HeaderName>(), HeaderValue::from_str(v)) {
                req_headers.insert(hname, hval);
            }
        }

        let response = self.client.get(&self.url)
            .headers(req_headers)
            .send()
            .await
            .map_err(|e| KestrelError::Network(e.to_string()))?;

        if !response.status().is_success() {
            return Err(KestrelError::Network(format!("Server returned HTTP {}", response.status())));
        }

        let mut stream = response.bytes_stream();
        let mut offset = 0u64;
        let mut buffer = Vec::with_capacity(CHUNK_COALESCE_SIZE);

        while let Some(chunk_res) = stream.next().await {
            if self.cancelled.load(Ordering::Relaxed) {
                return Err(KestrelError::Cancelled);
            }

            let chunk = chunk_res.map_err(|e| KestrelError::Network(e.to_string()))?;
            buffer.extend_from_slice(&chunk);

            if buffer.len() >= CHUNK_COALESCE_SIZE {
                let to_write = std::mem::replace(&mut buffer, Vec::with_capacity(CHUNK_COALESCE_SIZE));
                let len = to_write.len() as u64;
                self.writer.write_at(offset, to_write).await?;
                offset += len;
                progress_cb(offset, vec![SegmentSlice {
                    index: 0,
                    start_byte: 0,
                    end_byte: offset,
                    filled_bytes: offset,
                    active_worker: true,
                }]);
            }
        }

        if !buffer.is_empty() {
            let len = buffer.len() as u64;
            self.writer.write_at(offset, buffer).await?;
            offset += len;
            progress_cb(offset, vec![SegmentSlice {
                index: 0,
                start_byte: 0,
                end_byte: offset,
                filled_bytes: offset,
                active_worker: false,
            }]);
        }

        self.writer.finalize().await?;
        Ok(())
    }
}

#[allow(clippy::too_many_arguments)]
async fn run_worker_loop(
    client: Client,
    url: String,
    etag: Option<String>,
    headers: Vec<(String, String)>,
    mut worker: Arc<WorkerRange>,
    writer: PositionalDiskWriter,
    all_workers: Arc<Mutex<Vec<Arc<WorkerRange>>>>,
    cancelled: Arc<AtomicBool>,
) -> Result<(), KestrelError> {
    loop {
        if cancelled.load(Ordering::Relaxed) {
            return Ok(());
        }

        let cur = worker.current_byte.load(Ordering::SeqCst);
        let end = worker.end_byte.load(Ordering::SeqCst);

        if cur < end {
            // Download assigned span
            worker.active.store(true, Ordering::Relaxed);
            download_span(&client, &url, etag.as_deref(), &headers, &worker, &writer, &cancelled).await?;
            worker.active.store(false, Ordering::Relaxed);
        }

        // Check if there is work to steal from other workers
        let steal_target = {
            let list = all_workers.lock().await;
            let mut candidate: Option<(Arc<WorkerRange>, u64)> = None;

            for w in list.iter() {
                let w_cur = w.current_byte.load(Ordering::Relaxed);
                let w_end = w.end_byte.load(Ordering::Relaxed);
                let rem = w_end.saturating_sub(w_cur);
                if rem >= MIN_STEAL_SPAN {
                    if let Some((_, max_rem)) = candidate {
                        if rem > max_rem {
                            candidate = Some((Arc::clone(w), rem));
                        }
                    } else {
                        candidate = Some((Arc::clone(w), rem));
                    }
                }
            }
            candidate
        };

        if let Some((target, _rem)) = steal_target {
            let target_cur = target.current_byte.load(Ordering::SeqCst);
            let target_end = target.end_byte.load(Ordering::SeqCst);
            let actual_rem = target_end.saturating_sub(target_cur);

            if actual_rem >= MIN_STEAL_SPAN {
                // Calculate mid-point aligned to 4KB page boundary
                let half = actual_rem / 2;
                let mid = ((target_cur + half) / PAGE_ALIGN) * PAGE_ALIGN;

                if mid > target_cur && mid < target_end {
                    // Lower target's end_byte to mid
                    target.end_byte.store(mid, Ordering::SeqCst);

                    // Create or reassign worker range [mid, target_end)
                    let new_index = {
                        let mut list = all_workers.lock().await;
                        let idx = list.len() as u32;
                        let stolen_worker = Arc::new(WorkerRange::new(idx, mid, target_end, mid));
                        list.push(Arc::clone(&stolen_worker));
                        worker = stolen_worker;
                        idx
                    };
                    tracing::debug!("Worker stole range [{}, {}) as worker {}", mid, target_end, new_index);
                    continue;
                }
            }
        }

        // No more work to steal; thread exits
        break;
    }

    Ok(())
}

async fn download_span(
    client: &Client,
    url: &str,
    etag: Option<&str>,
    headers: &[(String, String)],
    worker: &Arc<WorkerRange>,
    writer: &PositionalDiskWriter,
    cancelled: &Arc<AtomicBool>,
) -> Result<(), KestrelError> {
    let mut attempt = 0;

    while attempt < MAX_RETRIES {
        if cancelled.load(Ordering::Relaxed) {
            return Ok(());
        }

        let cur = worker.current_byte.load(Ordering::SeqCst);
        let end = worker.end_byte.load(Ordering::SeqCst);

        if cur >= end {
            return Ok(());
        }

        let range_header = format!("bytes={}-{}", cur, end - 1);
        let mut req_headers = HeaderMap::new();
        for (k, v) in headers {
            if let (Ok(hname), Ok(hval)) = (k.parse::<reqwest::header::HeaderName>(), HeaderValue::from_str(v)) {
                req_headers.insert(hname, hval);
            }
        }
        req_headers.insert(RANGE, HeaderValue::from_str(&range_header).unwrap());
        if let Some(tag) = etag {
            req_headers.insert(IF_RANGE, HeaderValue::from_str(tag).unwrap());
        }

        let send_res = client.get(url)
            .headers(req_headers)
            .send()
            .await;

        let response = match send_res {
            Ok(res) => res,
            Err(_e) => {
                attempt += 1;
                let backoff = (BASE_BACKOFF_MS * (1 << attempt)).min(MAX_BACKOFF_MS);
                tokio::time::sleep(Duration::from_millis(backoff)).await;
                continue;
            }
        };

        let status = response.status();
        if status == reqwest::StatusCode::PRECONDITION_FAILED || (status == reqwest::StatusCode::OK && cur > 0) {
            return Err(KestrelError::SourceChanged("Source file changed on server".into()));
        }

        if !status.is_success() {
            attempt += 1;
            let backoff = (BASE_BACKOFF_MS * (1 << attempt)).min(MAX_BACKOFF_MS);
            tokio::time::sleep(Duration::from_millis(backoff)).await;
            continue;
        }

        let mut stream = response.bytes_stream();
        let mut buffer = Vec::with_capacity(CHUNK_COALESCE_SIZE);
        let mut write_offset = cur;

        let mut failed = false;
        while let Some(chunk_res) = stream.next().await {
            if cancelled.load(Ordering::Relaxed) {
                return Ok(());
            }

            // Check if end_byte was lowered by work-stealing
            let current_end = worker.end_byte.load(Ordering::Relaxed);
            if write_offset >= current_end {
                break;
            }

            match chunk_res {
                Ok(chunk) => {
                    let max_take = (current_end - write_offset) as usize;
                    let to_take = chunk.len().min(max_take);
                    buffer.extend_from_slice(&chunk[..to_take]);

                    if buffer.len() >= CHUNK_COALESCE_SIZE || write_offset + buffer.len() as u64 >= current_end {
                        let data = std::mem::replace(&mut buffer, Vec::with_capacity(CHUNK_COALESCE_SIZE));
                        let len = data.len() as u64;
                        writer.write_at(write_offset, data).await?;
                        write_offset += len;
                        worker.current_byte.store(write_offset, Ordering::SeqCst);
                    }

                    if write_offset >= current_end {
                        break;
                    }
                }
                Err(_) => {
                    failed = true;
                    break;
                }
            }
        }

        // Flush remaining buffer
        if !buffer.is_empty() {
            let len = buffer.len() as u64;
            writer.write_at(write_offset, buffer).await?;
            write_offset += len;
            worker.current_byte.store(write_offset, Ordering::SeqCst);
        }

        if !failed {
            return Ok(());
        }

        attempt += 1;
        let backoff = (BASE_BACKOFF_MS * (1 << attempt)).min(MAX_BACKOFF_MS);
        tokio::time::sleep(Duration::from_millis(backoff)).await;
    }

    Err(KestrelError::Network(format!("Segment worker exceeded max retries for range starting at {}", worker.start_byte)))
}
