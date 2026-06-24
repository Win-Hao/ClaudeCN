//! 跨平台公共汉化逻辑：探测、合并译文、写入、白名单、备份、locale 配置。
//! 逐函数移植自 skills/claude-localize/scripts/patch_macos.py，OS 无关的部分都在这里。
//! OS 差异（重签名 / 提权换入 / 备份打包）见 macos.rs / windows.rs。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use regex::Regex;
use serde_json::{Map, Value};
use tauri::{AppHandle, Emitter, Manager};
use walkdir::WalkDir;

pub mod assets;
#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(target_os = "windows")]
pub mod windows;

#[cfg(target_os = "macos")]
use macos as platform;
#[cfg(target_os = "windows")]
use windows as platform;

/// 目标 locale。若实测发现新版把 zh-CN 归一成别的，改这里。
pub const LOCALE: &str = "zh-CN";
/// 中文译文同时写入这些 locale 文件（尤其覆盖 en-US，见 §4.2①）。
pub const TARGET_LOCALES: [&str; 3] = ["en-US", "zh-CN", "zh"];

// ---------------------------------------------------------------- 对外数据类型

/// detect 一次性返回，驱动前端状态机。
#[derive(Debug, Clone, serde::Serialize)]
pub struct ClaudeStatus {
    pub installed: bool,
    pub version: Option<String>,
    pub patched: bool,
    pub has_backup: bool,
    pub coverage_pct: Option<f64>,
    pub dev_mode_hint: bool,
    pub app_path: Option<String>,
}

impl ClaudeStatus {
    /// Claude 未安装时的状态（仍带上备份信息）。
    pub fn not_installed(has_backup: bool) -> Self {
        ClaudeStatus {
            installed: false,
            version: None,
            patched: false,
            has_backup,
            coverage_pct: None,
            dev_mode_hint: false,
            app_path: None,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct LocalizeResult {
    pub ok: bool,
    pub coverage_pct: f64,
    pub version: String,
}

// ---------------------------------------------------------------- 平台分发

pub fn detect(app: &AppHandle) -> ClaudeStatus {
    platform::detect(app)
}

pub fn apply(app: &AppHandle) -> Result<LocalizeResult, String> {
    platform::apply(app)
}

pub fn restore(app: &AppHandle, force: bool) -> Result<(), String> {
    platform::restore(app, force)
}

// ---------------------------------------------------------------- 进度上报

/// 把一条进度同时 emit 给前端、并追加到日志缓冲（供「复制日志」）。
pub fn report(app: &AppHandle, msg: impl Into<String>) {
    let msg = msg.into();
    let _ = app.emit("localize-progress", msg.clone());
    if let Some(state) = app.try_state::<crate::AppState>() {
        state.logs.lock().unwrap().push(msg);
    }
}

// ---------------------------------------------------------------- JSON 工具

/// 读取一个 JSON 对象文件为 Map。serde_json 默认 Map = BTreeMap（键有序）。
pub fn load_json_map(path: &Path) -> Result<Map<String, Value>, String> {
    let txt = std::fs::read_to_string(path)
        .map_err(|e| format!("读取 {} 失败: {e}", path.display()))?;
    match serde_json::from_str::<Value>(&txt)
        .map_err(|e| format!("解析 {} 失败: {e}", path.display()))?
    {
        Value::Object(m) => Ok(m),
        _ => Err(format!("{} 不是 JSON 对象", path.display())),
    }
}

/// overlay 覆盖 base；都为对象时递归。Claude 现版是扁平表，递归只为兼容旧版。
fn deep_merge(base: &Map<String, Value>, overlay: &Map<String, Value>) -> Map<String, Value> {
    let mut out = base.clone();
    for (k, v) in overlay {
        match (out.get(k), v) {
            (Some(Value::Object(bo)), Value::Object(vo)) => {
                out.insert(k.clone(), Value::Object(deep_merge(bo, vo)));
            }
            _ => {
                out.insert(k.clone(), v.clone());
            }
        }
    }
    out
}

/// 返回 (merged, untranslated)。
/// merged = en-US 为底（保证未译 key 回退英文），叠加内置精校 base。
/// untranslated = 当前 en-US 里不在 base 的字符串 key（{key: english}）。
pub fn build_merged(
    en: &Map<String, Value>,
    base: &Map<String, Value>,
) -> (Map<String, Value>, BTreeMap<String, String>) {
    let merged = deep_merge(en, base);
    let mut untranslated = BTreeMap::new();
    for (k, v) in en {
        if !base.contains_key(k) {
            if let Value::String(s) = v {
                untranslated.insert(k.clone(), s.clone());
            }
        }
    }
    (merged, untranslated)
}

// ---------------------------------------------------------------- 探测 / 适配

/// 前端 i18n 目录：优先标准路径，否则在 Contents/Resources 下找含 en-US.json
/// 且同目录 ≥3 个 json 的目录（自适应，别写死）。
#[allow(dead_code)] // mac 端自适应探测用；windows 由 ClaudeInstallation 直接给出
pub fn find_i18n_dir(app_root: &Path) -> Option<PathBuf> {
    let std_dir = app_root.join("Contents/Resources/ion-dist/i18n");
    if std_dir.join("en-US.json").exists() {
        return Some(std_dir);
    }
    let res = app_root.join("Contents/Resources");
    if res.exists() {
        for entry in WalkDir::new(&res).into_iter().flatten() {
            if entry.file_name() == "en-US.json" {
                if let Some(parent) = entry.path().parent() {
                    let sibs = std::fs::read_dir(parent)
                        .map(|rd| {
                            rd.flatten()
                                .filter(|e| {
                                    e.path().extension().map(|x| x == "json").unwrap_or(false)
                                })
                                .count()
                        })
                        .unwrap_or(0);
                    if sibs >= 3 {
                        return Some(parent.to_path_buf());
                    }
                }
            }
        }
    }
    None
}

fn is_index_js(name: &std::ffi::OsStr) -> bool {
    let n = name.to_string_lossy();
    n.starts_with("index-") && n.ends_with(".js")
}

/// 放 index-*.js 的目录（前端入口 JS）。优先标准路径，否则自适应搜索。
#[allow(dead_code)] // mac 端自适应探测用；windows 由 ClaudeInstallation 直接给出
pub fn find_assets_dir(app_root: &Path) -> Option<PathBuf> {
    let std_dir = app_root.join("Contents/Resources/ion-dist/assets/v1");
    if std_dir.exists()
        && std::fs::read_dir(&std_dir)
            .map(|rd| rd.flatten().any(|e| is_index_js(&e.file_name())))
            .unwrap_or(false)
    {
        return Some(std_dir);
    }
    let res = app_root.join("Contents/Resources");
    if res.exists() {
        for entry in WalkDir::new(&res).into_iter().flatten() {
            if is_index_js(entry.file_name()) {
                return entry.path().parent().map(|p| p.to_path_buf());
            }
        }
    }
    None
}

/// 纯净英文源：en-US.json 被覆盖成中文后，改用旁置的 en-US.original.json。
pub fn en_source(i18n_dir: &Path) -> PathBuf {
    let orig = i18n_dir.join("en-US.original.json");
    if orig.exists() {
        orig
    } else {
        i18n_dir.join("en-US.json")
    }
}

/// 已汉化标记：en-US.original.json 是覆盖 en-US 时留下的，比 config.locale 可靠
/// （后者会被 app 按账号语言回写）。
pub fn is_patched(i18n_dir: &Path) -> bool {
    i18n_dir.join("en-US.original.json").exists()
}

/// 内置 base 译文对当前 en-US 的覆盖率（百分比，1 位小数）。
pub fn coverage(i18n_dir: &Path, base: &Map<String, Value>) -> Option<f64> {
    let en = load_json_map(&en_source(i18n_dir)).ok()?;
    if en.is_empty() {
        return Some(0.0);
    }
    let covered = en.keys().filter(|k| base.contains_key(*k)).count();
    Some(round1(100.0 * covered as f64 / en.len() as f64))
}

fn round1(x: f64) -> f64 {
    (x * 10.0).round() / 10.0
}

// ---------------------------------------------------------------- 写入文件

/// 把中文 merged 写进 app 真正加载的 locale 文件——尤其**覆盖 en-US.json**（§4.2①）。
/// 覆盖前把原始 en-US.json 存为 en-US.original.json（仅首次）作纯净英文源 + 已汉化标记。
pub fn write_frontend(i18n_dir: &Path, merged: &Map<String, Value>) -> Result<(), String> {
    let orig = i18n_dir.join("en-US.original.json");
    if !orig.exists() {
        std::fs::copy(i18n_dir.join("en-US.json"), &orig)
            .map_err(|e| format!("保存 en-US.original.json 失败: {e}"))?;
    }
    // serde_json 默认 Map=BTreeMap → 键有序（对齐 python sort_keys=True）；UTF-8 原样输出。
    let blob = serde_json::to_string(merged).map_err(|e| format!("序列化译文失败: {e}"))?;
    for name in TARGET_LOCALES {
        std::fs::write(i18n_dir.join(format!("{name}.json")), &blob)
            .map_err(|e| format!("写入 {name}.json 失败: {e}"))?;
    }
    // zh-CN/zh 可能被 fetch overrides；en-US 不会被请求 overrides，别动它。
    for name in ["zh-CN", "zh"] {
        let ov = i18n_dir.join(format!("{name}.overrides.json"));
        if !ov.exists() {
            std::fs::write(&ov, "{}").map_err(|e| format!("写入 {name}.overrides.json 失败: {e}"))?;
        }
    }
    let dyn_dir = i18n_dir.join("dynamic");
    let dyn_en = dyn_dir.join("en-US.json");
    if dyn_dir.exists() && dyn_en.exists() {
        for name in ["zh-CN", "zh"] {
            let tgt = dyn_dir.join(format!("{name}.json"));
            if !tgt.exists() {
                std::fs::copy(&dyn_en, &tgt)
                    .map_err(|e| format!("写入 dynamic/{name}.json 失败: {e}"))?;
            }
        }
    }
    Ok(())
}

/// statsig 实验文案（若该版本有 statsig 目录）。
pub fn write_statsig(i18n_dir: &Path, statsig_base: &Path) -> Result<(), String> {
    let sdir = i18n_dir.join("statsig");
    if !sdir.exists() {
        return Ok(());
    }
    let target = sdir.join(format!("{LOCALE}.json"));
    if statsig_base.exists() {
        std::fs::copy(statsig_base, &target).map_err(|e| format!("写入 statsig 译文失败: {e}"))?;
    } else if sdir.join("en-US.json").exists() {
        std::fs::copy(sdir.join("en-US.json"), &target)
            .map_err(|e| format!("回退复制 statsig en-US 失败: {e}"))?;
    }
    Ok(())
}

/// 语言白名单自适应（§4.2②）。新版多半没有白名单数组 → 返回 "skipped"（不是错误）。
pub fn patch_whitelist(assets_dir: &Path) -> String {
    let patterns = [
        r#"(\["en-US","de-DE","fr-FR","ko-KR","ja-JP","es-419","es-ES","it-IT","hi-IN","pt-BR","id-ID"[^\]]*)\]"#,
        r#"(\["en-US"(?:,"[a-zA-Z]{2,3}(?:-[a-zA-Z0-9]{2,4})*")+)\]"#,
        r#"(\["en-US"(?:\s*,\s*"[a-zA-Z]{2,3}(?:-[a-zA-Z0-9]{2,4})*")+\s*)\]"#,
    ];
    let mut index_files: Vec<PathBuf> = match std::fs::read_dir(assets_dir) {
        Ok(rd) => rd
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.file_name().map(is_index_js).unwrap_or(false))
            .collect(),
        Err(_) => return "no-assets".to_string(),
    };
    index_files.sort();
    for p in index_files {
        let Ok(text) = std::fs::read_to_string(&p) else {
            continue;
        };
        if text.contains("\"zh-CN\"") {
            return "already".to_string();
        }
        for pat in patterns {
            let re = Regex::new(pat).expect("白名单正则应当合法");
            if re.is_match(&text) {
                let new = re.replacen(&text, 1, r#"${1},"zh-CN"]"#);
                if std::fs::write(&p, new.as_ref()).is_ok() {
                    return "injected".to_string();
                }
            }
        }
    }
    "skipped".to_string()
}

// ---------------------------------------------------------------- locale 配置

/// 所有 Claude 桌面端数据目录（§4.2⑤）。除标准 'Claude' 外，接入第三方/自定义模型用
/// 'Claude-3p'——活动配置可能只在这里。**排除我们自己的 'ClaudeCN'/'claude-localize'**
/// （它们无连字符 / 小写，天然不匹配 "Claude" 或 "Claude-" 前缀）。
pub fn claude_config_dirs(support_dir: &Path) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Ok(rd) = std::fs::read_dir(support_dir) {
        for e in rd.flatten() {
            let p = e.path();
            if !p.is_dir() {
                continue;
            }
            if let Some(name) = p.file_name().and_then(|s| s.to_str()) {
                if name == "Claude" || name.starts_with("Claude-") {
                    dirs.push(p);
                }
            }
        }
    }
    dirs.sort();
    if dirs.is_empty() {
        dirs.push(support_dir.join("Claude"));
    }
    dirs
}

/// 给所有 Claude 数据目录写 locale=zh-CN（兜底/未登录场景用；主力是 en-US 覆盖）。
pub fn write_locale_config(support_dir: &Path) -> Result<(), String> {
    for d in claude_config_dirs(support_dir) {
        std::fs::create_dir_all(&d).ok();
        let cf = d.join("config.json");
        let mut cfg = if cf.exists() {
            load_json_map(&cf).unwrap_or_default()
        } else {
            Map::new()
        };
        cfg.insert("locale".into(), Value::String(LOCALE.into()));
        let s = serde_json::to_string_pretty(&Value::Object(cfg))
            .map_err(|e| format!("序列化 config 失败: {e}"))?;
        std::fs::write(&cf, s).map_err(|e| format!("写入 {} 失败: {e}", cf.display()))?;
    }
    Ok(())
}

/// 恢复时清掉我们写的 locale 字段（其余配置不动）。
pub fn remove_locale_config(support_dir: &Path) -> Result<(), String> {
    for d in claude_config_dirs(support_dir) {
        let cf = d.join("config.json");
        if !cf.exists() {
            continue;
        }
        if let Ok(mut cfg) = load_json_map(&cf) {
            cfg.remove("locale");
            if let Ok(s) = serde_json::to_string_pretty(&Value::Object(cfg)) {
                let _ = std::fs::write(&cf, s);
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------- 备份定位

/// 备份位置（新）：~/Library/Application Support/claude-localize/backups。
#[allow(dead_code)] // mac 端 ditto 备份用；windows 走自身按文件备份
pub fn backup_dir(support_dir: &Path) -> PathBuf {
    support_dir.join("claude-localize/backups")
}

/// 兼容旧 GUI(ClaudeCN) 的备份位置：恢复/查重时也去那找。
#[allow(dead_code)] // mac 端用；windows 走自身按文件备份
pub fn legacy_backup_dir(support_dir: &Path) -> PathBuf {
    support_dir.join("ClaudeCN/backups")
}

/// 在新位置和旧 GUI 位置都找原版备份，返回 (zip 路径, 版本)。
#[allow(dead_code)] // mac 端 ditto 备份用；windows 走自身按文件备份
pub fn existing_backup(support_dir: &Path) -> Option<(PathBuf, String)> {
    for d in [backup_dir(support_dir), legacy_backup_dir(support_dir)] {
        let z = d.join("Claude-original.zip");
        if z.exists() {
            let ver = std::fs::read_to_string(d.join("backup-version.txt"))
                .map(|s| s.trim().to_string())
                .unwrap_or_default();
            return Some((z, ver));
        }
    }
    None
}

pub const BACKUP_README: &str = "这是 Claude 汉化工具 (ClaudeCN) 的原版备份，用于一键恢复英文原版。\n请勿删除本目录。\n\n即使误删也不要紧：Claude.app 可从 https://claude.ai/download 免费重装即恢复官方原版；\n你的聊天记录和登录都在 ~/Library/Application Support/Claude/（与 app 分开存），重装不丢。\n";

// ---------------------------------------------------------------- 单元测试

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn obj(v: Value) -> Map<String, Value> {
        v.as_object().unwrap().clone()
    }

    #[test]
    fn merge_en_base_full_coverage_and_english_fallback() {
        let en = obj(json!({"a": "Apple", "b": "Banana", "c": "Cherry"}));
        let base = obj(json!({"a": "苹果", "b": "香蕉"})); // c 未翻译
        let (merged, untrans) = build_merged(&en, &base);
        // en 全覆盖：每个 en key 都在 merged 里
        assert_eq!(merged.len(), 3);
        assert_eq!(merged["a"], json!("苹果"));
        assert_eq!(merged["b"], json!("香蕉"));
        // 未译 key 回退英文，而不是丢失或显示哈希
        assert_eq!(merged["c"], json!("Cherry"));
        assert_eq!(untrans.len(), 1);
        assert_eq!(untrans["c"], "Cherry");
    }

    #[test]
    fn merge_base_only_keys_enter_merged_but_not_untranslated() {
        // base 里 en 没有的旧 key 会进 merged，但不计入 untranslated
        let en = obj(json!({"a": "A"}));
        let base = obj(json!({"a": "啊", "z": "额外"}));
        let (merged, untrans) = build_merged(&en, &base);
        assert_eq!(merged["a"], json!("啊"));
        assert_eq!(merged["z"], json!("额外"));
        assert!(untrans.is_empty());
    }

    #[test]
    fn written_blob_has_sorted_keys() {
        let merged = obj(json!({"b": "2", "a": "1", "c": "3"}));
        let blob = serde_json::to_string(&merged).unwrap();
        assert_eq!(blob, r#"{"a":"1","b":"2","c":"3"}"#);
    }

    #[test]
    fn config_dirs_include_claude_variants_exclude_our_own() {
        let tmp = std::env::temp_dir().join(format!("claudecn-test-cfgdirs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        for name in ["Claude", "Claude-3p", "ClaudeCN", "claude-localize", "NotClaude"] {
            std::fs::create_dir_all(tmp.join(name)).unwrap();
        }
        let names: Vec<String> = claude_config_dirs(&tmp)
            .iter()
            .map(|d| d.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert!(names.contains(&"Claude".to_string()));
        assert!(names.contains(&"Claude-3p".to_string()));
        assert!(!names.iter().any(|n| n == "ClaudeCN")); // 无连字符 → 排除
        assert!(!names.iter().any(|n| n == "claude-localize")); // 小写 → 排除
        assert!(!names.iter().any(|n| n == "NotClaude"));
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn config_dirs_fallback_when_empty() {
        let tmp = std::env::temp_dir().join(format!("claudecn-test-empty-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let dirs = claude_config_dirs(&tmp);
        assert_eq!(dirs.len(), 1);
        assert!(dirs[0].ends_with("Claude"));
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn write_frontend_overwrites_en_and_keeps_pristine_original() {
        let tmp = std::env::temp_dir().join(format!("claudecn-test-frontend-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        std::fs::write(tmp.join("en-US.json"), r#"{"a":"Apple"}"#).unwrap();
        let merged = obj(json!({"a": "苹果"}));
        write_frontend(&tmp, &merged).unwrap();
        // en-US 被覆盖成中文
        assert!(std::fs::read_to_string(tmp.join("en-US.json")).unwrap().contains("苹果"));
        // 原始英文留底
        assert_eq!(
            std::fs::read_to_string(tmp.join("en-US.original.json")).unwrap(),
            r#"{"a":"Apple"}"#
        );
        // zh-CN / zh 也写了
        assert!(tmp.join("zh-CN.json").exists());
        assert!(tmp.join("zh.json").exists());
        // overrides 占位
        assert_eq!(std::fs::read_to_string(tmp.join("zh-CN.overrides.json")).unwrap(), "{}");
        // 标记
        assert!(is_patched(&tmp));
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn write_frontend_twice_preserves_first_original() {
        let tmp = std::env::temp_dir().join(format!("claudecn-test-frontend2-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        std::fs::write(tmp.join("en-US.json"), r#"{"a":"Apple"}"#).unwrap();
        write_frontend(&tmp, &obj(json!({"a": "苹果"}))).unwrap();
        // 第二次打补丁：en-US 现在已是中文，但 original 必须保持第一次的纯净英文
        write_frontend(&tmp, &obj(json!({"a": "苹果2"}))).unwrap();
        assert_eq!(
            std::fs::read_to_string(tmp.join("en-US.original.json")).unwrap(),
            r#"{"a":"Apple"}"#
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn locale_config_roundtrip_preserves_other_keys() {
        let tmp = std::env::temp_dir().join(format!("claudecn-test-locale-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let claude = tmp.join("Claude");
        std::fs::create_dir_all(&claude).unwrap();
        std::fs::write(claude.join("config.json"), r#"{"keep":"me"}"#).unwrap();
        write_locale_config(&tmp).unwrap();
        let cfg = load_json_map(&claude.join("config.json")).unwrap();
        assert_eq!(cfg["locale"], json!("zh-CN"));
        assert_eq!(cfg["keep"], json!("me")); // 其余配置不动
        remove_locale_config(&tmp).unwrap();
        let cfg = load_json_map(&claude.join("config.json")).unwrap();
        assert!(!cfg.contains_key("locale"));
        assert_eq!(cfg["keep"], json!("me"));
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn whitelist_injects_into_array_and_is_idempotent() {
        let tmp = std::env::temp_dir().join(format!("claudecn-test-wl-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let js = tmp.join("index-abc123.js");
        std::fs::write(&js, r#"var langs=["en-US","de-DE","fr-FR"];console.log(1)"#).unwrap();
        assert_eq!(patch_whitelist(&tmp), "injected");
        let after = std::fs::read_to_string(&js).unwrap();
        assert!(after.contains(r#""zh-CN"]"#));
        // 再跑一次：已有 zh-CN → already
        assert_eq!(patch_whitelist(&tmp), "already");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn whitelist_skips_when_no_array() {
        let tmp = std::env::temp_dir().join(format!("claudecn-test-wl2-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        std::fs::write(tmp.join("index-x.js"), "const a=fetch(`/i18n/${loc}.json`)").unwrap();
        // 新机制：动态 fetch，无白名单数组 → skipped（不是错误）
        assert_eq!(patch_whitelist(&tmp), "skipped");
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
