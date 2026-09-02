//! 主进程注入点。在 app.asar 的 `.vite/build/*.js` 里定位主视图 webContents 的
//! `dom-ready` 处理器，把一段主进程语句用标记包起来插到处理器体开头。典型用法是
//! `receiver.executeJavaScript(页面脚本)`，把 DOM 层翻译送进远端 claude.ai 页面。
//!
//! 定位规则宁缺毋滥：整个 asar 必须恰好命中一处，0 处或多处都报错，绝不猜。
//! 注入块用 `MARK_BEGIN` / `MARK_END` 包裹，重复执行会先剥掉旧块再注入，幂等。

use regex::Regex;

use super::asar::Asar;

pub const MARK_BEGIN: &str = "/*@claudecn-online:begin*/";
pub const MARK_END: &str = "/*@claudecn-online:end*/";

/// 形如 `b.webContents.on(`dom-ready`,(()=>{` 或 `t.on("dom-ready",function(){`。
/// 捕获接收者表达式（点号链）与处理器体的起始位置。regex crate 不支持反向引用，
/// 所以引号允许不配对，实际代码里不会出现。
const HOOK_PATTERN: &str = r#"(?P<recv>[A-Za-z_$][A-Za-z0-9_$]*(?:\.[A-Za-z_$][A-Za-z0-9_$]*)*)\.on\(["'`]dom-ready["'`],\s*\(?\s*(?:\(\)\s*=>|function\s*\(\s*\))\s*\{"#;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hook {
    /// `.on(` 前面的接收者，例如 `b.webContents`。
    pub receiver: String,
    /// 整个匹配的起始字节偏移。
    pub match_start: usize,
    /// 处理器体第一个字符的偏移（紧跟 `{` 之后）。
    pub body_start: usize,
}

#[derive(Debug, Clone)]
pub struct InjectionPoint {
    /// asar 内路径，如 `/.vite/build/index.chunk-xxxx.js`。
    pub file: String,
    pub hook: Hook,
    /// 匹配处前后各约 80 字节，供日志。
    pub context: String,
}

#[derive(Debug, Clone)]
pub struct InstallReport {
    pub point: InjectionPoint,
    /// 一并删掉的 V8 编译缓存条目（`/compile-cache/<文件名>.<arch>.jsc`）。
    pub removed_caches: Vec<String>,
}

fn hook_regex() -> Regex {
    Regex::new(HOOK_PATTERN).expect("HOOK_PATTERN 合法")
}

/// 找出所有 dom-ready 处理器。
pub fn find_hooks(js: &str) -> Vec<Hook> {
    hook_regex()
        .captures_iter(js)
        .map(|c| {
            let m = c.get(0).unwrap();
            Hook {
                receiver: c["recv"].to_string(),
                match_start: m.start(),
                body_start: m.end(),
            }
        })
        .collect()
}

fn context_around(js: &str, at: usize) -> String {
    let start = js[..at].char_indices().rev().nth(80).map(|(i, _)| i).unwrap_or(0);
    let end = js[at..].char_indices().nth(160).map(|(i, _)| at + i).unwrap_or(js.len());
    js[start..end].replace('\n', " ")
}

/// 是否已含注入块。
#[allow(dead_code)]
pub fn is_injected(js: &str) -> bool {
    js.contains(MARK_BEGIN)
}

/// 剥掉全部注入块。标记不配对时原样返回剩余部分。
pub fn strip(js: &str) -> String {
    let mut out = String::with_capacity(js.len());
    let mut rest = js;
    while let Some(b) = rest.find(MARK_BEGIN) {
        out.push_str(&rest[..b]);
        let after = &rest[b + MARK_BEGIN.len()..];
        match after.find(MARK_END) {
            Some(e) => rest = &after[e + MARK_END.len()..],
            None => {
                rest = after;
                break;
            }
        }
    }
    out.push_str(rest);
    out
}

/// 在唯一的 dom-ready 处理器体开头插入 `statement`（会被 try/catch 包住）。
/// 已注入过的旧块先剥掉。返回新代码与命中的 hook（位置基于剥掉旧块后的代码）。
pub fn inject(js: &str, statement: &str) -> Result<(String, Hook), String> {
    let clean = strip(js);
    let hooks = find_hooks(&clean);
    let hook = match hooks.len() {
        1 => hooks.into_iter().next().unwrap(),
        0 => return Err("未找到 dom-ready 处理器".into()),
        n => return Err(format!("找到 {n} 个 dom-ready 处理器，拒绝盲选")),
    };
    let block = format!("{MARK_BEGIN}try{{{statement}}}catch(_claudecnErr){{}}{MARK_END}");
    let mut out = String::with_capacity(clean.len() + block.len());
    out.push_str(&clean[..hook.body_start]);
    out.push_str(&block);
    out.push_str(&clean[hook.body_start..]);
    Ok((out, hook))
}

/// 生成 `receiver.executeJavaScript(<页面脚本>, true)` 语句。脚本按 JSON 字符串字面量转义。
pub fn execute_in_page(receiver: &str, page_js: &str) -> String {
    let literal = serde_json::to_string(page_js).expect("字符串必可序列化");
    format!("{receiver}.executeJavaScript({literal},true).catch(()=>{{}})")
}

/// 扫描 asar，要求整个 `.vite/build/*.js` 里恰好一处 dom-ready 处理器。
pub fn locate(asar: &Asar) -> Result<InjectionPoint, String> {
    let mut found: Vec<InjectionPoint> = Vec::new();
    let mut scanned = 0usize;
    let mut non_utf8 = Vec::new();
    for e in asar.entries() {
        if !(e.path.starts_with("/.vite/build/") && e.path.ends_with(".js") && e.is_packed()) {
            continue;
        }
        scanned += 1;
        let bytes = asar.read(&e.path)?;
        let Ok(js) = std::str::from_utf8(&bytes) else {
            non_utf8.push(e.path.clone());
            continue;
        };
        let clean = strip(js);
        for hook in find_hooks(&clean) {
            found.push(InjectionPoint {
                file: e.path.clone(),
                context: context_around(&clean, hook.match_start),
                hook,
            });
        }
    }
    match found.len() {
        1 => Ok(found.remove(0)),
        0 => Err(format!(
            "扫描了 {scanned} 个 .vite/build/*.js，未找到 dom-ready 处理器{}",
            if non_utf8.is_empty() { String::new() } else { format!("（非 UTF-8 跳过: {}）", non_utf8.join(", ")) }
        )),
        n => Err(format!(
            "找到 {n} 处 dom-ready 处理器，拒绝盲选：\n{}",
            found
                .iter()
                .map(|p| format!("  {} @{}: …{}…", p.file, p.hook.match_start, p.context))
                .collect::<Vec<_>>()
                .join("\n")
        )),
    }
}

/// 定位 → 注入 → 替换该文件 → 删除它的 V8 编译缓存。`build_statement` 拿到 hook（含接收者）
/// 后返回要插入的主进程语句。只暂存到 `asar`，落盘由调用方 `save_to`。
pub fn install(
    asar: &mut Asar,
    build_statement: &dyn Fn(&Hook) -> String,
) -> Result<InstallReport, String> {
    let point = locate(asar)?;
    let bytes = asar.read(&point.file)?;
    let js = String::from_utf8(bytes).map_err(|_| format!("{} 不是 UTF-8", point.file))?;
    let statement = build_statement(&point.hook);
    let (patched, hook) = inject(&js, &statement)?;
    asar.replace(&point.file, patched.into_bytes())?;

    let file_name = point.file.rsplit('/').next().unwrap_or("").to_string();
    let prefix = format!("{file_name}.");
    let caches: Vec<String> = asar
        .entries()
        .into_iter()
        .filter(|e| {
            e.path.starts_with("/compile-cache/")
                && e.path.ends_with(".jsc")
                && e.path.rsplit('/').next().map(|n| n.starts_with(&prefix)).unwrap_or(false)
        })
        .map(|e| e.path)
        .collect();
    for c in &caches {
        asar.remove(c)?;
    }
    Ok(InstallReport {
        point: InjectionPoint { hook, ..point },
        removed_caches: caches,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::localize::asar::{encode_header, integrity_value};

    /// 截自官方 1.40609.1 主进程 bundle 的真实形状：反引号事件名、箭头函数外层多一对括号。
    const REAL: &str = "b.webContents.once(`ready-to-show`,(()=>L9(`main_view_ready_to_show`))),b.webContents.on(`dom-ready`,(()=>{let e=x();C=e?Date.now():void 0,izr(e?b.webContents:null,w),XN()})),nEn(b),XTn(b.webContents)";

    #[test]
    fn finds_real_shaped_hook() {
        let hooks = find_hooks(REAL);
        assert_eq!(hooks.len(), 1);
        let h = &hooks[0];
        assert_eq!(h.receiver, "b.webContents");
        assert!(REAL[h.body_start..].starts_with("let e=x();"));
        assert!(REAL[h.match_start..].starts_with("b.webContents.on(`dom-ready`"));
    }

    #[test]
    fn matches_function_form_and_quotes() {
        assert_eq!(find_hooks("t.on('dom-ready',function(){go()})").len(), 1);
        assert_eq!(find_hooks("w.webContents.on(\"dom-ready\", () => { go() })").len(), 1);
        assert_eq!(find_hooks("x.once(`dom-ready`,(()=>{}))").len(), 0, "once 不算");
        assert_eq!(find_hooks("x.on(`did-finish-load`,(()=>{}))").len(), 0);
    }

    #[test]
    fn inject_is_idempotent_and_strippable() {
        let stmt = "require('fs').writeFileSync('/tmp/x','1')";
        let (once, h1) = inject(REAL, stmt).unwrap();
        assert_eq!(h1.receiver, "b.webContents");
        assert!(is_injected(&once));
        assert_eq!(once.matches(MARK_BEGIN).count(), 1);
        let expect_block = format!("{MARK_BEGIN}try{{{stmt}}}catch(_claudecnErr){{}}{MARK_END}let e=x();");
        assert!(once.contains(&expect_block), "块应插在处理器体开头:\n{once}");

        let (twice, _) = inject(&once, "void 0").unwrap();
        assert_eq!(twice.matches(MARK_BEGIN).count(), 1, "重复注入只保留一份");
        assert!(twice.contains("try{void 0}"));
        assert!(!twice.contains(stmt));

        assert_eq!(strip(&twice), REAL);
        assert_eq!(strip(REAL), REAL);
    }

    #[test]
    fn refuses_zero_or_many() {
        assert!(inject("nothing here", "x").is_err());
        let two = format!("{REAL};{REAL}");
        let err = inject(&two, "x").unwrap_err();
        assert!(err.contains("2 个"), "{err}");
    }

    #[test]
    fn execute_in_page_escapes_as_json_literal() {
        let s = execute_in_page("b.webContents", "alert(\"hi\");\n// 中文");
        assert!(s.starts_with("b.webContents.executeJavaScript(\"alert(\\\"hi\\\");\\n// 中文\",true)"));
        assert!(s.ends_with(".catch(()=>{})"));
    }

    fn tiny_asar(main_js: &str) -> Asar {
        let main = main_js.as_bytes().to_vec();
        let cache_a = b"\x00bytecode-arm64".to_vec();
        let cache_x = b"\x00bytecode-x64".to_vec();
        let other = b"other".to_vec();
        let mut offset = 0usize;
        let mut entry = |content: &[u8]| {
            let v = serde_json::json!({"size": content.len(), "offset": offset.to_string(), "integrity": integrity_value(content)});
            offset += content.len();
            v
        };
        let header = serde_json::json!({"files": {
            ".vite": {"files": {"build": {"files": {
                "index.chunk-AbC.js": entry(&main),
                "index.js": entry(&other),
            }}}},
            "compile-cache": {"files": {
                "index.chunk-AbC.js.arm64.jsc": entry(&cache_a),
                "index.chunk-AbC.js.x64.jsc": entry(&cache_x),
                "index.js.arm64.jsc": entry(&other),
            }},
        }});
        let mut bytes = encode_header(&serde_json::to_string(&header).unwrap());
        for c in [&main, &other, &cache_a, &cache_x, &other] {
            bytes.extend_from_slice(c);
        }
        Asar::from_bytes(bytes).unwrap()
    }

    #[test]
    fn locate_and_install_on_asar() {
        let mut asar = tiny_asar(REAL);
        let p = locate(&asar).unwrap();
        assert_eq!(p.file, "/.vite/build/index.chunk-AbC.js");
        assert_eq!(p.hook.receiver, "b.webContents");
        assert!(p.context.contains("dom-ready"));

        let report = install(&mut asar, &|h| execute_in_page(&h.receiver, "void 0")).unwrap();
        assert_eq!(report.point.file, "/.vite/build/index.chunk-AbC.js");
        let mut removed = report.removed_caches.clone();
        removed.sort();
        assert_eq!(
            removed,
            vec!["/compile-cache/index.chunk-AbC.js.arm64.jsc", "/compile-cache/index.chunk-AbC.js.x64.jsc"],
            "只删该文件的缓存，index.js 的缓存保留"
        );
        assert!(asar.entry("/compile-cache/index.js.arm64.jsc").is_some());
        let patched = String::from_utf8(asar.read("/.vite/build/index.chunk-AbC.js").unwrap()).unwrap();
        assert!(patched.contains("b.webContents.executeJavaScript(\"void 0\",true)"));

        // 再次 locate 仍然只命中一处（旧块被剥掉后再匹配）
        assert!(locate(&asar).is_ok());
        assert!(locate(&tiny_asar("no hooks")).is_err());
    }
}
