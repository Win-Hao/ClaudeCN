//! OAuth 模式的在线页面汉化。官方账号登录后主窗口加载的是远端 claude.ai，本地 i18n 补丁
//! 碰不到它；这里从 en-US.json 与 zh-CN base 生成「英文原文 → 中文」词典，拼成注入页面的
//! DOM 层翻译脚本（online-translate.js），再通过 inject 装进 app.asar 的主进程 dom-ready 钩子。

use std::path::Path;

use serde_json::{json, Map, Value};

use super::asar::Asar;
use super::inject;

/// 页面脚本正文，由 `build_page_script` 包成 IIFE。
pub const PAGE_SCRIPT: &str = include_str!("online-translate.js");
pub const HOST: &str = "claude.ai";
/// 超过这个长度的英文原文不进词典（长段落改版频繁，命中率低还拖慢匹配）。
pub const MAX_SOURCE_LEN: usize = 200;
/// 页面里挂状态的全局名。
pub const MARKER: &str = "__claudecnOnline";

/// 默认跳过的容器：对话正文、编辑器。选择器按对 claude.ai 页面的观察维护，可随版本调整。
pub const DEFAULT_SKIP_SELECTORS: &[&str] = &[
    "[data-testid=\"user-message\"]",
    ".font-claude-message",
    "[data-testid*=\"message-content\"]",
    ".ProseMirror",
    "[contenteditable=\"true\"]",
    "[data-claudecn-skip]",
];

/// 动态文案：JS 正则源 → 替换串（`$1` 引用分组）。只放词典覆盖不了的带数字句式。
pub const DEFAULT_PATTERNS: &[(&str, &str)] = &[
    (r"^(\d+) minutes? ago$", "$1 分钟前"),
    (r"^(\d+) hours? ago$", "$1 小时前"),
    (r"^(\d+) days? ago$", "$1 天前"),
    (r"^(\d+) weeks? ago$", "$1 周前"),
    (r"^(\d+) months? ago$", "$1 个月前"),
    (r"^(\d+) years? ago$", "$1 年前"),
    (r"^(\d+) messages?$", "$1 条消息"),
    (r"^(\d+) files?$", "$1 个文件"),
    (r"^(\d+) results?$", "$1 条结果"),
];

#[derive(Debug, Clone)]
pub struct OnlineConfig {
    pub host: String,
    pub skip_selectors: Vec<String>,
    pub patterns: Vec<(String, String)>,
    /// claude.ai 存语言偏好的 localStorage 键；未核实前留空，不写。
    pub locale_lock_key: Option<String>,
    pub locale_lock_value: String,
    pub debounce_ms: u64,
    pub max_wait_ms: u64,
    /// 测试用：脚本延迟多少毫秒后把统计作为 Promise 结果返回；生产为 None，立即返回。
    pub report_delay_ms: Option<u64>,
}

impl Default for OnlineConfig {
    fn default() -> Self {
        OnlineConfig {
            host: HOST.into(),
            skip_selectors: DEFAULT_SKIP_SELECTORS.iter().map(|s| s.to_string()).collect(),
            patterns: DEFAULT_PATTERNS
                .iter()
                .map(|(a, b)| (a.to_string(), b.to_string()))
                .collect(),
            locale_lock_key: None,
            locale_lock_value: "zh-CN".into(),
            debounce_ms: 200,
            max_wait_ms: 1000,
            report_delay_ms: None,
        }
    }
}

/// 一条英文原文能否进词典：短、单行、不含 ICU 占位符和标签、至少有一个字母。
pub fn usable_source(en: &str) -> bool {
    !en.is_empty()
        && en.len() <= MAX_SOURCE_LEN
        && !en.contains('{')
        && !en.contains('<')
        && !en.contains('\n')
        && en.chars().any(|c| c.is_ascii_alphabetic())
}

/// 用 i18n key 把两份表对上，生成「英文原文 → 中文」。同一句英文对应多条中文时按 key 顺序
/// 取第一条，保证结果确定。
pub fn build_dictionary(en: &Map<String, Value>, base: &Map<String, Value>) -> Map<String, Value> {
    let mut out = Map::new();
    for (key, en_val) in en {
        let (Some(e), Some(z)) = (en_val.as_str(), base.get(key).and_then(Value::as_str)) else {
            continue;
        };
        let (e, z) = (e.trim(), z.trim());
        if !usable_source(e) || z.is_empty() || e == z {
            continue;
        }
        out.entry(e.to_string()).or_insert_with(|| Value::String(z.to_string()));
    }
    out
}

/// 用另一对「英文 JSON / 中文 JSON」（同 key 体系）补充词典：只补缺，不覆盖已有条目。
/// 用于桌面壳菜单（Contents/Resources/en-US.json ↔ desktop-zh-CN.base.json）和
/// statsig 文案（i18n/statsig/en-US.json ↔ statsig-zh-CN.base.json）。返回新增条数；
/// 任一文件不存在返回 Ok(0)。
pub fn extend_dictionary(dict: &mut Map<String, Value>, en_path: &Path, zh_path: &Path) -> Result<usize, String> {
    if !en_path.exists() || !zh_path.exists() {
        return Ok(0);
    }
    let en = super::load_json_map(en_path)?;
    let zh = super::load_json_map(zh_path)?;
    let extra = build_dictionary(&en, &zh);
    let mut added = 0;
    for (k, v) in extra {
        if !dict.contains_key(&k) {
            dict.insert(k, v);
            added += 1;
        }
    }
    Ok(added)
}

/// 把脚本正文和配置包成一个自执行表达式，交给 webContents.executeJavaScript。
pub fn build_page_script(dict: &Map<String, Value>, cfg: &OnlineConfig) -> String {
    let config = json!({
        "host": cfg.host,
        "dict": Value::Object(dict.clone()),
        "patterns": cfg.patterns.iter().map(|(a, b)| json!([a, b])).collect::<Vec<_>>(),
        "skipSelectors": cfg.skip_selectors,
        "attrs": ["placeholder", "aria-label", "title", "alt"],
        "debounceMs": cfg.debounce_ms,
        "maxWaitMs": cfg.max_wait_ms,
        "localeLockKey": cfg.locale_lock_key,
        "localeLockValue": cfg.locale_lock_value,
        "marker": MARKER,
        "reportDelayMs": cfg.report_delay_ms.unwrap_or(0),
    });
    format!("(function(CFG){{\n{PAGE_SCRIPT}\n}})({config})")
}

/// 装进 asar：定位主视图 dom-ready 钩子，注入 `receiver.executeJavaScript(页面脚本)`，
/// 删该文件的编译缓存，重建 asar 写回原路径。返回注入报告与新头部 sha256
/// （调用方须写进 Info.plist 的 ElectronAsarIntegrity）。
pub fn install_into_asar(
    asar_path: &Path,
    page_script: &str,
) -> Result<(inject::InstallReport, String), String> {
    let mut ar = Asar::open(asar_path)?;
    let report = inject::install(&mut ar, &|hook| inject::execute_in_page(&hook.receiver, page_script))?;
    let tmp = asar_path.with_extension("asar.claudecn-new");
    let saved = ar.save_to(&tmp)?;
    std::fs::rename(&tmp, asar_path).map_err(|e| format!("写回 app.asar 失败: {e}"))?;
    Ok((report, saved.header_sha256))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(pairs: &[(&str, &str)]) -> Map<String, Value> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), Value::String(v.to_string())))
            .collect()
    }

    #[test]
    fn dictionary_filters_and_pairs_by_key() {
        let en = map(&[
            ("k1", "Log in"),
            ("k2", "Hello {name}"),
            ("k3", "Use <b>bold</b>"),
            ("k4", "Line\nbreak"),
            ("k5", "12345"),
            ("k6", "Same"),
            ("k7", " Continue "),
            ("k8", "Only in en"),
            ("k9", "Log in"),
        ]);
        let base = map(&[
            ("k1", "登录"),
            ("k2", "你好 {name}"),
            ("k3", "用<b>粗体</b>"),
            ("k4", "换行"),
            ("k5", "一二三"),
            ("k6", "Same"),
            ("k7", "继续"),
            ("k9", "登入"),
        ]);
        let d = build_dictionary(&en, &base);
        assert_eq!(d.get("Log in"), Some(&Value::String("登录".into())), "同一英文取 key 序第一条");
        assert_eq!(d.get("Continue"), Some(&Value::String("继续".into())), "两侧 trim");
        for absent in ["Hello {name}", "Use <b>bold</b>", "Line\nbreak", "12345", "Same", "Only in en"] {
            assert!(d.get(absent).is_none(), "{absent} 不应进词典");
        }
        assert_eq!(d.len(), 2);
    }

    #[test]
    fn extend_only_fills_missing_entries() {
        let tmp = std::env::temp_dir().join(format!("claudecn-extend-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        let en_p = tmp.join("en.json");
        let zh_p = tmp.join("zh.json");
        std::fs::write(&en_p, serde_json::to_string(&map(&[("a", "Copy"), ("b", "Actual Size")])).unwrap()).unwrap();
        std::fs::write(&zh_p, serde_json::to_string(&map(&[("a", "复制（菜单）"), ("b", "实际大小")])).unwrap()).unwrap();
        let mut dict = map(&[("Copy", "复制")]);
        let added = extend_dictionary(&mut dict, &en_p, &zh_p).unwrap();
        assert_eq!(added, 1);
        assert_eq!(dict.get("Copy"), Some(&Value::String("复制".into())), "前端词典优先，不被覆盖");
        assert_eq!(dict.get("Actual Size"), Some(&Value::String("实际大小".into())));
        assert_eq!(extend_dictionary(&mut dict, &tmp.join("missing.json"), &zh_p).unwrap(), 0, "文件缺失视为 0 条");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn usable_source_rules() {
        assert!(usable_source("Welcome back"));
        assert!(!usable_source(""));
        assert!(!usable_source(&"a".repeat(MAX_SOURCE_LEN + 1)));
        assert!(!usable_source("…"));
    }

    #[test]
    fn page_script_is_valid_js_and_carries_config() {
        let dict = map(&[("Log in", "登录"), ("Say \"hi\"", "说“嗨”")]);
        let cfg = OnlineConfig { report_delay_ms: Some(5), ..OnlineConfig::default() };
        let script = build_page_script(&dict, &cfg);
        assert!(script.starts_with("(function(CFG){"));
        assert!(script.contains("\"dict\":{"));
        assert!(script.contains("\"host\":\"claude.ai\""));
        assert!(script.contains("\"reportDelayMs\":5"));
        assert!(script.contains("\"localeLockKey\":null"));

        // 有 node 就做一次语法检查；没有就跳过（CI 环境可能没装）。
        let Ok(node) = which_node() else {
            eprintln!("跳过 node --check：未找到 node");
            return;
        };
        let tmp = std::env::temp_dir().join(format!("claudecn-online-{}.js", std::process::id()));
        std::fs::write(&tmp, &script).unwrap();
        let out = std::process::Command::new(node).arg("--check").arg(&tmp).output().unwrap();
        let _ = std::fs::remove_file(&tmp);
        assert!(out.status.success(), "node --check: {}", String::from_utf8_lossy(&out.stderr));
    }

    fn which_node() -> Result<std::path::PathBuf, ()> {
        let out = std::process::Command::new("sh").args(["-lc", "command -v node"]).output().map_err(|_| ())?;
        if !out.status.success() {
            return Err(());
        }
        let p = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if p.is_empty() { Err(()) } else { Ok(p.into()) }
    }
}
