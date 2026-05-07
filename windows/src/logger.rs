use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

static LOG: Mutex<Option<PathBuf>> = Mutex::new(None);
static BUFFER: Mutex<Vec<String>> = Mutex::new(Vec::new());
const MAX_LINES: usize = 50;

pub fn init() {
    if let Ok(dir) = std::env::var("LOCALAPPDATA") {
        let log_dir = PathBuf::from(dir).join("ClaudeCN");
        let _ = fs::create_dir_all(&log_dir);
        let path = log_dir.join("debug.log");
        let _ = fs::write(
            &path,
            format!(
                "=== ClaudeCN v1.2.1 started at {:?} ===\n",
                std::time::SystemTime::now()
            ),
        );
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
            let _ = writeln!(f, "{}", msg);
        }
    }

    match BUFFER.lock() {
        Ok(mut buf) => {
            buf.push(msg.to_string());
            let len = buf.len();
            if len > MAX_LINES {
                buf.drain(..len - MAX_LINES);
            }
        }
        Err(e) => {
            let mut buf = e.into_inner();
            buf.push(msg.to_string());
        }
    }
}

pub fn recent_lines() -> Vec<String> {
    match BUFFER.lock() {
        Ok(buf) => buf.clone(),
        Err(e) => e.into_inner().clone(),
    }
}
