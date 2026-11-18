use kestrel_proto::KestrelError;
use sha2::{Digest, Sha256};
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub const CHUNK_COALESCE_SIZE: usize = 256 * 1024; // 256 KB aligned buffer

#[derive(Clone)]
pub struct PositionalDiskWriter {
    part_path: PathBuf,
    final_path: PathBuf,
    file: Arc<Mutex<File>>,
}

impl PositionalDiskWriter {
    pub fn new<P: AsRef<Path>>(dest_path: P, total_bytes: Option<u64>) -> Result<Self, KestrelError> {
        let final_path = dest_path.as_ref().to_path_buf();
        let part_path = PathBuf::from(format!("{}.kestrel.part", final_path.display()));

        if let Some(parent) = final_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| KestrelError::Io(e.to_string()))?;
        }

        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&part_path)
            .map_err(|e| KestrelError::Io(format!("Failed to open part file {}: {}", part_path.display(), e)))?;

        // Pre-allocate target file size if known
        if let Some(total) = total_bytes {
            let _ = file.set_len(total);
        }

        Ok(Self {
            part_path,
            final_path,
            file: Arc::new(Mutex::new(file)),
        })
    }

    pub async fn write_at(&self, offset: u64, data: Vec<u8>) -> Result<(), KestrelError> {
        let file = Arc::clone(&self.file);
        tokio::task::spawn_blocking(move || {
            let mut f = file.lock().unwrap();
            f.seek(SeekFrom::Start(offset))
                .map_err(|e| KestrelError::Io(e.to_string()))?;
            f.write_all(&data)
                .map_err(|e| KestrelError::Io(e.to_string()))?;
            Ok::<(), KestrelError>(())
        }).await.map_err(|e| KestrelError::Io(e.to_string()))?
    }

    pub fn flush(&self) -> Result<(), KestrelError> {
        let mut f = self.file.lock().unwrap();
        f.flush().map_err(|e| KestrelError::Io(e.to_string()))
    }

    pub async fn finalize(&self) -> Result<String, KestrelError> {
        self.flush()?;

        let part_path = self.part_path.clone();
        let final_path = self.final_path.clone();

        tokio::task::spawn_blocking(move || {
            // Rename .kestrel.part -> final filename (zero merge step)
            std::fs::rename(&part_path, &final_path)
                .map_err(|e| KestrelError::Io(format!("Failed to rename part file to final: {}", e)))?;
            Ok::<String, KestrelError>(final_path.to_string_lossy().to_string())
        }).await.map_err(|e| KestrelError::Io(e.to_string()))?
    }

    pub async fn compute_sha256(&self) -> Result<String, KestrelError> {
        let target = if self.final_path.exists() {
            self.final_path.clone()
        } else {
            self.part_path.clone()
        };

        tokio::task::spawn_blocking(move || {
            let mut file = File::open(&target)
                .map_err(|e| KestrelError::Io(format!("Failed to open file for SHA-256: {}", e)))?;
            let mut hasher = Sha256::new();
            let mut buffer = vec![0u8; 1024 * 1024]; // 1 MB buffer

            loop {
                let n = file.read(&mut buffer)
                    .map_err(|e| KestrelError::Io(e.to_string()))?;
                if n == 0 {
                    break;
                }
                hasher.update(&buffer[..n]);
            }

            let result = hasher.finalize();
            Ok(format!("{:x}", result))
        }).await.map_err(|e| KestrelError::Io(e.to_string()))?
    }
}
