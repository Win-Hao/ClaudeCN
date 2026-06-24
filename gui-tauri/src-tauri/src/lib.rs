//! ClaudeCN — Tauri 入口。命令分发到 localize 模块（mac 实现见 localize/macos.rs）。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_opener::OpenerExt;

mod localize;

use localize::{ClaudeStatus, LocalizeResult};

/// 开发者模式说明（指向项目 README）。
const DEV_MODE_HELP_URL: &str = "https://github.com/Win-Hao/ClaudeCN#readme";

/// 极简全局状态：处理锁 + 日志缓冲（供「复制日志」）。
pub struct AppState {
    /// 是否正在汉化/恢复（防并发触发）。
    pub processing: AtomicBool,
    /// 最近一次处理的进度日志。
    pub logs: Mutex<Vec<String>>,
}

/// RAII：进入处理时占锁、清空旧日志；离开（含出错早返回）时自动释放。
struct BusyGuard(AppHandle);

impl Drop for BusyGuard {
    fn drop(&mut self) {
        self.0
            .state::<AppState>()
            .processing
            .store(false, Ordering::SeqCst);
    }
}

fn acquire_busy(app: &AppHandle) -> Result<BusyGuard, String> {
    let st = app.state::<AppState>();
    if st.processing.swap(true, Ordering::SeqCst) {
        return Err("正在处理中，请稍候…".to_string());
    }
    st.logs.lock().unwrap().clear();
    Ok(BusyGuard(app.clone()))
}

/// GUI 自身版本号（供前端关于页/状态显示）。
#[tauri::command]
fn gui_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// 探测 Claude 安装/版本/是否已汉化/有无备份/覆盖率，驱动前端状态机。
#[tauri::command]
async fn detect_claude(app: AppHandle) -> ClaudeStatus {
    let app2 = app.clone();
    tauri::async_runtime::spawn_blocking(move || localize::detect(&app2))
        .await
        .unwrap_or_else(|_| ClaudeStatus::not_installed(false))
}

/// 执行汉化（内部 emit 进度，mac 触发一次提权）。
#[tauri::command]
async fn apply_localization(app: AppHandle) -> Result<LocalizeResult, String> {
    let _guard = acquire_busy(&app)?;
    let app2 = app.clone();
    let res = tauri::async_runtime::spawn_blocking(move || localize::apply(&app2))
        .await
        .map_err(|e| format!("任务执行异常: {e}"))?;
    if let Err(ref e) = res {
        let _ = app.emit("localize-error", e.clone());
    }
    res
}

/// 恢复英文原版（含防降级/缺失提示，mac 提权一次）。
#[tauri::command]
async fn restore_original(app: AppHandle, force: bool) -> Result<(), String> {
    let _guard = acquire_busy(&app)?;
    let app2 = app.clone();
    let res = tauri::async_runtime::spawn_blocking(move || localize::restore(&app2, force))
        .await
        .map_err(|e| format!("任务执行异常: {e}"))?;
    if let Err(ref e) = res {
        let _ = app.emit("localize-error", e.clone());
    }
    res
}

/// 打开开发者模式说明网页。
#[tauri::command]
fn open_dev_mode_help(app: AppHandle) -> Result<(), String> {
    app.opener()
        .open_url(DEV_MODE_HELP_URL, None::<&str>)
        .map_err(|e| format!("打开网页失败: {e}"))
}

/// 自动更新装完后的可靠重启：兜底 Tauri v2 在 macOS 上 relaunch() 的已知 bug
/// （装好新包却没能重启）。spawn 一个脱离的 helper 轮询父进程退出后再 `open -n` 重开。
#[tauri::command]
fn force_quit_and_relaunch(app: AppHandle) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let current_exe =
            std::env::current_exe().map_err(|e| format!("current_exe failed: {e}"))?;
        let ppid = std::process::id();
        let app_bundle = current_exe
            .ancestors()
            .find(|p| p.extension().and_then(|s| s.to_str()) == Some("app"))
            .ok_or_else(|| "current_exe 祖先里没有 .app bundle".to_string())?;
        let escaped = format!("'{}'", app_bundle.to_string_lossy().replace('\'', "'\\''"));
        let cmd = format!(
            "i=0; while kill -0 {ppid} 2>/dev/null && [ $i -lt 100 ]; do sleep 0.1; i=$((i+1)); done; sleep 0.3; open -n {escaped}"
        );
        std::process::Command::new("sh")
            .arg("-c")
            .arg(&cmd)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|e| format!("spawn relaunch helper failed: {e}"))?;
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(200));
            app.exit(0);
        });
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    {
        // windows/linux 上 plugin-process 的 relaunch 没有那个 bug；这里直接 restart（返回 ! 不再返回）
        app.restart()
    }
}

/// 汇总最近一次处理日志，供「复制日志」反馈。
#[tauri::command]
fn collect_logs(app: AppHandle) -> String {
    let st = app.state::<AppState>();
    let logs = st.logs.lock().unwrap();
    let mut out = format!(
        "ClaudeCN GUI v{} · {}\n",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS
    );
    if logs.is_empty() {
        out.push_str("（暂无处理日志）");
    } else {
        out.push_str(&logs.join("\n"));
    }
    out
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .manage(AppState {
            processing: AtomicBool::new(false),
            logs: Mutex::new(Vec::new()),
        })
        .invoke_handler(tauri::generate_handler![
            gui_version,
            detect_claude,
            apply_localization,
            restore_original,
            open_dev_mode_help,
            collect_logs,
            force_quit_and_relaunch
        ])
        .run(tauri::generate_context!())
        .expect("error while running ClaudeCN");
}
