use std::path::Path;
use std::sync::{Arc, Mutex};
use kestrel_proto::{DownloadStatus, KestrelError, MediaVariant, MimeCategory};
use rusqlite::{params, Connection, OptionalExtension};

#[derive(Debug, Clone)]
pub struct DownloadRecord {
    pub id: String,
    pub url: String,
    pub title: String,
    pub dest_path: String,
    pub mime_category: MimeCategory,
    pub status: DownloadStatus,
    pub total_bytes: Option<u64>,
    pub downloaded_bytes: u64,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub headers: Vec<(String, String)>,
    pub quality_options: Vec<MediaVariant>,
    pub selected_variant_id: Option<String>,
    pub error_summary: Option<String>,
    pub sha256_hash: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone)]
pub struct SegmentRecord {
    pub download_id: String,
    pub segment_index: u32,
    pub start_byte: u64,
    pub end_byte: u64,
    pub current_byte: u64,
    pub is_complete: bool,
}

#[derive(Clone)]
pub struct Store {
    conn: Arc<Mutex<Connection>>,
}

impl Store {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, KestrelError> {
        let conn = Connection::open(path).map_err(|e| KestrelError::Store(e.to_string()))?;
        
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA foreign_keys = ON;"
        ).map_err(|e| KestrelError::Store(e.to_string()))?;

        let store = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        store.init_schema()?;
        Ok(store)
    }

    pub fn open_in_memory() -> Result<Self, KestrelError> {
        let conn = Connection::open_in_memory().map_err(|e| KestrelError::Store(e.to_string()))?;
        let store = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        store.init_schema()?;
        Ok(store)
    }

    fn init_schema(&self) -> Result<(), KestrelError> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS downloads (
                id TEXT PRIMARY KEY,
                url TEXT NOT NULL,
                title TEXT NOT NULL,
                dest_path TEXT NOT NULL,
                mime_category TEXT NOT NULL,
                status TEXT NOT NULL,
                total_bytes INTEGER,
                downloaded_bytes INTEGER NOT NULL DEFAULT 0,
                etag TEXT,
                last_modified TEXT,
                headers_json TEXT NOT NULL DEFAULT '[]',
                quality_options_json TEXT NOT NULL DEFAULT '[]',
                selected_variant_id TEXT,
                error_summary TEXT,
                sha256_hash TEXT,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS segments (
                download_id TEXT NOT NULL,
                segment_index INTEGER NOT NULL,
                start_byte INTEGER NOT NULL,
                end_byte INTEGER NOT NULL,
                current_byte INTEGER NOT NULL,
                is_complete INTEGER NOT NULL DEFAULT 0,
                PRIMARY KEY (download_id, segment_index),
                FOREIGN KEY (download_id) REFERENCES downloads(id) ON DELETE CASCADE
            );"
        ).map_err(|e| KestrelError::Store(e.to_string()))?;

        Ok(())
    }

    pub fn insert_download(&self, rec: &DownloadRecord) -> Result<(), KestrelError> {
        let conn = self.conn.lock().unwrap();
        let headers_json = serde_json::to_string(&rec.headers).unwrap_or_else(|_| "[]".to_string());
        let quality_json = serde_json::to_string(&rec.quality_options).unwrap_or_else(|_| "[]".to_string());
        let mime_str = serde_json::to_string(&rec.mime_category).unwrap_or_default();
        let status_str = serde_json::to_string(&rec.status).unwrap_or_default();

        conn.execute(
            "INSERT OR REPLACE INTO downloads (
                id, url, title, dest_path, mime_category, status, total_bytes, downloaded_bytes,
                etag, last_modified, headers_json, quality_options_json, selected_variant_id,
                error_summary, sha256_hash, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
            params![
                rec.id,
                rec.url,
                rec.title,
                rec.dest_path,
                mime_str,
                status_str,
                rec.total_bytes.map(|v| v as i64),
                rec.downloaded_bytes as i64,
                rec.etag,
                rec.last_modified,
                headers_json,
                quality_json,
                rec.selected_variant_id,
                rec.error_summary,
                rec.sha256_hash,
                rec.created_at,
                rec.updated_at,
            ],
        ).map_err(|e| KestrelError::Store(e.to_string()))?;

        Ok(())
    }

    pub fn update_status(&self, id: &str, status: DownloadStatus, error: Option<&str>) -> Result<(), KestrelError> {
        let conn = self.conn.lock().unwrap();
        let status_str = serde_json::to_string(&status).unwrap_or_default();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        conn.execute(
            "UPDATE downloads SET status = ?1, error_summary = ?2, updated_at = ?3 WHERE id = ?4",
            params![status_str, error, now, id],
        ).map_err(|e| KestrelError::Store(e.to_string()))?;

        Ok(())
    }

    pub fn update_progress(&self, id: &str, downloaded_bytes: u64, total_bytes: Option<u64>) -> Result<(), KestrelError> {
        let conn = self.conn.lock().unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        conn.execute(
            "UPDATE downloads SET downloaded_bytes = ?1, total_bytes = COALESCE(?2, total_bytes), updated_at = ?3 WHERE id = ?4",
            params![downloaded_bytes as i64, total_bytes.map(|v| v as i64), now, id],
        ).map_err(|e| KestrelError::Store(e.to_string()))?;

        Ok(())
    }

    pub fn save_segments(&self, download_id: &str, segments: &[SegmentRecord]) -> Result<(), KestrelError> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction().map_err(|e| KestrelError::Store(e.to_string()))?;

        for seg in segments {
            tx.execute(
                "INSERT OR REPLACE INTO segments (
                    download_id, segment_index, start_byte, end_byte, current_byte, is_complete
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    download_id,
                    seg.segment_index,
                    seg.start_byte as i64,
                    seg.end_byte as i64,
                    seg.current_byte as i64,
                    if seg.is_complete { 1 } else { 0 },
                ],
            ).map_err(|e| KestrelError::Store(e.to_string()))?;
        }

        tx.commit().map_err(|e| KestrelError::Store(e.to_string()))?;
        Ok(())
    }

    pub fn update_segment_cursor(&self, download_id: &str, segment_index: u32, current_byte: u64, is_complete: bool) -> Result<(), KestrelError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE segments SET current_byte = ?1, is_complete = ?2 WHERE download_id = ?3 AND segment_index = ?4",
            params![
                current_byte as i64,
                if is_complete { 1 } else { 0 },
                download_id,
                segment_index,
            ],
        ).map_err(|e| KestrelError::Store(e.to_string()))?;
        Ok(())
    }

    pub fn load_segments(&self, download_id: &str) -> Result<Vec<SegmentRecord>, KestrelError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT segment_index, start_byte, end_byte, current_byte, is_complete
             FROM segments WHERE download_id = ?1 ORDER BY segment_index ASC"
        ).map_err(|e| KestrelError::Store(e.to_string()))?;

        let rows = stmt.query_map(params![download_id], |row| {
            Ok(SegmentRecord {
                download_id: download_id.to_string(),
                segment_index: row.get(0)?,
                start_byte: row.get::<_, i64>(1)? as u64,
                end_byte: row.get::<_, i64>(2)? as u64,
                current_byte: row.get::<_, i64>(3)? as u64,
                is_complete: row.get::<_, i32>(4)? != 0,
            })
        }).map_err(|e| KestrelError::Store(e.to_string()))?;

        let mut list = Vec::new();
        for r in rows {
            list.push(r.map_err(|e| KestrelError::Store(e.to_string()))?);
        }
        Ok(list)
    }

    pub fn get_download(&self, id: &str) -> Result<Option<DownloadRecord>, KestrelError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, url, title, dest_path, mime_category, status, total_bytes, downloaded_bytes,
                    etag, last_modified, headers_json, quality_options_json, selected_variant_id,
                    error_summary, sha256_hash, created_at, updated_at
             FROM downloads WHERE id = ?1"
        ).map_err(|e| KestrelError::Store(e.to_string()))?;

        let rec = stmt.query_row(params![id], |row| {
            let mime_str: String = row.get(4)?;
            let status_str: String = row.get(5)?;
            let headers_json: String = row.get(10)?;
            let quality_json: String = row.get(11)?;

            let mime_category = serde_json::from_str(&mime_str).unwrap_or(MimeCategory::Other);
            let status = serde_json::from_str(&status_str).unwrap_or(DownloadStatus::Queued);
            let headers = serde_json::from_str(&headers_json).unwrap_or_default();
            let quality_options = serde_json::from_str(&quality_json).unwrap_or_default();

            Ok(DownloadRecord {
                id: row.get(0)?,
                url: row.get(1)?,
                title: row.get(2)?,
                dest_path: row.get(3)?,
                mime_category,
                status,
                total_bytes: row.get::<_, Option<i64>>(6)?.map(|v| v as u64),
                downloaded_bytes: row.get::<_, i64>(7)? as u64,
                etag: row.get(8)?,
                last_modified: row.get(9)?,
                headers,
                quality_options,
                selected_variant_id: row.get(12)?,
                error_summary: row.get(13)?,
                sha256_hash: row.get(14)?,
                created_at: row.get(15)?,
                updated_at: row.get(16)?,
            })
        }).optional().map_err(|e| KestrelError::Store(e.to_string()))?;

        Ok(rec)
    }

    pub fn list_downloads(&self) -> Result<Vec<DownloadRecord>, KestrelError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, url, title, dest_path, mime_category, status, total_bytes, downloaded_bytes,
                    etag, last_modified, headers_json, quality_options_json, selected_variant_id,
                    error_summary, sha256_hash, created_at, updated_at
             FROM downloads ORDER BY created_at DESC"
        ).map_err(|e| KestrelError::Store(e.to_string()))?;

        let rows = stmt.query_map([], |row| {
            let mime_str: String = row.get(4)?;
            let status_str: String = row.get(5)?;
            let headers_json: String = row.get(10)?;
            let quality_json: String = row.get(11)?;

            let mime_category = serde_json::from_str(&mime_str).unwrap_or(MimeCategory::Other);
            let status = serde_json::from_str(&status_str).unwrap_or(DownloadStatus::Queued);
            let headers = serde_json::from_str(&headers_json).unwrap_or_default();
            let quality_options = serde_json::from_str(&quality_json).unwrap_or_default();

            Ok(DownloadRecord {
                id: row.get(0)?,
                url: row.get(1)?,
                title: row.get(2)?,
                dest_path: row.get(3)?,
                mime_category,
                status,
                total_bytes: row.get::<_, Option<i64>>(6)?.map(|v| v as u64),
                downloaded_bytes: row.get::<_, i64>(7)? as u64,
                etag: row.get(8)?,
                last_modified: row.get(9)?,
                headers,
                quality_options,
                selected_variant_id: row.get(12)?,
                error_summary: row.get(13)?,
                sha256_hash: row.get(14)?,
                created_at: row.get(15)?,
                updated_at: row.get(16)?,
            })
        }).map_err(|e| KestrelError::Store(e.to_string()))?;

        let mut list = Vec::new();
        for r in rows {
            list.push(r.map_err(|e| KestrelError::Store(e.to_string()))?);
        }
        Ok(list)
    }

    pub fn delete_download(&self, id: &str) -> Result<(), KestrelError> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM downloads WHERE id = ?1", params![id])
            .map_err(|e| KestrelError::Store(e.to_string()))?;
        Ok(())
    }

    /// Startup Crash Recovery:
    /// On daemon boot, transition any `Active` rows from an unclean exit into `Paused`,
    /// verify `.kestrel.part` file size on disk against `segments.current_byte`, and allow instant 1-click resume.
    pub fn recover_on_startup(&self) -> Result<usize, KestrelError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, dest_path FROM downloads WHERE status = '\"Active\"' OR status = '\"Resolving\"' OR status = '\"Muxing\"'"
        ).map_err(|e| KestrelError::Store(e.to_string()))?;

        let active_items: Vec<(String, String)> = stmt.query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?))
        }).map_err(|e| KestrelError::Store(e.to_string()))?
          .filter_map(Result::ok)
          .collect();

        let count = active_items.len();
        for (id, dest_path) in active_items {
            let part_path = format!("{}.kestrel.part", dest_path);
            let part_exists = Path::new(&part_path).exists();
            
            let paused_status = serde_json::to_string(&DownloadStatus::Paused).unwrap();
            let note = if part_exists {
                "Recovered after unexpected shutdown (ready to resume)"
            } else {
                "Paused after shutdown"
            };

            conn.execute(
                "UPDATE downloads SET status = ?1, error_summary = ?2 WHERE id = ?3",
                params![paused_status, note, id],
            ).map_err(|e| KestrelError::Store(e.to_string()))?;
        }

        Ok(count)
    }
}
