//! Windows 专属：多路径/注册表/MSIX 定位、takeown/icacls 提权、按文件备份 index-*.js。
//! 译文写入复用公共 mod.rs（含 §4.2① 的 en-US 覆盖、⑤ 的多 Claude 目录 locale）；
//! Windows 不强制校验签名，故无重签名步骤（§4.2③）。移植自 windows/src/{detector,patcher,backup}.rs。

use std::fs;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use walkdir::WalkDir;

use super::assets;
use super::{
    build_merged, coverage, en_source, is_patched, load_json_map, patch_whitelist,
    remove_locale_config, report, unpatch_whitelist, verify_frontend, write_frontend,
    write_locale_config, write_statsig, ZH_LOCALES,
};
use super::{ClaudeStatus, LocalizeResult};

const CREATE_NO_WINDOW: u32 = 0x08000000;

fn hidden(program: &str) -> Command {
    let mut cmd = Command::new(program);
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd
}

// ---------------------------------------------------------------- 安装探测

struct ClaudeInstallation {
    resources_dir: PathBuf,
    ion_dist_dir: PathBuf,
    version: String,
    is_msix: bool,
    package_family: Option<String>,
}

impl ClaudeInstallation {
    fn i18n_dir(&self) -> PathBuf {
        self.ion_dist_dir.join("i18n")
    }
    fn assets_dir(&self) -> PathBuf {
        self.ion_dist_dir.join("assets").join("v1")
    }
}

fn find_claude() -> Option<ClaudeInstallation> {
    find_claude_msix().or_else(find_claude_exe)
}

fn find_claude_msix() -> Option<ClaudeInstallation> {
    let base = PathBuf::from(r"C:\Program Files\WindowsApps");
    if !base.exists() {
        return None;
    }
    let mut candidates: Vec<_> = fs::read_dir(&base)
        .ok()?
        .filter_map(|e| e.ok())
        .filter(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            name.starts_with("Claude_")
                && (name.contains("_x64__") || name.contains("_arm64__") || name.contains("_x86__"))
        })
        .collect();
    candidates.sort_by_key(|e| std::cmp::Reverse(e.file_name()));

    for entry in candidates {
        let app_dir = entry.path().join("app");
        let resources_dir = app_dir.join("resources");
        if resources_dir.join("en-US.json").exists()
            || resources_dir.join("ion-dist").join("i18n").join("en-US.json").exists()
        {
            let package_name = entry.file_name().to_string_lossy().to_string();
            let version = package_name.split('_').nth(1).unwrap_or("unknown").to_string();
            return Some(ClaudeInstallation {
                ion_dist_dir: resources_dir.join("ion-dist"),
                resources_dir,
                version,
                is_msix: true,
                package_family: get_claude_package_family(),
            });
        }
    }
    None
}

fn find_claude_exe() -> Option<ClaudeInstallation> {
    let local = PathBuf::from(std::env::var("LOCALAPPDATA").ok()?);
    let mut search_bases = vec![
        local.join("AnthropicClaude"),
        local.join("Programs").join("claude-desktop"),
        local.join("Programs").join("Claude"),
        local.join("Programs").join("Claude Desktop"),
        local.join("Claude"),
        local.join("claude-desktop"),
        local.join("Anthropic").join("Claude"),
        PathBuf::from(r"C:\Program Files\Claude"),
        PathBuf::from(r"C:\Program Files\Claude Desktop"),
        PathBuf::from(r"C:\Program Files\Anthropic\Claude"),
    ];
    for reg_path in find_claude_from_registry() {
        if !search_bases.contains(&reg_path) {
            search_bases.insert(0, reg_path);
        }
    }
    for base in &search_bases {
        if !base.exists() {
            continue;
        }
        if let Some(inst) = try_detect_in_dir(base) {
            return Some(inst);
        }
    }
    None
}

fn try_detect_in_dir(base: &Path) -> Option<ClaudeInstallation> {
    // app-X.X.X 版本子目录（每次更新都变，glob 匹配，不写死版本号）
    if let Ok(read_dir) = fs::read_dir(base) {
        let mut candidates: Vec<_> = read_dir
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().starts_with("app-"))
            .collect();
        candidates.sort_by_key(|e| std::cmp::Reverse(e.file_name()));
        for entry in candidates {
            let app_dir = entry.path();
            let resources_dir = app_dir.join("resources");
            if resources_dir.join("ion-dist").join("i18n").join("en-US.json").exists()
                || resources_dir.join("en-US.json").exists()
            {
                let dir_name = entry.file_name().to_string_lossy().to_string();
                let version = dir_name.strip_prefix("app-").unwrap_or("unknown").to_string();
                return Some(ClaudeInstallation {
                    ion_dist_dir: resources_dir.join("ion-dist"),
                    resources_dir,
                    version,
                    is_msix: false,
                    package_family: None,
                });
            }
        }
    }
    // 直接 resources 目录（无版本子目录）
    let resources_dir = base.join("resources");
    if resources_dir.join("ion-dist").join("i18n").join("en-US.json").exists()
        || resources_dir.join("en-US.json").exists()
    {
        return Some(ClaudeInstallation {
            ion_dist_dir: resources_dir.join("ion-dist"),
            resources_dir,
            version: read_version_from_dir(base),
            is_msix: false,
            package_family: None,
        });
    }
    None
}

fn read_version_from_dir(dir: &Path) -> String {
    let pkg = dir.join("package.json");
    if let Ok(content) = fs::read_to_string(&pkg) {
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) {
            if let Some(v) = val.get("version").and_then(|v| v.as_str()) {
                return v.to_string();
            }
        }
    }
    if dir.join("Claude.exe").exists() {
        "installed".to_string()
    } else {
        "unknown".to_string()
    }
}

fn find_claude_from_registry() -> Vec<PathBuf> {
    let mut results = Vec::new();
    let queries = [
        (r"HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall", "InstallLocation"),
        (r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall", "InstallLocation"),
        (r"HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall", "DisplayIcon"),
        (r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall", "DisplayIcon"),
    ];
    for (root, field) in queries {
        let Ok(output) = hidden("reg")
            .args(["query", root, "/s", "/v", field])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output()
        else {
            continue;
        };
        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut current_key = String::new();
        for line in stdout.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("HKEY_") {
                current_key = trimmed.to_lowercase();
                continue;
            }
            if !trimmed.contains(field) {
                continue;
            }
            let key_is_claude =
                current_key.contains("claude") || current_key.contains("anthropic");
            let Some((_f, value)) = trimmed.split_once("REG_SZ") else {
                continue;
            };
            let value = value.trim();
            let lower = value.to_lowercase();
            let value_is_claude = lower.contains("claude") || lower.contains("anthropic");
            if !key_is_claude && !value_is_claude {
                continue;
            }
            let path = if field == "DisplayIcon" {
                match PathBuf::from(value.trim_matches(',')).parent() {
                    Some(p) => p.to_path_buf(),
                    None => continue,
                }
            } else {
                PathBuf::from(value)
            };
            if path.exists() && !results.contains(&path) {
                results.push(path);
            }
        }
    }
    results
}

fn get_claude_package_family() -> Option<String> {
    let output = hidden("powershell")
        .args([
            "-NoProfile",
            "-Command",
            "(Get-AppxPackage -Name '*Claude*' | Select-Object -First 1).PackageFamilyName",
        ])
        .output()
        .ok()?;
    let name = String::from_utf8_lossy(&output.stdout)
        .lines()
        .next()
        .unwrap_or("")
        .trim()
        .to_string();
    (!name.is_empty()).then_some(name)
}

// ---------------------------------------------------------------- detect

fn appdata_dir() -> Result<PathBuf, String> {
    std::env::var("APPDATA").map(PathBuf::from).map_err(|_| "无法获取 APPDATA".to_string())
}

pub fn detect(app: &tauri::AppHandle) -> ClaudeStatus {
    let has_backup = existing_backup().is_some();
    let Some(inst) = find_claude() else {
        return ClaudeStatus::not_installed(has_backup);
    };
    let i18n = inst.i18n_dir();
    let patched = is_patched(&i18n);
    let coverage_pct = match assets::load_base(app) {
        Ok(base) => coverage(&i18n, &base),
        Err(_) => None,
    };
    ClaudeStatus {
        installed: true,
        version: Some(inst.version.clone()),
        patched,
        has_backup,
        coverage_pct,
        dev_mode_hint: !patched,
        app_path: Some(inst.resources_dir.to_string_lossy().into_owned()),
    }
}

// ---------------------------------------------------------------- 提权 / 进程

fn grant_dir(path: &str) {
    let _ = hidden("takeown")
        .args(["/F", path, "/R", "/A", "/D", "Y"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    let _ = hidden("icacls")
        .args([path, "/grant", "Administrators:F", "/T", "/Q"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

fn take_ownership(inst: &ClaudeInstallation) -> Result<(), String> {
    let res = inst.resources_dir.to_string_lossy().to_string();
    grant_dir(&res);
    // 清掉只读属性（icacls 改 ACL 但不动只读位，write_frontend 会被只读位挡住）
    clear_readonly_recursive(&inst.ion_dist_dir);
    // 写测试：确认确实可写
    let test_file = inst.resources_dir.join(".claude_cn_test");
    match fs::write(&test_file, "test") {
        Ok(()) => {
            let _ = fs::remove_file(&test_file);
            Ok(())
        }
        Err(e) => Err(format!("无法写入 Claude 目录，请确认以管理员身份运行: {e}")),
    }
}

fn clear_readonly(path: &Path) {
    if let Ok(meta) = fs::metadata(path) {
        let mut perms = meta.permissions();
        #[allow(clippy::permissions_set_readonly_false)]
        perms.set_readonly(false);
        let _ = fs::set_permissions(path, perms);
    }
}

fn clear_readonly_recursive(dir: &Path) {
    for entry in WalkDir::new(dir).into_iter().flatten() {
        if entry.file_type().is_file() {
            clear_readonly(entry.path());
        }
    }
}

fn kill_claude() {
    let our_pid = std::process::id();
    if let Ok(output) = hidden("tasklist")
        .args(["/FI", "IMAGENAME eq Claude.exe", "/FO", "CSV", "/NH"])
        .output()
    {
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            let parts: Vec<&str> = line.split(',').collect();
            if parts.len() >= 2 {
                let pid = parts[1].trim_matches('"').trim();
                if let Ok(pid_num) = pid.parse::<u32>() {
                    if pid_num == our_pid {
                        continue;
                    }
                    let _ = hidden("taskkill")
                        .args(["/F", "/PID", &pid_num.to_string()])
                        .stdout(Stdio::null())
                        .stderr(Stdio::null())
                        .status();
                }
            }
        }
    }
    std::thread::sleep(std::time::Duration::from_secs(2));
}

fn start_claude(inst: &ClaudeInstallation) {
    if let Some(exe) = find_claude_exe_path() {
        let _ = Command::new(&exe).stdout(Stdio::null()).stderr(Stdio::null()).spawn();
        return;
    }
    if inst.is_msix {
        if let Some(family) = &inst.package_family {
            let uri = format!(r"shell:AppsFolder\{family}!Claude");
            let _ = hidden("cmd")
                .args(["/C", "start", "", &uri])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
    }
}

fn find_claude_exe_path() -> Option<PathBuf> {
    let local = PathBuf::from(std::env::var("LOCALAPPDATA").ok()?);
    let search_dirs = [
        local.join("AnthropicClaude"),
        local.join("Programs").join("claude-desktop"),
        local.join("Programs").join("Claude"),
        local.join("Programs").join("Claude Desktop"),
        local.join("Claude"),
        local.join("claude-desktop"),
        local.join("Anthropic").join("Claude"),
        PathBuf::from(r"C:\Program Files\Claude"),
        PathBuf::from(r"C:\Program Files\Claude Desktop"),
        PathBuf::from(r"C:\Program Files\Anthropic\Claude"),
    ];
    for dir in &search_dirs {
        let exe = dir.join("Claude.exe");
        if exe.exists() {
            return Some(exe);
        }
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                if entry.file_name().to_string_lossy().starts_with("app-") {
                    let exe = entry.path().join("Claude.exe");
                    if exe.exists() {
                        return Some(exe);
                    }
                }
            }
        }
    }
    None
}

// ---------------------------------------------------------------- 备份 index-*.js

// 备份只需存 whitelist 注入会改的 index-*.js（en-US 原件由 write_frontend 的 en-US.original.json 保管）。
fn backup_base() -> Option<PathBuf> {
    let local = std::env::var("LOCALAPPDATA").ok()?;
    Some(PathBuf::from(local).join("claude-localize").join("backups"))
}

fn legacy_backup_base() -> Option<PathBuf> {
    let local = std::env::var("LOCALAPPDATA").ok()?;
    Some(PathBuf::from(local).join("ClaudeCN").join("backups"))
}

/// 返回 (备份目录, 版本)；新位置优先，兼容旧 GUI 的 ClaudeCN\backups。
fn existing_backup() -> Option<(PathBuf, String)> {
    for d in [backup_base(), legacy_backup_base()].into_iter().flatten() {
        let assets = d.join("assets").join("v1");
        let has_files = assets.exists()
            && fs::read_dir(&assets).map(|mut e| e.next().is_some()).unwrap_or(false);
        if has_files {
            let ver = fs::read_to_string(d.join("backup-version.txt"))
                .map(|s| s.trim().to_string())
                .unwrap_or_default();
            return Some((d, ver));
        }
    }
    None
}

fn backup_index_js(inst: &ClaudeInstallation) -> Result<(), String> {
    let base = backup_base().ok_or("无法获取备份目录")?;
    // 同版本已存在就复用
    if let Some((_, bver)) = existing_backup() {
        if bver == inst.version {
            return Ok(());
        }
    }
    let backup_assets = base.join("assets").join("v1");
    fs::create_dir_all(&backup_assets).map_err(|e| format!("创建备份目录失败: {e}"))?;
    let assets_dir = inst.assets_dir();
    if assets_dir.exists() {
        for entry in fs::read_dir(&assets_dir).map_err(|e| e.to_string())?.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with("index-") && name.ends_with(".js") {
                let dest = backup_assets.join(&name);
                if dest.exists() {
                    continue;
                }
                let data =
                    fs::read(entry.path()).map_err(|e| format!("读取 {name} 失败: {e}"))?;
                fs::write(&dest, &data).map_err(|e| format!("写入备份 {name} 失败: {e}"))?;
            }
        }
    }
    let _ = fs::write(base.join("backup-version.txt"), &inst.version);
    let _ = fs::write(base.join("请勿删除_README.txt"), super::BACKUP_README);
    Ok(())
}

fn restore_index_js(inst: &ClaudeInstallation) -> Result<(), String> {
    let Some((base, _)) = existing_backup() else {
        return Ok(());
    };
    let backup_assets = base.join("assets").join("v1");
    let assets_dir = inst.assets_dir();
    for entry in fs::read_dir(&backup_assets).map_err(|e| e.to_string())?.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with("index-") && name.ends_with(".js") {
            let dest = assets_dir.join(&name);
            clear_readonly(&dest);
            let data = fs::read(entry.path()).map_err(|e| format!("读取备份 {name} 失败: {e}"))?;
            fs::write(&dest, &data).map_err(|e| format!("恢复 {name} 失败: {e}"))?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------- 桌面层

fn copy_force(src: &Path, dst: &Path) -> Result<(), String> {
    if let Some(parent) = dst.parent() {
        let _ = fs::create_dir_all(parent);
    }
    clear_readonly(dst);
    fs::copy(src, dst)
        .map(|_| ())
        .map_err(|e| format!("复制 {} 失败: {e}", dst.display()))
}

fn write_desktop(inst: &ClaudeInstallation, app: &tauri::AppHandle) -> Result<(), String> {
    let desktop_src = assets::desktop_base(app);
    if desktop_src.exists() {
        copy_force(&desktop_src, &inst.resources_dir.join("zh-CN.json"))?;
    }
    Ok(())
}

// ---------------------------------------------------------------- apply / restore

pub fn apply(app: &tauri::AppHandle) -> Result<LocalizeResult, String> {
    let inst = find_claude().ok_or("未检测到 Claude，请先安装 Claude 桌面端")?;
    let i18n = inst.i18n_dir();
    let base = assets::load_base(app)?;

    report(app, "正在关闭 Claude…");
    kill_claude();

    report(app, "正在获取文件权限…");
    take_ownership(&inst)?;

    report(app, "正在备份原始文件…");
    backup_index_js(&inst)?;

    report(app, "正在写入译文（覆盖 en-US）…");
    let en = load_json_map(&en_source(&i18n))?;
    let (merged, untrans) = build_merged(&en, &base);
    write_frontend(&i18n, &merged)?; // ① en-US 覆盖 + zh/zh-CN/overrides/dynamic
    write_desktop(&inst, app)?;
    write_statsig(&i18n, &assets::statsig_base(app))?;

    report(app, "正在注入语言白名单（扫 chunk，非仅 index）…");
    let wl = patch_whitelist(&inst.assets_dir());
    report(app, format!("  白名单: {wl}"));

    // 自检：渲染层 i18n 加载器会 fetch 的文件齐全且合法，否则会白屏。Windows 原地写入，
    // 失败就让用户用「恢复原版」回退（备份 + en-US.original 都在）。
    let problems = verify_frontend(&i18n);
    if !problems.is_empty() {
        return Err(format!(
            "汉化文件自检未通过：\n  - {}\n为避免白屏已停在此处，请用「恢复原版」回退后重试。",
            problems.join("\n  - ")
        ));
    }

    report(app, "正在设置语言配置…");
    write_locale_config(&appdata_dir()?)?; // ⑤ 所有 Claude/Claude-* 目录

    report(app, "正在重启 Claude…");
    start_claude(&inst);

    let ok = is_patched(&i18n);
    let en_total = en.len();
    let cov_after = if en_total > 0 {
        super::round1(100.0 * (en_total - untrans.len()) as f64 / en_total as f64)
    } else {
        0.0
    };
    let result = LocalizeResult { ok, coverage_pct: cov_after, version: inst.version.clone() };
    let _ = tauri::Emitter::emit(app, "localize-done", &result);
    Ok(result)
}

pub fn restore(app: &tauri::AppHandle, force: bool) -> Result<(), String> {
    let inst = find_claude().ok_or("未检测到 Claude，请先安装 Claude 桌面端")?;
    let i18n = inst.i18n_dir();

    let (_, backup_ver) = existing_backup().ok_or_else(|| {
        "没找到原版备份——但你不会被卡住：Claude 可从 https://claude.ai/download 免费重装即恢复英文原版；\
         聊天记录与登录在 %APPDATA%\\Claude（与程序分开存），重装不丢。"
            .to_string()
    })?;
    let patched_now = is_patched(&i18n);
    if !patched_now && !force && !backup_ver.is_empty() && backup_ver != inst.version {
        return Err(format!(
            "已中止：当前 Claude（{}）看起来不是本工具汉化的状态，而备份是旧版本（{backup_ver}）——\
             恢复会把 Claude 降级。只想要英文很可能已是原版无需恢复；确实要回到备份版本请用「强制恢复」。",
            inst.version
        ));
    }

    report(app, "正在关闭 Claude…");
    kill_claude();

    report(app, "正在获取文件权限…");
    take_ownership(&inst)?;

    report(app, "正在恢复原始文件…");
    restore_index_js(&inst)?;
    // 把注入到“支持 locale 列表”里的 zh-CN 去掉（现版数组在 chunk 里，index 备份盖不到它）。
    // 不去掉的话：还原成英文后，数组仍声明支持 zh-CN，但 zh-CN 文件已删 → 协商到 zh-CN
    // 时 fetch 404 → 还原后的英文版反而白屏。unpatch 直接从数组里移除，与备份是否完整无关。
    let _ = unpatch_whitelist(&inst.assets_dir());
    // en-US 从旁置原件还原（§① 的逆操作）
    let orig = i18n.join("en-US.original.json");
    if orig.exists() {
        let en_us = i18n.join("en-US.json");
        clear_readonly(&en_us);
        fs::copy(&orig, &en_us).map_err(|e| format!("还原 en-US.json 失败: {e}"))?;
        let _ = fs::remove_file(&orig);
    }

    report(app, "正在清理译文…");
    // 清掉 write_frontend 写的所有中文 locale（含全部别名）的 public/overrides/dynamic。
    for loc in ZH_LOCALES {
        let _ = fs::remove_file(i18n.join(format!("{loc}.json")));
        let _ = fs::remove_file(i18n.join(format!("{loc}.overrides.json")));
        let _ = fs::remove_file(i18n.join("dynamic").join(format!("{loc}.json")));
    }
    let _ = fs::remove_file(i18n.join("statsig/zh-CN.json"));
    let _ = fs::remove_file(inst.resources_dir.join("zh-CN.json"));

    report(app, "正在恢复配置…");
    remove_locale_config(&appdata_dir()?)?;

    report(app, "正在重启 Claude…");
    start_claude(&inst);

    let _ = tauri::Emitter::emit(app, "localize-done", serde_json::json!({"ok": true, "restored": true}));
    Ok(())
}
