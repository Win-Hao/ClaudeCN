use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

static LOG: Mutex<Option<PathBuf>> = Mutex::new(None);

pub fn init() {
    if let Ok(dir) = std::env::var("LOCALAPPDATA") {
        let log_dir = PathBuf::from(dir).join("ClaudeCN");
        let _ = fs::create_dir_all(&log_dir);
        let path = log_dir.join("debug.log");
        let _ = fs::write(&path, format!("=== ClaudeCN v1.2.0 started at {:?} ===\n", std::time::SystemTime::now()));
        if let Ok(mut lock) = LOG.lock() {
            *lock = Some(path);
        }
    }
}

pub fn log(msg: &str) {
    let path = match LOG.lock() {
        Ok(guard) => guard.clone(),
        Err(e) => e.into_inner().clone(),
    };
    if let Some(path) = path {
        if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&path) {
            let _ = writeln!(f, "[{:?}] {}", std::time::SystemTime::now(), msg);
        }
    }
}

pub fn log_path() -> String {
    match LOG.lock() {
        Ok(guard) => guard.as_ref().map(|p| p.to_string_lossy().to_string()).unwrap_or_default(),
        Err(e) => e.into_inner().as_ref().map(|p| p.to_string_lossy().to_string()).unwrap_or_default(),
    }
}
