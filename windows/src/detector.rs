use std::fs;
use std::path::PathBuf;

#[derive(Clone)]
pub struct ClaudeInstallation {
    pub app_dir: PathBuf,
    pub resources_dir: PathBuf,
    pub ion_dist_dir: PathBuf,
    pub version: String,
    pub package_name: String,
    pub is_msix: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PatchStatus {
    NotInstalled,
    Unpatched,
    Patched,
}

pub fn find_claude() -> Option<ClaudeInstallation> {
    if let Some(inst) = find_claude_msix() {
        return Some(inst);
    }
    if let Some(inst) = find_claude_exe() {
        return Some(inst);
    }
    crate::logger::log("find_claude: no valid Claude installation found");
    None
}

fn find_claude_msix() -> Option<ClaudeInstallation> {
    let base = PathBuf::from(r"C:\Program Files\WindowsApps");

    if !base.exists() {
        crate::logger::log("find_msix: WindowsApps directory does not exist");
        return None;
    }

    let read_dir = match fs::read_dir(&base) {
        Ok(rd) => rd,
        Err(e) => {
            crate::logger::log(&format!("find_msix: cannot read WindowsApps: {}", e));
            return None;
        }
    };

    let mut candidates: Vec<_> = read_dir
        .filter_map(|e| e.ok())
        .filter(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            name.starts_with("Claude_")
                && (name.contains("_x64__") || name.contains("_arm64__") || name.contains("_x86__"))
        })
        .collect();

    crate::logger::log(&format!("find_msix: found {} candidates", candidates.len()));

    candidates.sort_by(|a, b| b.file_name().cmp(&a.file_name()));

    for entry in candidates {
        let app_dir = entry.path().join("app");
        let resources_dir = app_dir.join("resources");
        let en_us = resources_dir.join("en-US.json");

        crate::logger::log(&format!("find_msix: checking {}", entry.path().display()));

        if en_us.exists() {
            let ion_dist_dir = resources_dir.join("ion-dist");
            let package_name = entry.file_name().to_string_lossy().to_string();
            let version = package_name
                .split('_')
                .nth(1)
                .unwrap_or("unknown")
                .to_string();

            crate::logger::log(&format!("find_msix: detected v{}", version));

            return Some(ClaudeInstallation {
                app_dir,
                resources_dir,
                ion_dist_dir,
                version,
                package_name,
                is_msix: true,
            });
        }
    }

    None
}

fn find_claude_exe() -> Option<ClaudeInstallation> {
    let local_app_data = std::env::var("LOCALAPPDATA").ok()?;
    let local = PathBuf::from(&local_app_data);

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

    // Query the registry for custom install locations
    for reg_path in find_claude_from_registry() {
        if !search_bases.contains(&reg_path) {
            crate::logger::log(&format!("find_exe: registry found {}", reg_path.display()));
            search_bases.insert(0, reg_path);
        }
    }

    for base in &search_bases {
        if !base.exists() {
            continue;
        }
        crate::logger::log(&format!("find_exe: scanning {}", base.display()));

        if let Some(inst) = try_detect_in_dir(base) {
            return Some(inst);
        }
    }

    crate::logger::log("find_exe: no exe installation found");
    None
}

fn try_detect_in_dir(base: &PathBuf) -> Option<ClaudeInstallation> {
    // Check versioned subdirectories (app-X.X.X)
    if let Ok(read_dir) = fs::read_dir(base) {
        let mut candidates: Vec<_> = read_dir
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().starts_with("app-"))
            .collect();

        if !candidates.is_empty() {
            candidates.sort_by(|a, b| b.file_name().cmp(&a.file_name()));

            for entry in candidates {
                let app_dir = entry.path();
                let resources_dir = app_dir.join("resources");

                if resources_dir.join("en-US.json").exists() {
                    let dir_name = entry.file_name().to_string_lossy().to_string();
                    let version = dir_name
                        .strip_prefix("app-")
                        .unwrap_or("unknown")
                        .to_string();

                    crate::logger::log(&format!(
                        "find_exe: detected v{} at {}",
                        version,
                        app_dir.display()
                    ));

                    return Some(ClaudeInstallation {
                        app_dir,
                        resources_dir: resources_dir.clone(),
                        ion_dist_dir: resources_dir.join("ion-dist"),
                        version,
                        package_name: dir_name,
                        is_msix: false,
                    });
                }
            }
        }
    }

    // Check direct resources directory (no versioned subdir)
    let resources_dir = base.join("resources");
    if resources_dir.join("en-US.json").exists() {
        let version = read_version_from_dir(base);
        let dir_name = base
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        crate::logger::log(&format!(
            "find_exe: detected v{} (direct) at {}",
            version,
            base.display()
        ));

        return Some(ClaudeInstallation {
            app_dir: base.clone(),
            resources_dir: resources_dir.clone(),
            ion_dist_dir: resources_dir.join("ion-dist"),
            version,
            package_name: dir_name,
            is_msix: false,
        });
    }

    None
}

fn read_version_from_dir(dir: &PathBuf) -> String {
    let pkg = dir.join("package.json");
    if let Ok(content) = fs::read_to_string(&pkg) {
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) {
            if let Some(v) = val.get("version").and_then(|v| v.as_str()) {
                return v.to_string();
            }
        }
    }
    let exe = dir.join("Claude.exe");
    if exe.exists() {
        return "installed".to_string();
    }
    "unknown".to_string()
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
        let output = reg_query_silent(root, field);
        let Ok(output) = output else { continue };
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

            let key_is_claude = current_key.contains("claude")
                || current_key.contains("anthropic");

            let Some((_f, value)) = trimmed.split_once("REG_SZ") else {
                continue;
            };
            let value = value.trim();

            let value_is_claude = {
                let lower = value.to_lowercase();
                lower.contains("claude") || lower.contains("anthropic")
            };

            if !key_is_claude && !value_is_claude {
                continue;
            }

            let path = if field == "DisplayIcon" {
                let exe_path = PathBuf::from(value.trim_matches(','));
                match exe_path.parent() {
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

fn reg_query_silent(root: &str, field: &str) -> Result<std::process::Output, std::io::Error> {
    let mut cmd = std::process::Command::new("reg");
    cmd.args(["query", root, "/s", "/v", field])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null());
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
    cmd.output()
}

pub fn check_patch_status(installation: &ClaudeInstallation) -> PatchStatus {
    let i18n_zh = installation.ion_dist_dir.join("i18n").join("zh-CN.json");
    let i18n_exists = i18n_zh.exists();
    crate::logger::log(&format!("patch_status: i18n/zh-CN.json exists={}", i18n_exists));
    if !i18n_exists {
        crate::logger::log("patch_status: -> Unpatched (no i18n file)");
        return PatchStatus::Unpatched;
    }

    let wl = is_whitelist_patched(installation);
    crate::logger::log(&format!("patch_status: whitelist_patched={}", wl));
    if !wl {
        crate::logger::log("patch_status: -> Unpatched (whitelist not patched)");
        return PatchStatus::Unpatched;
    }

    let locale = has_locale_config();
    crate::logger::log(&format!("patch_status: has_locale_config={}", locale));
    if locale {
        crate::logger::log("patch_status: -> Patched");
        return PatchStatus::Patched;
    }

    crate::logger::log("patch_status: -> Unpatched (no locale config)");
    PatchStatus::Unpatched
}

fn is_whitelist_patched(installation: &ClaudeInstallation) -> bool {
    let assets_dir = installation.ion_dist_dir.join("assets").join("v1");
    let Ok(entries) = fs::read_dir(&assets_dir) else {
        return false;
    };
    let mut found_any = false;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with("index-") && name.ends_with(".js") {
            found_any = true;
            if let Ok(content) = fs::read_to_string(entry.path()) {
                if !content.contains("\"zh-CN\"") {
                    return false;
                }
            } else {
                return false;
            }
        }
    }
    found_any
}

fn has_locale_config() -> bool {
    let Ok(appdata) = std::env::var("APPDATA") else {
        return false;
    };
    let base = PathBuf::from(&appdata);
    for dir_name in ["Claude", "Claude-3p"] {
        let path = base.join(dir_name).join("config.json");
        if let Ok(content) = fs::read_to_string(&path) {
            if content.contains("\"zh-CN\"") {
                return true;
            }
        }
    }
    false
}
