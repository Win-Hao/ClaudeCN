use std::fs;
use std::path::Path;

use regex::Regex;
use serde_json::Value;

use crate::backup;
use crate::detector::{config_path, ClaudeInstallation};
use crate::logger;

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x08000000;

const FRONTEND_ZH_CN: &str = include_str!("../resources/zh-CN.json");
const DESKTOP_ZH_CN: &str = include_str!("../resources/desktop-zh-CN.json");
const STATSIG_ZH_CN: &str = include_str!("../resources/statsig-zh-CN.json");

#[derive(Debug)]
pub enum PatchError {
    Backup(String),
    Io(String),
    Whitelist(String),
    Config(String),
}

impl std::fmt::Display for PatchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PatchError::Backup(m) => write!(f, "备份失败: {}", m),
            PatchError::Io(m) => write!(f, "文件操作失败: {}", m),
            PatchError::Whitelist(m) => write!(f, "语言白名单注入失败: {}", m),
            PatchError::Config(m) => write!(f, "配置文件修改失败: {}", m),
        }
    }
}

pub fn apply_patch(
    installation: &ClaudeInstallation,
    on_progress: &dyn Fn(&str),
) -> Result<(), PatchError> {
    logger::log(&format!("apply_patch: resources_dir={}", installation.resources_dir.display()));

    on_progress("正在关闭 Claude...");
    logger::log("step: kill_claude");
    kill_claude();
    logger::log("step: kill_claude done");

    on_progress("正在获取文件权限...");
    logger::log("step: take_ownership");
    take_ownership(installation)?;
    logger::log("step: take_ownership done");

    on_progress("正在备份原始文件...");
    logger::log("step: create_backup");
    backup::create_backup(installation).map_err(|e| {
        logger::log(&format!("create_backup FAILED: {}", e));
        PatchError::Backup(e.to_string())
    })?;
    logger::log("step: create_backup done");

    on_progress("正在写入翻译文件...");
    logger::log("step: write_translation_files");
    logger::log(&format!("  ion_dist_dir={}", installation.ion_dist_dir.display()));
    logger::log(&format!("  ion_dist exists={}", installation.ion_dist_dir.exists()));
    logger::log(&format!("  i18n dir exists={}", installation.ion_dist_dir.join("i18n").exists()));
    write_translation_files(installation)?;
    logger::log("step: write_translation_files done");

    let zh_cn_path = installation.ion_dist_dir.join("i18n").join("zh-CN.json");
    let desktop_zh = installation.resources_dir.join("zh-CN.json");
    logger::log(&format!("verify: i18n/zh-CN.json exists={} size={}",
        zh_cn_path.exists(),
        fs::metadata(&zh_cn_path).map(|m| m.len()).unwrap_or(0)));
    logger::log(&format!("verify: resources/zh-CN.json exists={} size={}",
        desktop_zh.exists(),
        fs::metadata(&desktop_zh).map(|m| m.len()).unwrap_or(0)));

    on_progress("正在注入语言白名单...");
    logger::log("step: patch_language_whitelist");
    patch_language_whitelist(installation)?;
    logger::log("step: patch_language_whitelist done");

    on_progress("正在设置语言配置...");
    logger::log("step: set_config_locale");
    set_config_locale()?;
    if let Some(cp) = config_path() {
        logger::log(&format!("verify: config path={}", cp.display()));
        if let Ok(content) = fs::read_to_string(&cp) {
            logger::log(&format!("verify: config contains zh-CN={}", content.contains("zh-CN")));
        }
    }
    logger::log("step: set_config_locale done");

    on_progress("正在重启 Claude...");
    logger::log("step: start_claude");
    start_claude();
    logger::log("apply_patch completed successfully");

    Ok(())
}

pub fn remove_patch(
    installation: &ClaudeInstallation,
    on_progress: &dyn Fn(&str),
) -> Result<(), PatchError> {
    logger::log("remove_patch: starting");

    on_progress("正在关闭 Claude...");
    logger::log("step: kill_claude");
    kill_claude();
    logger::log("step: kill_claude done");

    on_progress("正在获取文件权限...");
    logger::log("step: take_ownership");
    take_ownership(installation)?;
    logger::log("step: take_ownership done");

    on_progress("正在恢复原始文件...");
    logger::log("step: restore_backup");
    backup::restore_backup(installation).map_err(|e| {
        logger::log(&format!("restore_backup FAILED: {}", e));
        PatchError::Backup(e.to_string())
    })?;
    logger::log("step: restore_backup done");

    on_progress("正在清理翻译文件...");
    let _ = fs::remove_file(installation.resources_dir.join("zh-CN.json"));
    let _ = fs::remove_file(installation.ion_dist_dir.join("i18n").join("zh-CN.json"));
    let _ = fs::remove_file(
        installation
            .ion_dist_dir
            .join("i18n")
            .join("statsig")
            .join("zh-CN.json"),
    );

    on_progress("正在恢复配置...");
    remove_config_locale()?;

    on_progress("正在重启 Claude...");
    start_claude();

    Ok(())
}

fn hidden_cmd(program: &str) -> std::process::Command {
    let mut cmd = std::process::Command::new(program);
    #[cfg(target_os = "windows")]
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd
}

fn grant_dir(path: &str) {
    let _ = hidden_cmd("takeown")
        .args(["/F", path, "/R", "/A", "/D", "Y"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
    let _ = hidden_cmd("icacls")
        .args([path, "/grant", "Administrators:F", "/T", "/Q"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
}

fn take_ownership(installation: &ClaudeInstallation) -> Result<(), PatchError> {
    let res = installation.resources_dir.to_string_lossy().to_string();
    logger::log(&format!("takeown on: {}", res));
    grant_dir(&res);

    let test_file = installation.resources_dir.join(".claude_cn_test");
    logger::log(&format!("write test: {}", test_file.display()));
    match fs::write(&test_file, "test") {
        Ok(()) => {
            let _ = fs::remove_file(&test_file);
            logger::log("write test: OK");
            Ok(())
        }
        Err(e) => {
            logger::log(&format!("write test FAILED: {}", e));
            Err(PatchError::Io(format!(
                "无法写入 Claude 目录，请确认以管理员身份运行: {}",
                e
            )))
        }
    }
}

fn kill_claude() {
    let our_pid = std::process::id();
    logger::log(&format!("kill_claude: our PID={}", our_pid));

    let output = hidden_cmd("tasklist")
        .args(["/FI", "IMAGENAME eq Claude.exe", "/FO", "CSV", "/NH"])
        .output();

    if let Ok(output) = output {
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            let parts: Vec<&str> = line.split(',').collect();
            if parts.len() >= 2 {
                let pid = parts[1].trim_matches('"').trim();
                if let Ok(pid_num) = pid.parse::<u32>() {
                    if pid_num == our_pid {
                        continue;
                    }
                    logger::log(&format!("killing Claude.exe PID {}", pid_num));
                    let _ = hidden_cmd("taskkill")
                        .args(["/F", "/PID", &pid_num.to_string()])
                        .stdout(std::process::Stdio::null())
                        .stderr(std::process::Stdio::null())
                        .status();
                }
            }
        }
    }

    std::thread::sleep(std::time::Duration::from_secs(2));
    logger::log("kill_claude done");
}

fn start_claude() {
    if let Some(exe) = find_claude_exe_path() {
        logger::log(&format!("start_claude: launching {}", exe.display()));
        let _ = std::process::Command::new(&exe)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
        return;
    }

    if let Some(family) = get_claude_package_family() {
        let uri = format!(r"shell:AppsFolder\{}!Claude", family);
        logger::log(&format!("start_claude: launching MSIX via {}", uri));
        let _ = hidden_cmd("cmd")
            .args(["/C", "start", "", &uri])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
        return;
    }

    logger::log("start_claude: could not find Claude to launch");
}

fn find_claude_exe_path() -> Option<std::path::PathBuf> {
    let local = std::env::var("LOCALAPPDATA").ok()?;
    let exe = std::path::PathBuf::from(&local)
        .join("AnthropicClaude")
        .join("Claude.exe");
    if exe.exists() {
        return Some(exe);
    }
    let exe2 = std::path::PathBuf::from(&local)
        .join("Programs")
        .join("claude-desktop")
        .join("Claude.exe");
    if exe2.exists() {
        return Some(exe2);
    }
    None
}

fn get_claude_package_family() -> Option<String> {
    let output = hidden_cmd("powershell")
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
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

fn write_with_retry(path: &Path, content: &str) -> Result<(), PatchError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| PatchError::Io(format!("{}: {}", path.display(), e)))?;
    }

    match fs::write(path, content) {
        Ok(()) => Ok(()),
        Err(_) => {
            if path.exists() {
                if let Ok(meta) = fs::metadata(path) {
                    let mut perms = meta.permissions();
                    #[allow(clippy::permissions_set_readonly_false)]
                    perms.set_readonly(false);
                    let _ = fs::set_permissions(path, perms);
                }
            }
            fs::write(path, content)
                .map_err(|e| PatchError::Io(format!("{}: {}", path.display(), e)))
        }
    }
}

fn merge_json(base: &Value, overlay: &Value) -> Value {
    match (base, overlay) {
        (Value::Object(base_obj), Value::Object(overlay_obj)) => {
            let mut result = base_obj.clone();
            for (key, value) in overlay_obj {
                match result.get(key) {
                    Some(base_val) => {
                        result.insert(key.clone(), merge_json(base_val, value));
                    }
                    None => {
                        result.insert(key.clone(), value.clone());
                    }
                }
            }
            Value::Object(result)
        }
        (_, overlay_val) => overlay_val.clone(),
    }
}

fn write_translation_files(installation: &ClaudeInstallation) -> Result<(), PatchError> {
    let desktop_path = installation.resources_dir.join("zh-CN.json");
    write_with_retry(&desktop_path, DESKTOP_ZH_CN)?;

    let frontend_path = installation.ion_dist_dir.join("i18n").join("zh-CN.json");
    let en_us_path = installation.ion_dist_dir.join("i18n").join("en-US.json");

    if en_us_path.exists() {
        let en_us_content = fs::read_to_string(&en_us_path)
            .map_err(|e| PatchError::Io(format!("读取 en-US.json 失败: {}", e)))?;
        let en_us: Value =
            serde_json::from_str(&en_us_content).unwrap_or(Value::Object(Default::default()));
        let zh_cn: Value =
            serde_json::from_str(FRONTEND_ZH_CN).unwrap_or(Value::Object(Default::default()));
        let merged = merge_json(&en_us, &zh_cn);
        let merged_str = serde_json::to_string(&merged)
            .map_err(|e| PatchError::Io(format!("序列化合并结果失败: {}", e)))?;
        write_with_retry(&frontend_path, &merged_str)?;
    } else {
        write_with_retry(&frontend_path, FRONTEND_ZH_CN)?;
    }

    let statsig_path = installation
        .ion_dist_dir
        .join("i18n")
        .join("statsig")
        .join("zh-CN.json");
    write_with_retry(&statsig_path, STATSIG_ZH_CN)?;

    Ok(())
}

fn patch_language_whitelist(installation: &ClaudeInstallation) -> Result<(), PatchError> {
    let assets_dir = installation.ion_dist_dir.join("assets").join("v1");
    if !assets_dir.exists() {
        return Err(PatchError::Whitelist("assets/v1 目录不存在".into()));
    }

    let re = Regex::new(r#"\["en-US"(?:,"[a-zA-Z]{2,3}(?:-[a-zA-Z0-9]{2,4})*")+\]"#)
        .map_err(|e| PatchError::Whitelist(e.to_string()))?;

    let mut patched = false;

    for entry in fs::read_dir(&assets_dir)
        .map_err(|e| PatchError::Whitelist(e.to_string()))?
        .flatten()
    {
        let path = entry.path();
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        if !name.starts_with("index-") || !name.ends_with(".js") {
            continue;
        }

        let content = fs::read_to_string(&path)
            .map_err(|e| PatchError::Whitelist(format!("读取 {} 失败: {}", name, e)))?;

        if content.contains("\"zh-CN\"") {
            patched = true;
            continue;
        }

        if let Some(m) = re.find(&content) {
            let original = m.as_str();
            let injected = format!("{}{}]", &original[..original.len() - 1], ",\"zh-CN\"");
            let new_content = content.replacen(original, &injected, 1);
            write_with_retry(&path, &new_content)?;
            patched = true;
        }
    }

    if !patched {
        return Err(PatchError::Whitelist(
            "未找到 index-*.js 或语言列表".into(),
        ));
    }

    Ok(())
}

fn set_config_locale() -> Result<(), PatchError> {
    let path = config_path().ok_or_else(|| PatchError::Config("无法确定配置文件路径".into()))?;

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| PatchError::Config(e.to_string()))?;
    }

    let mut config: Value = if path.exists() {
        let s = fs::read_to_string(&path).map_err(|e| PatchError::Config(e.to_string()))?;
        serde_json::from_str(&s).unwrap_or(serde_json::json!({}))
    } else {
        serde_json::json!({})
    };

    config
        .as_object_mut()
        .ok_or_else(|| PatchError::Config("配置文件格式错误".into()))?
        .insert("locale".into(), serde_json::json!("zh-CN"));

    let out =
        serde_json::to_string_pretty(&config).map_err(|e| PatchError::Config(e.to_string()))?;
    fs::write(&path, out).map_err(|e| PatchError::Config(e.to_string()))?;

    Ok(())
}

fn remove_config_locale() -> Result<(), PatchError> {
    let path = config_path().ok_or_else(|| PatchError::Config("无法确定配置文件路径".into()))?;

    if !path.exists() {
        return Ok(());
    }

    let s = fs::read_to_string(&path).map_err(|e| PatchError::Config(e.to_string()))?;
    let mut config: Value = serde_json::from_str(&s).unwrap_or(serde_json::json!({}));

    if let Some(obj) = config.as_object_mut() {
        obj.remove("locale");
    }

    let out =
        serde_json::to_string_pretty(&config).map_err(|e| PatchError::Config(e.to_string()))?;
    fs::write(&path, out).map_err(|e| PatchError::Config(e.to_string()))?;

    Ok(())
}
