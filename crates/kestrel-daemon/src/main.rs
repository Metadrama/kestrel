use directories::ProjectDirs;
use kestrel_core::DownloadEngine;
use kestrel_ipc::{get_default_socket_path, read_frame, write_frame};
use kestrel_proto::{IpcRequest, IpcResponse};
use kestrel_store::Store;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::net::UnixListener;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();
    tracing::info!("Starting Kestrel daemon...");

    let proj_dirs = ProjectDirs::from("io", "github.metadrama", "kestrel");
    let data_dir = match &proj_dirs {
        Some(pd) => pd.data_local_dir().to_path_buf(),
        None => PathBuf::from("./data"),
    };
    std::fs::create_dir_all(&data_dir)?;

    let db_path = data_dir.join("kestrel.db");
    let store = Store::open(&db_path)?;
    
    // Crash recovery
    let recovered = store.recover_on_startup()?;
    if recovered > 0 {
        tracing::info!("Recovered {} active downloads into paused state", recovered);
    }

    let download_dir = directories::UserDirs::new()
        .and_then(|u| u.download_dir().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| data_dir.join("downloads"));
    std::fs::create_dir_all(&download_dir)?;

    let engine = Arc::new(DownloadEngine::new(store.clone(), download_dir, false));

    // Listen on IPC socket
    let sock_path = get_default_socket_path();
    if sock_path.exists() {
        let _ = std::fs::remove_file(&sock_path);
    }

    #[cfg(unix)]
    {
        let listener = UnixListener::bind(&sock_path)?;
        tracing::info!("Listening on IPC socket: {}", sock_path.display());

        loop {
            match listener.accept().await {
                Ok((mut stream, _)) => {
                    let engine = Arc::clone(&engine);
                    tokio::spawn(async move {
                        let (mut reader, mut writer) = stream.split();
                        while let Ok(Some(req)) = read_frame::<_, IpcRequest>(&mut reader).await {
                            match req {
                                IpcRequest::EnqueueUrl { url, headers, dest_dir, auto_start } => {
                                    match engine.enqueue(&url, headers, dest_dir, auto_start).await {
                                        Ok(id) => {
                                            let _ = write_frame(&mut writer, &IpcResponse::Enqueued { id }).await;
                                        }
                                        Err(e) => {
                                            let _ = write_frame(&mut writer, &IpcResponse::Error(e.to_string())).await;
                                        }
                                    }
                                }
                                IpcRequest::Pause { id } => {
                                    let _ = engine.pause(&id).await;
                                    let _ = write_frame(&mut writer, &IpcResponse::Ok).await;
                                }
                                IpcRequest::Resume { id } => {
                                    let _ = engine.resume(&id).await;
                                    let _ = write_frame(&mut writer, &IpcResponse::Ok).await;
                                }
                                IpcRequest::Cancel { id, delete_file } => {
                                    let _ = engine.cancel(&id, delete_file).await;
                                    let _ = write_frame(&mut writer, &IpcResponse::Ok).await;
                                }
                                IpcRequest::SubscribeSnapshots => {
                                    let mut rx = engine.subscribe();
                                    while rx.changed().await.is_ok() {
                                        let snaps = rx.borrow().clone();
                                        if write_frame(&mut writer, &IpcResponse::Snapshots(snaps)).await.is_err() {
                                            break;
                                        }
                                    }
                                }
                                _ => {
                                    let _ = write_frame(&mut writer, &IpcResponse::Ok).await;
                                }
                            }
                        }
                    });
                }
                Err(e) => {
                    tracing::error!("IPC accept error: {}", e);
                }
            }
        }
    }

    #[cfg(not(unix))]
    {
        tracing::info!("Kestrel engine ready.");
        tokio::signal::ctrl_c().await?;
        Ok(())
    }
}
