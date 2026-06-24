//! 内置译文资源的读取。打包后从 Tauri resource_dir 读，开发期回退源码 resources/。

use std::path::PathBuf;

use serde_json::{Map, Value};
use tauri::{AppHandle, Manager};

/// 内置资源目录：优先打包进 app 的 resource_dir/resources，开发期回退 CARGO_MANIFEST_DIR/resources。
pub fn base_dir(app: &AppHandle) -> PathBuf {
    if let Ok(res) = app.path().resource_dir() {
        let p = res.join("resources");
        if p.exists() {
            return p;
        }
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources")
}

/// 前端 i18n 精校 base（18,686 条），合并的主力。
pub fn load_base(app: &AppHandle) -> Result<Map<String, Value>, String> {
    super::load_json_map(&base_dir(app).join("zh-CN.base.json"))
}

/// 桌面菜单层译文（Contents/Resources/zh-CN.json）。
pub fn desktop_base(app: &AppHandle) -> PathBuf {
    base_dir(app).join("desktop-zh-CN.base.json")
}

/// statsig 实验文案。
pub fn statsig_base(app: &AppHandle) -> PathBuf {
    base_dir(app).join("statsig-zh-CN.base.json")
}

/// 原生 Localizable.strings（mac 专属桌面层）。
#[allow(dead_code)] // mac 端写 lproj 用；windows 无此层
pub fn strings_base(app: &AppHandle) -> PathBuf {
    base_dir(app).join("claude-Localizable.strings")
}
