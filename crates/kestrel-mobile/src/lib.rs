use jni::objects::{JClass, JString};
use jni::sys::{jboolean, jstring};
use jni::JNIEnv;
use kestrel_core::DownloadEngine;
use kestrel_store::Store;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use tokio::runtime::Runtime;

static RUNTIME: OnceLock<Runtime> = OnceLock::new();
static ENGINE: OnceLock<Arc<DownloadEngine>> = OnceLock::new();
static INITIALIZED: AtomicBool = AtomicBool::new(false);

fn get_runtime() -> &'static Runtime {
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(4)
            .enable_all()
            .build()
            .expect("Failed to create Tokio runtime")
    })
}

#[no_mangle]
pub extern "system" fn Java_io_github_metadrama_kestrel_KestrelEngine_nativeInit(
    mut env: JNIEnv,
    _class: JClass,
    data_dir_j: JString,
    download_dir_j: JString,
) -> jboolean {
    if INITIALIZED.load(Ordering::SeqCst) {
        return 1;
    }

    let data_dir_str: String = match env.get_string(&data_dir_j) {
        Ok(s) => s.into(),
        Err(_) => return 0,
    };
    let download_dir_str: String = match env.get_string(&download_dir_j) {
        Ok(s) => s.into(),
        Err(_) => return 0,
    };

    let data_path = PathBuf::from(data_dir_str);
    let download_path = PathBuf::from(download_dir_str);
    let _ = std::fs::create_dir_all(&data_path);
    let _ = std::fs::create_dir_all(&download_path);

    let db_path = data_path.join("kestrel.db");
    let store = match Store::open(&db_path) {
        Ok(s) => s,
        Err(_) => return 0,
    };

    // Crash recovery
    let _ = store.recover_on_startup();

    let engine = Arc::new(DownloadEngine::new(store, download_path, true));
    let _ = ENGINE.set(engine);
    INITIALIZED.store(true, Ordering::SeqCst);

    1
}

#[no_mangle]
pub extern "system" fn Java_io_github_metadrama_kestrel_KestrelEngine_nativeEnqueue(
    mut env: JNIEnv,
    _class: JClass,
    url_j: JString,
    headers_json_j: JString,
    dest_dir_j: JString,
    auto_start: jboolean,
) -> jstring {
    let engine = match ENGINE.get() {
        Some(e) => Arc::clone(e),
        None => return std::ptr::null_mut(),
    };

    let url: String = match env.get_string(&url_j) {
        Ok(s) => s.into(),
        Err(_) => return std::ptr::null_mut(),
    };

    let headers_json: String = match env.get_string(&headers_json_j) {
        Ok(s) => s.into(),
        Err(_) => "[]".to_string(),
    };

    let dest_dir: Option<String> = env.get_string(&dest_dir_j)
        .ok()
        .map(|s| s.into())
        .filter(|s: &String| !s.is_empty());

    let headers: Vec<(String, String)> = serde_json::from_str(&headers_json).unwrap_or_default();

    let rt = get_runtime();
    let res = rt.block_on(async move {
        engine.enqueue(&url, headers, dest_dir, auto_start != 0).await
    });

    match res {
        Ok(id) => match env.new_string(id) {
            Ok(js) => js.into_raw(),
            Err(_) => std::ptr::null_mut(),
        },
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub extern "system" fn Java_io_github_metadrama_kestrel_KestrelEngine_nativePause(
    mut env: JNIEnv,
    _class: JClass,
    id_j: JString,
) -> jboolean {
    let engine = match ENGINE.get() {
        Some(e) => Arc::clone(e),
        None => return 0,
    };

    let id: String = match env.get_string(&id_j) {
        Ok(s) => s.into(),
        Err(_) => return 0,
    };

    let rt = get_runtime();
    let res = rt.block_on(async move {
        engine.pause(&id).await
    });

    if res.is_ok() { 1 } else { 0 }
}

#[no_mangle]
pub extern "system" fn Java_io_github_metadrama_kestrel_KestrelEngine_nativeResume(
    mut env: JNIEnv,
    _class: JClass,
    id_j: JString,
) -> jboolean {
    let engine = match ENGINE.get() {
        Some(e) => Arc::clone(e),
        None => return 0,
    };

    let id: String = match env.get_string(&id_j) {
        Ok(s) => s.into(),
        Err(_) => return 0,
    };

    let rt = get_runtime();
    let res = rt.block_on(async move {
        engine.resume(&id).await
    });

    if res.is_ok() { 1 } else { 0 }
}

#[no_mangle]
pub extern "system" fn Java_io_github_metadrama_kestrel_KestrelEngine_nativeCancel(
    mut env: JNIEnv,
    _class: JClass,
    id_j: JString,
    delete_file: jboolean,
) -> jboolean {
    let engine = match ENGINE.get() {
        Some(e) => Arc::clone(e),
        None => return 0,
    };

    let id: String = match env.get_string(&id_j) {
        Ok(s) => s.into(),
        Err(_) => return 0,
    };

    let rt = get_runtime();
    let res = rt.block_on(async move {
        engine.cancel(&id, delete_file != 0).await
    });

    if res.is_ok() { 1 } else { 0 }
}

#[no_mangle]
pub extern "system" fn Java_io_github_metadrama_kestrel_KestrelEngine_nativeGetSnapshotsJson(
    env: JNIEnv,
    _class: JClass,
) -> jstring {
    let engine = match ENGINE.get() {
        Some(e) => Arc::clone(e),
        None => return std::ptr::null_mut(),
    };

    let rt = get_runtime();
    let snapshots = rt.block_on(async move {
        engine.get_snapshots().await
    });

    let json = serde_json::to_string(&snapshots).unwrap_or_else(|_| "[]".to_string());
    match env.new_string(json) {
        Ok(js) => js.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}
