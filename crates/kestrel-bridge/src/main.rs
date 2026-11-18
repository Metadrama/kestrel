use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use kestrel_ipc::{get_default_socket_path, read_frame, write_frame};
use kestrel_proto::{IpcRequest, IpcResponse};
use std::io::{stdin, stdout, Read, Write};
use tokio::net::UnixStream;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let sock_path = get_default_socket_path();

    #[cfg(unix)]
    {
        let mut stream = UnixStream::connect(&sock_path).await?;
        let (mut reader, mut writer) = stream.split();

        loop {
            // Read 4-byte length (little-endian for native messaging)
            let mut stdin_lock = stdin().lock();
            let len = match stdin_lock.read_u32::<LittleEndian>() {
                Ok(l) => l as usize,
                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
                Err(e) => return Err(e.into()),
            };

            let mut msg_buf = vec![0u8; len];
            stdin_lock.read_exact(&mut msg_buf)?;

            let val: serde_json::Value = serde_json::from_slice(&msg_buf)?;

            if let Some(action) = val.get("action").and_then(|a| a.as_str()) {
                if action == "enqueue" {
                    if let Some(url) = val.get("url").and_then(|u| u.as_str()) {
                        let mut headers = Vec::new();
                        if let Some(h_arr) = val.get("headers").and_then(|h| h.as_array()) {
                            for item in h_arr {
                                if let (Some(k), Some(v)) = (item.get(0).and_then(|s| s.as_str()), item.get(1).and_then(|s| s.as_str())) {
                                    headers.push((k.to_string(), v.to_string()));
                                }
                            }
                        }

                        let req = IpcRequest::EnqueueUrl {
                            url: url.to_string(),
                            headers,
                            dest_dir: None,
                            auto_start: true,
                        };

                        write_frame(&mut writer, &req).await?;
                        if let Ok(Some(resp)) = read_frame::<_, IpcResponse>(&mut reader).await {
                            let out_json = serde_json::to_vec(&resp)?;
                            let mut stdout_lock = stdout().lock();
                            stdout_lock.write_u32::<LittleEndian>(out_json.len() as u32)?;
                            stdout_lock.write_all(&out_json)?;
                            stdout_lock.flush()?;
                        }
                    }
                }
            }
        }
    }

    Ok(())
}
