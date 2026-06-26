//! macOS 专属：定位/版本、桌面层文案、重签名(ad-hoc + 过滤 entitlements)、
//! osascript 局部提权换入、ditto 备份/恢复。移植自 patch_macos.py 的 mac 相关函数。

use std::path::{Path, PathBuf};
use std::process::Command;

use plist::Value as PlistValue;
use tauri::{AppHandle, Emitter};
use walkdir::WalkDir;

use super::assets;
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
/// 去签名再 force 重签。
fn resign(app: &Path) -> Result<(), String> {
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
    let _ = std::fs::remove_dir_all(&tmp);
    if !signed.status.success() {
        return Err(format!(
            "重签名失败: {}",
            String::from_utf8_lossy(&signed.stderr)
        ));
    }
    Ok(())
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

    report(app, "重签名（ad-hoc）…");
    resign(&staged)?;
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
        resign(&staged).expect("重签名应成功");

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
}
