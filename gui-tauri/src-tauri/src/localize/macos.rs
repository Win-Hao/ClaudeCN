//! macOS 专属：定位/版本、桌面层文案、重签名(ad-hoc + 过滤 entitlements)、
//! osascript 局部提权换入、ditto 备份/恢复。移植自 patch_macos.py 的 mac 相关函数。

use std::path::{Path, PathBuf};
use std::process::Command;

use plist::Value as PlistValue;
use tauri::{AppHandle, Emitter};
use walkdir::WalkDir;

use super::assets;
use super::online;
use super::{
    backup_dir, build_merged, coverage, en_source, existing_backup, find_assets_dir,
    find_i18n_dir, is_patched, load_json_map, patch_whitelist, remove_locale_config, report,
    verify_frontend, write_frontend, write_locale_config, write_statsig, BACKUP_README,
    FRONTEND_LOCALES, LOCALE,
};
use super::{ClaudeStatus, LocalizeResult};

// 重签名策略：剥掉绑定 Team/Apple 授权的 entitlements（否则新版 macOS 的 AMFI 在启动时
// 拒绝 spawn，open 报 error 163），补上 cs.* relaxations，其余原样保留以保住摄像头/麦克风等。
const STRIP_KEYS: [&str; 3] = [
    "com.apple.application-identifier",
    "keychain-access-groups",
    "com.apple.developer.team-identifier",
];
const STRIP_PREFIXES: [&str; 1] = ["com.apple.developer."];
/// Claude Desktop 的 bundle identifier；重签时用它做 identifier 级 designated requirement。
pub const BUNDLE_ID: &str = "com.anthropic.claudefordesktop";
const REQUIRED_CS: [&str; 4] = [
    "com.apple.security.cs.allow-jit",
    "com.apple.security.cs.allow-unsigned-executable-memory",
    "com.apple.security.cs.disable-library-validation",
    "com.apple.security.cs.allow-dyld-environment-variables",
];

// ---------------------------------------------------------------- 定位 / 版本

fn support_dir() -> PathBuf {
    dirs::data_dir().unwrap_or_else(|| {
        dirs::home_dir()
            .unwrap_or_default()
            .join("Library/Application Support")
    })
}

/// 按序查 /Applications/Claude.app → ~/Applications/Claude.app。
pub fn find_app(explicit: Option<&Path>) -> Option<PathBuf> {
    if let Some(p) = explicit {
        if p.exists() {
            return Some(p.to_path_buf());
        }
    }
    let mut cands = vec![PathBuf::from("/Applications/Claude.app")];
    if let Some(home) = dirs::home_dir() {
        cands.push(home.join("Applications/Claude.app"));
    }
    cands.into_iter().find(|p| p.exists())
}

/// 读 Contents/Info.plist 的 CFBundleShortVersionString。
pub fn app_version(app: &Path) -> String {
    let info = app.join("Contents/Info.plist");
    match PlistValue::from_file(&info) {
        Ok(PlistValue::Dictionary(d)) => d
            .get("CFBundleShortVersionString")
            .and_then(|v| v.as_string())
            .unwrap_or("")
            .to_string(),
        _ => String::new(),
    }
}

// ---------------------------------------------------------------- detect

pub fn detect(app: &AppHandle) -> ClaudeStatus {
    let sup = support_dir();
    let has_backup = existing_backup(&sup).is_some();
    let Some(app_path) = find_app(None) else {
        return ClaudeStatus::not_installed(has_backup);
    };
    let i18n = find_i18n_dir(&app_path);
    let patched = i18n.as_ref().map(|d| is_patched(d)).unwrap_or(false);
    let coverage_pct = match (i18n.as_ref(), assets::load_base(app)) {
        (Some(d), Ok(base)) => coverage(d, &base),
        _ => None,
    };
    ClaudeStatus {
        installed: true,
        version: Some(app_version(&app_path)),
        patched,
        has_backup,
        coverage_pct,
        dev_mode_hint: !patched,
        app_path: Some(app_path.to_string_lossy().into_owned()),
    }
}

// ---------------------------------------------------------------- 重签名

fn extract_entitlements(binary: &Path) -> plist::Dictionary {
    if let Ok(o) = Command::new("codesign")
        .args(["-d", "--entitlements", "-", "--xml"])
        .arg(binary)
        .output()
    {
        if o.status.success() && !o.stdout.is_empty() {
            if let Ok(PlistValue::Dictionary(d)) =
                PlistValue::from_reader(std::io::Cursor::new(o.stdout.as_slice()))
            {
                return d;
            }
        }
    }
    plist::Dictionary::new()
}

/// 过滤 entitlements：剥 Team/Apple 授权项，补 cs.* relaxations，其余保留。
pub fn filter_entitlements(ent: &plist::Dictionary) -> plist::Dictionary {
    let mut out = plist::Dictionary::new();
    for (k, v) in ent.iter() {
        if STRIP_KEYS.contains(&k.as_str()) {
            continue;
        }
        if STRIP_PREFIXES.iter().any(|p| k.starts_with(p)) {
            continue;
        }
        out.insert(k.clone(), v.clone());
    }
    for k in REQUIRED_CS {
        out.insert(k.to_string(), PlistValue::Boolean(true));
    }
    out
}

/// ad-hoc + hardened runtime 重签整个 app。先改 ElectronTeamID，写过滤后的 entitlements，
/// 去签名再 force 重签。`designated_requirement` 形如 `identifier "com.anthropic.claudefordesktop"`：
/// ad-hoc 默认的 DR 是 cdhash 级别，官方更新器（ShipIt）校验新包时对不上就装不进去；
/// 改成 identifier 级别后官方自动更新可以正常安装。传 None 保持旧行为。
pub(crate) fn resign(app: &Path, designated_requirement: Option<&str>) -> Result<(), String> {
    let ent = filter_entitlements(&extract_entitlements(&app.join("Contents/MacOS/Claude")));

    // ElectronTeamID 与 ad-hoc 的 TeamIdentifier 对齐（"not set"）。须在签名前改。
    let info = app.join("Contents/Info.plist");
    if let Ok(PlistValue::Dictionary(mut d)) = PlistValue::from_file(&info) {
        d.insert("ElectronTeamID".into(), PlistValue::String("not set".into()));
        let _ = PlistValue::Dictionary(d).to_file_xml(&info);
    }

    let tmp = std::env::temp_dir().join(format!("claudecn-ent-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).map_err(|e| format!("创建临时目录失败: {e}"))?;
    let entp = tmp.join("ent.plist");
    PlistValue::Dictionary(ent)
        .to_file_xml(&entp)
        .map_err(|e| format!("写 entitlements 失败: {e}"))?;

    let _ = Command::new("codesign")
        .args(["--remove-signature", "--deep"])
        .arg(app)
        .output();
    let signed = Command::new("codesign")
        .args(["--force", "--deep", "--options", "runtime", "--entitlements"])
        .arg(&entp)
        .args(["--sign", "-"])
        .arg(app)
        .output()
        .map_err(|e| format!("codesign 执行失败: {e}"))?;
    if !signed.status.success() {
        let _ = std::fs::remove_dir_all(&tmp);
        return Err(format!(
            "重签名失败: {}",
            String::from_utf8_lossy(&signed.stderr)
        ));
    }
    // DR 只能加在外层 app 上：带 --deep 时 codesign 会把同一条 DR 套到所有嵌套 helper /
    // framework，它们的 identifier 各不相同，验证必挂（"nested code is modified or invalid"）。
    // 所以第二遍不带 --deep，只重签最外层，嵌套签名原样保留。
    if let Some(dr) = designated_requirement {
        let outer = Command::new("codesign")
            .args(["--force", "--options", "runtime", "--entitlements"])
            .arg(&entp)
            .arg("-r")
            .arg(format!("=designated => {dr}"))
            .args(["--sign", "-"])
            .arg(app)
            .output()
            .map_err(|e| format!("codesign 执行失败: {e}"))?;
        if !outer.status.success() {
            let _ = std::fs::remove_dir_all(&tmp);
            return Err(format!(
                "外层 DR 重签失败: {}",
                String::from_utf8_lossy(&outer.stderr)
            ));
        }
    }
    let _ = std::fs::remove_dir_all(&tmp);
    Ok(())
}

// ---------------------------------------------------------------- asar 完整性

/// 把 Info.plist 的 `ElectronAsarIntegrity → Resources/app.asar → hash` 改成新 asar 头部
/// JSON 的 sha256。官方包的 EnableEmbeddedAsarIntegrityValidation fuse 是开着的：asar 重建后
/// 不同步这里，应用启动即崩。
pub(crate) fn write_asar_integrity(app: &Path, header_sha256: &str) -> Result<(), String> {
    let info = app.join("Contents/Info.plist");
    let PlistValue::Dictionary(mut d) =
        PlistValue::from_file(&info).map_err(|e| format!("读 Info.plist 失败: {e}"))?
    else {
        return Err("Info.plist 顶层不是字典".into());
    };
    let mut integ = match d.remove("ElectronAsarIntegrity") {
        Some(PlistValue::Dictionary(m)) => m,
        _ => plist::Dictionary::new(),
    };
    let mut entry = match integ.remove("Resources/app.asar") {
        Some(PlistValue::Dictionary(m)) => m,
        _ => plist::Dictionary::new(),
    };
    entry.insert("algorithm".into(), PlistValue::String("SHA256".into()));
    entry.insert("hash".into(), PlistValue::String(header_sha256.into()));
    integ.insert("Resources/app.asar".into(), PlistValue::Dictionary(entry));
    d.insert("ElectronAsarIntegrity".into(), PlistValue::Dictionary(integ));
    PlistValue::Dictionary(d)
        .to_file_xml(&info)
        .map_err(|e| format!("写 Info.plist 失败: {e}"))
}

/// 读出 Info.plist 里当前记录的 app.asar 头部哈希。
#[allow(dead_code)] // 测试与诊断用
pub(crate) fn read_asar_integrity(app: &Path) -> Option<String> {
    let PlistValue::Dictionary(d) = PlistValue::from_file(app.join("Contents/Info.plist")).ok()? else {
        return None;
    };
    d.get("ElectronAsarIntegrity")?
        .as_dictionary()?
        .get("Resources/app.asar")?
        .as_dictionary()?
        .get("hash")?
        .as_string()
        .map(str::to_string)
}

// ---------------------------------------------------------------- 提权换入

/// 用 osascript **局部**提权：仅「换入那一步」弹密码框，app 本身普通权限运行。
fn osascript_admin(shell_cmd: &str) -> Result<(), String> {
    let esc = shell_cmd.replace('\\', "\\\\").replace('"', "\\\"");
    let script = format!("do shell script \"{esc}\" with administrator privileges");
    let out = Command::new("osascript")
        .arg("-e")
        .arg(&script)
        .output()
        .map_err(|e| format!("osascript 执行失败: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "提权命令失败（可能取消了密码框）: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(())
}

fn quit_claude() {
    let _ = Command::new("osascript")
        .args(["-e", "tell application \"Claude\" to quit"])
        .output();
}

fn launch_claude(app: &Path) {
    let _ = Command::new("open").arg(app).output();
}

/// 把已修改的 staged app 换入 app 原位（root 权限 mv）。
fn privileged_swap(target: &Path, staged: &Path) -> Result<(), String> {
    let old = format!("/private/tmp/ClaudeCN-old-{}", std::process::id());
    let cmd = format!(
        "mv '{}' '{}' && mv '{}' '{}' && rm -rf '{}'",
        target.display(),
        old,
        staged.display(),
        target.display(),
        old
    );
    osascript_admin(&cmd)
}

// ---------------------------------------------------------------- 备份 / 桌面层

fn backup(app: &Path) -> Result<(), String> {
    let sup = support_dir();
    let ver = app_version(app);
    if let Some((_, bver)) = existing_backup(&sup) {
        if bver == ver {
            return Ok(()); // 同版本已有备份，复用，不重复占空间
        }
    }
    let bdir = backup_dir(&sup);
    std::fs::create_dir_all(&bdir).map_err(|e| format!("创建备份目录失败: {e}"))?;
    let zip = bdir.join("Claude-original.zip");
    let out = Command::new("ditto")
        .args(["-ck", "--keepParent"])
        .arg(app)
        .arg(&zip)
        .output()
        .map_err(|e| format!("ditto 执行失败: {e}"))?;
    if !out.status.success() {
        return Err(format!("备份失败: {}", String::from_utf8_lossy(&out.stderr)));
    }
    let _ = std::fs::write(bdir.join("backup-version.txt"), &ver);
    let _ = std::fs::write(bdir.join("请勿删除_README.txt"), BACKUP_README);
    Ok(())
}

/// 桌面菜单文案 + 原生 strings（mac 专属 Contents/Resources 布局）。
fn write_desktop(app: &Path, app_handle: &AppHandle) -> Result<(), String> {
    let res = app.join("Contents/Resources");
    let desktop_src = assets::desktop_base(app_handle);
    if desktop_src.exists() {
        std::fs::copy(&desktop_src, res.join(format!("{LOCALE}.json")))
            .map_err(|e| format!("写桌面菜单译文失败: {e}"))?;
    }
    let strings = assets::strings_base(app_handle);
    if strings.exists() {
        for folder in ["zh-CN.lproj", "zh_CN.lproj"] {
            let d = res.join(folder);
            std::fs::create_dir_all(&d).ok();
            std::fs::copy(&strings, d.join("Localizable.strings"))
                .map_err(|e| format!("写 Localizable.strings 失败: {e}"))?;
        }
    }
    Ok(())
}

fn write_apple_languages() {
    let _ = Command::new("defaults")
        .args([
            "write",
            "com.anthropic.claudefordesktop",
            "AppleLanguages",
            "-array",
            "zh-CN",
            "zh-Hans",
            "en",
        ])
        .output();
}

fn remove_apple_languages() {
    let _ = Command::new("defaults")
        .args(["delete", "com.anthropic.claudefordesktop", "AppleLanguages"])
        .output();
}

// ---------------------------------------------------------------- apply 编排

pub fn apply(app: &AppHandle) -> Result<LocalizeResult, String> {
    let app_path = find_app(None).ok_or("未检测到 Claude，请先从 claude.ai 安装")?;
    let i18n_src = find_i18n_dir(&app_path).ok_or("未找到 i18n 目录，无法汉化")?;
    let rel_i18n = i18n_src
        .strip_prefix(&app_path)
        .map_err(|_| "i18n 路径异常".to_string())?
        .to_path_buf();
    let assets_src = find_assets_dir(&app_path);
    let rel_assets = assets_src
        .as_ref()
        .and_then(|a| a.strip_prefix(&app_path).ok())
        .map(|p| p.to_path_buf());

    let base = assets::load_base(app)?;

    report(app, "备份原版…");
    backup(&app_path)?;

    report(app, "复制到临时目录…");
    let tmp = std::env::temp_dir().join(format!("claudecn-patch-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).map_err(|e| format!("创建临时目录失败: {e}"))?;
    let staged = tmp.join("Claude.app");
    let cp = Command::new("cp")
        .arg("-R")
        .arg(&app_path)
        .arg(&staged)
        .output()
        .map_err(|e| format!("cp 执行失败: {e}"))?;
    if !cp.status.success() {
        let _ = std::fs::remove_dir_all(&tmp);
        return Err(format!("复制失败: {}", String::from_utf8_lossy(&cp.stderr)));
    }

    let i18n = staged.join(&rel_i18n);
    let assets_dir = rel_assets.as_ref().map(|r| staged.join(r));

    report(app, "合并译文…");
    let en = load_json_map(&en_source(&i18n))?;
    let (merged, untrans) = build_merged(&en, &base);
    write_frontend(&i18n, &merged)?;
    write_desktop(&staged, app)?;
    write_statsig(&i18n, &assets::statsig_base(app))?;

    report(app, "处理语言白名单（扫 chunk，非仅 index）…");
    let wl = match &assets_dir {
        Some(a) => patch_whitelist(a),
        None => "no-assets".to_string(),
    };
    report(app, format!("  白名单: {wl}"));

    // 换入前自检：拦截会导致白屏的损坏。失败就直接中止——此刻还没动 /Applications 里的 app。
    report(app, "自检渲染层 i18n（避免换入后白屏）…");
    let problems = verify_frontend(&i18n);
    if !problems.is_empty() {
        let _ = std::fs::remove_dir_all(&tmp);
        return Err(format!(
            "汉化文件自检未通过，已中止（你的 Claude 未被改动）：\n  - {}\n这道关专门拦截换入后会白屏的损坏。请重试；若反复失败，可能是该 Claude 版本结构变了。",
            problems.join("\n  - ")
        ));
    }
    report(app, format!("  自检通过：{} 个 locale 前端文件齐全且合法", FRONTEND_LOCALES.len()));

    // OAuth 模式：官方账号登录后主窗口是远端 claude.ai，本地 i18n 碰不到，得往主进程的
    // dom-ready 钩子里注入 DOM 层翻译脚本。失败不中止：本地界面汉化照常，只是登录后的页面保持英文。
    report(app, "安装在线页面汉化（官方账号登录后的 claude.ai 界面）…");
    let mut dict = online::build_dictionary(&en, &base);
    let n_frontend = dict.len();
    // 桌面壳菜单与 statsig 文案只补缺；它们的英文源在 staged app 里未被改动。
    let n_desktop = online::extend_dictionary(
        &mut dict,
        &staged.join("Contents/Resources/en-US.json"),
        &assets::desktop_base(app),
    )
    .unwrap_or_else(|e| {
        report(app, format!("  桌面菜单词典跳过：{e}"));
        0
    });
    let n_statsig = online::extend_dictionary(
        &mut dict,
        &i18n.join("statsig/en-US.json"),
        &assets::statsig_base(app),
    )
    .unwrap_or_else(|e| {
        report(app, format!("  statsig 词典跳过：{e}"));
        0
    });
    report(app, format!("  在线词典：前端 {n_frontend} 条 + 桌面菜单补 {n_desktop} 条 + statsig 补 {n_statsig} 条"));
    let page_script = online::build_page_script(&dict, &online::OnlineConfig::default());
    match online::install_into_asar(&staged.join("Contents/Resources/app.asar"), &page_script) {
        Ok((rep, header_sha)) => {
            write_asar_integrity(&staged, &header_sha)?;
            report(
                app,
                format!(
                    "  词典 {} 条；注入 {}（接收者 {}）；清理编译缓存 {} 个",
                    dict.len(),
                    rep.point.file,
                    rep.point.hook.receiver,
                    rep.removed_caches.len()
                ),
            );
        }
        Err(e) => report(app, format!("  ⚠️ 未安装在线页面汉化（本地界面汉化不受影响）：{e}")),
    }

    // identifier 级 DR：官方自动更新仍可安装；更新后重跑一次汉化即可，不再把用户锁在旧版本。
    report(app, "重签名（ad-hoc，保留官方自动更新）…");
    resign(&staged, Some(&format!("identifier \"{BUNDLE_ID}\"")))?;
    let _ = Command::new("xattr")
        .args(["-dr", "com.apple.quarantine"])
        .arg(&staged)
        .output();

    report(app, "关闭 Claude 并换入（需要管理员密码）…");
    quit_claude();
    if let Err(e) = privileged_swap(&app_path, &staged) {
        let _ = std::fs::remove_dir_all(&tmp);
        return Err(e);
    }
    let _ = std::fs::remove_dir_all(&tmp);

    report(app, "写语言配置…");
    let sup = support_dir();
    write_locale_config(&sup)?;
    write_apple_languages();

    let i18n_final = find_i18n_dir(&app_path);
    let ok = i18n_final.as_ref().map(|d| is_patched(d)).unwrap_or(false);

    report(app, "重启 Claude…");
    launch_claude(&app_path);

    let en_total = en.len();
    let cov_after = if en_total > 0 {
        super::round1(100.0 * (en_total - untrans.len()) as f64 / en_total as f64)
    } else {
        0.0
    };
    let result = LocalizeResult {
        ok,
        coverage_pct: cov_after,
        version: app_version(&app_path),
    };
    let _ = app.emit("localize-done", &result);
    Ok(result)
}

// ---------------------------------------------------------------- restore 编排

pub fn restore(app: &AppHandle, force: bool) -> Result<(), String> {
    let app_path = find_app(None).unwrap_or_else(|| PathBuf::from("/Applications/Claude.app"));
    let sup = support_dir();
    let (backup_zip, backup_ver) = existing_backup(&sup).ok_or_else(|| {
        "没找到原版备份——但你不会被卡住：\n\
         · Claude.app 可从 https://claude.ai/download 免费重装，即恢复官方英文原版；\n\
         · 聊天记录和登录都在 ~/Library/Application Support/Claude/（与 app 分开存），重装不丢。\n\
         想要英文就重装一下 Claude；想保留中文则什么都不用做。"
            .to_string()
    })?;

    let cur_ver = app_version(&app_path);
    let patched_now = find_i18n_dir(&app_path)
        .map(|d| is_patched(&d))
        .unwrap_or(false);
    // 防降级：当前已不是本工具汉化状态、且备份是旧版本（多半 Claude 自己更新过）→ 中止
    if !patched_now && !force && !backup_ver.is_empty() && backup_ver != cur_ver {
        return Err(format!(
            "已中止：当前 Claude（{cur_ver}）看起来不是本工具汉化的状态，而备份是旧版本（{backup_ver}）——\
             恢复会把 Claude 降级到 {backup_ver}。\n\
             · 只想要英文？现在很可能已是英文原版，无需恢复。\n\
             · 确实要回到备份那个版本：用「强制恢复」。"
        ));
    }

    report(app, "关闭 Claude…");
    quit_claude();

    report(app, "解压备份…");
    let tmp = std::env::temp_dir().join(format!("claudecn-restore-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).map_err(|e| format!("创建临时目录失败: {e}"))?;
    let out = Command::new("ditto")
        .args(["-xk"])
        .arg(&backup_zip)
        .arg(&tmp)
        .output()
        .map_err(|e| format!("ditto 执行失败: {e}"))?;
    if !out.status.success() {
        let _ = std::fs::remove_dir_all(&tmp);
        return Err(format!("解压备份失败: {}", String::from_utf8_lossy(&out.stderr)));
    }
    let mut restored = tmp.join("Claude.app");
    if !restored.exists() {
        // ditto --keepParent 可能多一层
        restored = WalkDir::new(&tmp)
            .into_iter()
            .flatten()
            .map(|e| e.path().to_path_buf())
            .find(|p| p.file_name().map(|n| n == "Claude.app").unwrap_or(false))
            .ok_or("备份内容异常，找不到 Claude.app")?;
    }

    report(app, "换入原版（需要管理员密码）…");
    if let Err(e) = privileged_swap(&app_path, &restored) {
        let _ = std::fs::remove_dir_all(&tmp);
        return Err(e);
    }
    let _ = std::fs::remove_dir_all(&tmp);

    report(app, "清理语言配置…");
    let _ = remove_locale_config(&sup);
    remove_apple_languages();

    report(app, "重启 Claude…");
    launch_claude(&app_path);
    let _ = app.emit("localize-done", serde_json::json!({"ok": true, "restored": true}));
    Ok(())
}

// ---------------------------------------------------------------- 单元测试

#[cfg(test)]
mod tests {
    use super::*;

    fn dict_from(pairs: &[(&str, bool)]) -> plist::Dictionary {
        let mut d = plist::Dictionary::new();
        for (k, v) in pairs {
            d.insert((*k).to_string(), PlistValue::Boolean(*v));
        }
        d
    }

    #[test]
    fn filter_strips_team_bound_and_adds_cs_relaxations() {
        let mut d = dict_from(&[
            ("com.apple.application-identifier", true),
            ("keychain-access-groups", true),
            ("com.apple.developer.team-identifier", true),
            ("com.apple.developer.aps-environment", true), // developer.* 前缀 → 剥
            ("com.apple.security.device.camera", true),    // 保留（ad-hoc 下可用）
            ("com.apple.security.device.audio-input", true),
        ]);
        d.insert(
            "com.apple.security.personal-information.location".into(),
            PlistValue::Boolean(true),
        );
        let out = filter_entitlements(&d);

        // 剥掉的
        assert!(!out.contains_key("com.apple.application-identifier"));
        assert!(!out.contains_key("keychain-access-groups"));
        assert!(!out.contains_key("com.apple.developer.team-identifier"));
        assert!(!out.contains_key("com.apple.developer.aps-environment"));
        // 保留的（保住摄像头/麦克风/定位能力）
        assert!(out.contains_key("com.apple.security.device.camera"));
        assert!(out.contains_key("com.apple.security.device.audio-input"));
        assert!(out.contains_key("com.apple.security.personal-information.location"));
        // 补上的 cs.* relaxations（全 true）
        for k in REQUIRED_CS {
            assert_eq!(out.get(k).and_then(|v| v.as_boolean()), Some(true));
        }
    }

    #[test]
    fn filter_on_empty_yields_only_cs_relaxations() {
        let out = filter_entitlements(&plist::Dictionary::new());
        assert_eq!(out.len(), REQUIRED_CS.len());
        for k in REQUIRED_CS {
            assert!(out.contains_key(k));
        }
    }

    /// 端到端（除提权换入外）：拷贝真实 Claude.app → 合并覆盖 en-US → ad-hoc 重签，
    /// 校验译文注入 + 签名有效 + entitlements 过滤正确。**不动用户真实安装、无需密码**。
    /// 仅在本机装有 Claude 时手动跑：`cargo test -- --ignored real_app`
    #[test]
    #[ignore]
    fn real_app_apply_pipeline_on_copy() {
        let Some(app) = find_app(None) else {
            eprintln!("跳过：本机未安装 Claude.app");
            return;
        };
        let tmp = std::env::temp_dir().join(format!("claudecn-itest-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let staged = tmp.join("Claude.app");
        let cp = Command::new("cp").arg("-R").arg(&app).arg(&staged).output().unwrap();
        assert!(cp.status.success(), "cp 失败: {}", String::from_utf8_lossy(&cp.stderr));

        let i18n = find_i18n_dir(&staged).expect("找不到 i18n 目录");
        let base = load_json_map(
            &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/zh-CN.base.json"),
        )
        .unwrap();
        let en = load_json_map(&en_source(&i18n)).unwrap();
        let en_count = en.len();
        let (merged, untrans) = build_merged(&en, &base);
        // 全覆盖 = 每个 en key 都在 merged（merged 还可能含 base 多出来的旧 key，故 len 可更大）
        assert!(en.keys().all(|k| merged.contains_key(k)), "merged 必须覆盖全部 en key");
        assert!(merged.len() >= en_count);
        write_frontend(&i18n, &merged).unwrap();

        // 译文注入校验
        assert!(i18n.join("en-US.original.json").exists(), "应留下纯净英文原件");
        let patched_en = std::fs::read_to_string(i18n.join("en-US.json")).unwrap();
        assert!(
            patched_en.chars().any(|c| ('\u{4E00}'..='\u{9FFF}').contains(&c)),
            "en-US.json 覆盖后应含中文"
        );
        assert!(i18n.join("zh-CN.json").exists());
        let cov = 100.0 * (en_count - untrans.len()) as f64 / en_count as f64;
        println!(
            "真实版本覆盖率: {:.1}%（{}/{} 已译）",
            cov,
            en_count - untrans.len(),
            en_count
        );

        // 重签名
        resign(&staged, None).expect("重签名应成功");

        // 签名结构有效
        let verify = Command::new("codesign")
            .args(["--verify", "--deep", "--strict"])
            .arg(&staged)
            .output()
            .unwrap();
        assert!(
            verify.status.success(),
            "codesign --verify 应通过: {}",
            String::from_utf8_lossy(&verify.stderr)
        );

        // ad-hoc 签名（TeamIdentifier 不再绑定 Anthropic）
        let dv = Command::new("codesign").args(["-dvvv"]).arg(&staged).output().unwrap();
        let dv_err = String::from_utf8_lossy(&dv.stderr);
        assert!(dv_err.contains("Signature=adhoc"), "应为 ad-hoc 签名，实际:\n{dv_err}");

        // entitlements 过滤正确：剥掉 team 绑定项，补上 cs.*
        let ent = extract_entitlements(&staged.join("Contents/MacOS/Claude"));
        for k in STRIP_KEYS {
            assert!(!ent.contains_key(k), "重签后不应残留 {k}");
        }
        assert!(
            !ent.keys().any(|k| k.starts_with("com.apple.developer.")),
            "不应残留 com.apple.developer.*"
        );
        for k in REQUIRED_CS {
            assert!(ent.contains_key(k), "应补上 {k}");
        }

        let _ = std::fs::remove_dir_all(&tmp);
        println!("✅ 真实 app 汉化流水线（除提权换入外）全部通过");
    }

    #[test]
    fn write_and_read_asar_integrity_roundtrip() {
        let tmp = std::env::temp_dir().join(format!("claudecn-plist-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let app = tmp.join("Fake.app");
        std::fs::create_dir_all(app.join("Contents")).unwrap();
        let mut asar_entry = plist::Dictionary::new();
        asar_entry.insert("algorithm".into(), PlistValue::String("SHA256".into()));
        asar_entry.insert("hash".into(), PlistValue::String("old".into()));
        let mut integ = plist::Dictionary::new();
        integ.insert("Resources/app.asar".into(), PlistValue::Dictionary(asar_entry));
        let mut root = plist::Dictionary::new();
        root.insert("CFBundleIdentifier".into(), PlistValue::String(BUNDLE_ID.into()));
        root.insert("ElectronAsarIntegrity".into(), PlistValue::Dictionary(integ));
        PlistValue::Dictionary(root).to_file_xml(app.join("Contents/Info.plist")).unwrap();

        assert_eq!(read_asar_integrity(&app).as_deref(), Some("old"));
        write_asar_integrity(&app, "deadbeef").unwrap();
        assert_eq!(read_asar_integrity(&app).as_deref(), Some("deadbeef"));
        // 其他键保留
        let PlistValue::Dictionary(d) = PlistValue::from_file(app.join("Contents/Info.plist")).unwrap() else { panic!() };
        assert_eq!(d.get("CFBundleIdentifier").and_then(|v| v.as_string()), Some(BUNDLE_ID));
        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// 端到端：拷贝官方 Claude.app → 在 asar 主进程 dom-ready 处理器里注入一条“写标记文件”
    /// 语句 → 重建 asar、更新 Info.plist 完整性哈希、ad-hoc 重签（identifier 级 DR）→
    /// 用独立数据目录启动 → 等标记文件出现。证明：定位正确、asar 重建后 Electron 完整性
    /// 校验通过、删掉的编译缓存不会抢先加载旧字节码。不动用户真实安装、无需密码、无需登录。
    /// 用法：`CLAUDECN_E2E_APP=/path/to/Claude.app cargo test -- --ignored online_hook --nocapture`
    #[test]
    #[ignore]
    fn real_app_online_hook_launches() {
        use super::super::asar::Asar;
        use super::super::inject;

        let app = std::env::var("CLAUDECN_E2E_APP")
            .ok()
            .map(PathBuf::from)
            .or_else(|| find_app(None));
        let Some(app) = app else {
            eprintln!("跳过：未指定 CLAUDECN_E2E_APP 且本机未安装 Claude.app");
            return;
        };
        let tmp = std::env::temp_dir().join(format!("claudecn-e2e-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let staged = tmp.join("Claude.app");
        let cp = Command::new("cp").arg("-R").arg(&app).arg(&staged).output().unwrap();
        assert!(cp.status.success(), "cp 失败: {}", String::from_utf8_lossy(&cp.stderr));
        let _ = Command::new("xattr").args(["-dr", "com.apple.quarantine"]).arg(&staged).output();

        let marker = tmp.join("hook-ran.txt");
        let marker_lit = serde_json::to_string(marker.to_string_lossy().as_ref()).unwrap();
        let asar_path = staged.join("Contents/Resources/app.asar");
        let old_hash = read_asar_integrity(&staged).expect("官方包应带 ElectronAsarIntegrity");

        let mut ar = Asar::open(&asar_path).expect("解析 app.asar");
        let report = inject::install(&mut ar, &|h| {
            format!(
                "require(\"fs\").writeFileSync({marker_lit},String(Date.now()));{}",
                inject::execute_in_page(&h.receiver, "void 0")
            )
        })
        .expect("定位并注入");
        println!(
            "注入点: {} @{} 接收者={} 删缓存={:?}",
            report.point.file, report.point.hook.match_start, report.point.hook.receiver, report.removed_caches
        );
        let new_asar = tmp.join("app.asar.new");
        let save = ar.save_to(&new_asar).expect("重建 asar");
        std::fs::rename(&new_asar, &asar_path).unwrap();
        assert_ne!(save.header_sha256, old_hash);
        assert!(Asar::open(&asar_path).is_ok(), "重建后的 asar 应可再次解析");

        write_asar_integrity(&staged, &save.header_sha256).unwrap();
        assert_eq!(read_asar_integrity(&staged).as_deref(), Some(save.header_sha256.as_str()));
        resign(&staged, Some(&format!("identifier \"{BUNDLE_ID}\""))).expect("重签名");
        let verify = Command::new("codesign")
            .args(["--verify", "--deep", "--strict"])
            .arg(&staged)
            .output()
            .unwrap();
        assert!(verify.status.success(), "codesign --verify: {}", String::from_utf8_lossy(&verify.stderr));
        let dr = Command::new("codesign").args(["-d", "-r", "-"]).arg(&staged).output().unwrap();
        let dr_txt = String::from_utf8_lossy(&dr.stderr).to_string() + &String::from_utf8_lossy(&dr.stdout);
        assert!(dr_txt.contains(&format!("identifier \"{BUNDLE_ID}\"")), "DR 应为 identifier 级: {dr_txt}");

        let profile = tmp.join("profile");
        let mut child = Command::new(staged.join("Contents/MacOS/Claude"))
            .arg(format!("--user-data-dir={}", profile.display()))
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("启动 Claude");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(40);
        let mut ran = false;
        while std::time::Instant::now() < deadline {
            if marker.exists() {
                ran = true;
                break;
            }
            if let Ok(Some(status)) = child.try_wait() {
                panic!("Claude 提前退出（{status}）：多半是 asar 完整性或签名没过");
            }
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
        let _ = child.kill();
        let _ = child.wait();
        let _ = Command::new("pkill")
            .arg("-f")
            .arg(format!("user-data-dir={}", profile.display()))
            .output();
        let content = std::fs::read_to_string(&marker).unwrap_or_default();
        let _ = std::fs::remove_dir_all(&tmp);
        assert!(ran, "40 秒内标记文件未出现：dom-ready 注入未执行");
        println!("✅ 注入语句在真实 app 的 dom-ready 里执行了（标记时间戳 {content}）");
    }

    /// 端到端二：注入真实的页面翻译脚本，在未登录的 claude.ai 登录页上验证脚本跑通、
    /// 词典命中，并把统计写回标记文件。
    /// 用法：`CLAUDECN_E2E_APP=/path/to/Claude.app cargo test -- --ignored online_translation --nocapture`
    #[test]
    #[ignore]
    fn real_app_online_translation_on_login_page() {
        use super::super::asar::Asar;
        use super::super::{inject, online};

        let app = std::env::var("CLAUDECN_E2E_APP")
            .ok()
            .map(PathBuf::from)
            .or_else(|| find_app(None));
        let Some(app) = app else {
            eprintln!("跳过：未指定 CLAUDECN_E2E_APP 且本机未安装 Claude.app");
            return;
        };
        let tmp = std::env::temp_dir().join(format!("claudecn-e2e2-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let staged = tmp.join("Claude.app");
        let cp = Command::new("cp").arg("-R").arg(&app).arg(&staged).output().unwrap();
        assert!(cp.status.success(), "cp 失败: {}", String::from_utf8_lossy(&cp.stderr));
        let _ = Command::new("xattr").args(["-dr", "com.apple.quarantine"]).arg(&staged).output();

        let i18n = find_i18n_dir(&staged).expect("找不到 i18n 目录");
        let en = load_json_map(&en_source(&i18n)).unwrap();
        let base = load_json_map(
            &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/zh-CN.base.json"),
        )
        .unwrap();
        let mut dict = online::build_dictionary(&en, &base);
        let n_frontend = dict.len();
        let res_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources");
        let n_desktop = online::extend_dictionary(&mut dict, &staged.join("Contents/Resources/en-US.json"), &res_dir.join("desktop-zh-CN.base.json")).unwrap();
        let n_statsig = online::extend_dictionary(&mut dict, &i18n.join("statsig/en-US.json"), &res_dir.join("statsig-zh-CN.base.json")).unwrap();
        println!("在线词典：前端 {n_frontend} + 桌面菜单 {n_desktop} + statsig {n_statsig} = {}", dict.len());
        assert!(dict.len() > 5000, "词典太小: {}", dict.len());
        let cfg = online::OnlineConfig { report_delay_ms: Some(10_000), ..Default::default() };
        let script = online::build_page_script(&dict, &cfg);
        let script_lit = serde_json::to_string(&script).unwrap();

        let marker = tmp.join("stats.json");
        let marker_lit = serde_json::to_string(marker.to_string_lossy().as_ref()).unwrap();
        let asar_path = staged.join("Contents/Resources/app.asar");
        let mut ar = Asar::open(&asar_path).expect("解析 app.asar");
        let report = inject::install(&mut ar, &|h| {
            format!(
                "{recv}.executeJavaScript({script_lit},true).then(function(r){{require(\"fs\").writeFileSync({marker_lit},JSON.stringify(r))}}).catch(function(e){{require(\"fs\").writeFileSync({marker_lit},\"ERR:\"+e)}})",
                recv = h.receiver
            )
        })
        .expect("定位并注入");
        println!("注入点: {} 接收者={} 词典={} 条 脚本={} KB", report.point.file, report.point.hook.receiver, dict.len(), script.len() / 1024);
        let new_asar = tmp.join("app.asar.new");
        let save = ar.save_to(&new_asar).expect("重建 asar");
        std::fs::rename(&new_asar, &asar_path).unwrap();
        write_asar_integrity(&staged, &save.header_sha256).unwrap();
        resign(&staged, Some(&format!("identifier \"{BUNDLE_ID}\""))).expect("重签名");

        let profile = tmp.join("profile");
        let mut child = Command::new(staged.join("Contents/MacOS/Claude"))
            .arg(format!("--user-data-dir={}", profile.display()))
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("启动 Claude");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(75);
        let mut got = None;
        while std::time::Instant::now() < deadline {
            if let Ok(c) = std::fs::read_to_string(&marker) {
                if !c.is_empty() {
                    got = Some(c);
                    break;
                }
            }
            if let Ok(Some(status)) = child.try_wait() {
                panic!("Claude 提前退出（{status}）");
            }
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
        let _ = child.kill();
        let _ = child.wait();
        let _ = Command::new("pkill").arg("-f").arg(format!("user-data-dir={}", profile.display())).output();
        let _ = std::fs::remove_dir_all(&tmp);

        let got = got.expect("75 秒内页面脚本没有回报统计");
        assert!(!got.starts_with("ERR:"), "页面脚本执行出错: {got}");
        let stats: serde_json::Value = serde_json::from_str(&got).expect("统计应是 JSON");
        println!("页面统计: {}", serde_json::to_string_pretty(&stats).unwrap());
        assert!(stats.get("skipped").is_none(), "脚本因 host 不匹配跳过了: {stats}");
        assert!(stats["dict"].as_u64().unwrap_or(0) > 5000, "词典没送进页面");
        assert_eq!(stats["errors"].as_array().map(|a| a.len()).unwrap_or(99), 0, "脚本内部报错: {}", stats["errors"]);
        println!(
            "✅ 登录页实测：替换文本 {} 处、属性 {} 处、遍历节点 {}、批次 {}",
            stats["replaced"], stats["attrs"], stats["nodes"], stats["passes"]
        );
    }
}
