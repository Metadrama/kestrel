use kestrel_proto::KestrelError;
use std::path::PathBuf;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

pub const DEFAULT_SOCKET_PATH: &str = "/tmp/kestrel.sock";
pub const WINDOWS_PIPE_NAME: &str = r"\\.\pipe\kestrel-ipc";

pub fn get_default_socket_path() -> PathBuf {
    #[cfg(unix)]
    {
        if let Ok(runtime_dir) = std::env::var("XDG_RUNTIME_DIR") {
            return PathBuf::from(runtime_dir).join("kestrel.sock");
        }
    }
    PathBuf::from(DEFAULT_SOCKET_PATH)
}

/// Read a length-prefixed frame from an async stream
pub async fn read_frame<R, T>(reader: &mut R) -> Result<Option<T>, KestrelError>
where
    R: AsyncRead + Unpin,
    T: serde::de::DeserializeOwned,
{
    let mut len_buf = [0u8; 4];
    match reader.read_exact(&mut len_buf).await {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(KestrelError::Io(e.to_string())),
    }

    let len = u32::from_be_bytes(len_buf) as usize;
    if len > 16 * 1024 * 1024 {
        return Err(KestrelError::Protocol(format!("Frame size {} exceeds 16MB limit", len)));
    }

    let mut data = vec![0u8; len];
    reader.read_exact(&mut data).await
        .map_err(|e| KestrelError::Io(e.to_string()))?;

    let payload: T = serde_json::from_slice(&data)
        .map_err(|e| KestrelError::Protocol(format!("Failed to deserialize frame: {}", e)))?;

    Ok(Some(payload))
}

/// Write a length-prefixed frame to an async stream
pub async fn write_frame<W, T>(writer: &mut W, value: &T) -> Result<(), KestrelError>
where
    W: AsyncWrite + Unpin,
    T: serde::Serialize,
{
    let payload = serde_json::to_vec(value)
        .map_err(|e| KestrelError::Protocol(e.to_string()))?;

    let len = payload.len() as u32;
    writer.write_all(&len.to_be_bytes()).await
        .map_err(|e| KestrelError::Io(e.to_string()))?;
    writer.write_all(&payload).await
        .map_err(|e| KestrelError::Io(e.to_string()))?;
    writer.flush().await
        .map_err(|e| KestrelError::Io(e.to_string()))?;

    Ok(())
}
