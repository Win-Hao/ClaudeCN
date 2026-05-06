use std::fs;
use std::path::PathBuf;

#[derive(Clone)]
pub struct ClaudeInstallation {
    pub app_dir: PathBuf,
    pub resources_dir: PathBuf,
    pub ion_dist_dir: PathBuf,
    pub version: String,
    pub package_name: String,
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
            name.starts_with("Claude_") && name.contains("_x64__")
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
            });
        }
    }

    None
}

fn find_claude_exe() -> Option<ClaudeInstallation> {
    let local_app_data = std::env::var("LOCALAPPDATA").ok()?;
    let base = PathBuf::from(&local_app_data).join("AnthropicClaude");

    if !base.exists() {
        crate::logger::log(&format!("find_exe: {} does not exist", base.display()));
        return None;
    }

    let read_dir = match fs::read_dir(&base) {
        Ok(rd) => rd,
        Err(e) => {
            crate::logger::log(&format!("find_exe: cannot read {}: {}", base.display(), e));
            return None;
        }
    };

    let mut candidates: Vec<_> = read_dir
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .starts_with("app-")
        })
        .collect();

    crate::logger::log(&format!("find_exe: found {} app- candidates", candidates.len()));

    candidates.sort_by(|a, b| b.file_name().cmp(&a.file_name()));

    for entry in candidates {
        let app_dir = entry.path();
        let resources_dir = app_dir.join("resources");
        let en_us = resources_dir.join("en-US.json");

        crate::logger::log(&format!("find_exe: checking {}", app_dir.display()));

        if en_us.exists() {
            let ion_dist_dir = resources_dir.join("ion-dist");
            let dir_name = entry.file_name().to_string_lossy().to_string();
            let version = dir_name
                .strip_prefix("app-")
                .unwrap_or("unknown")
                .to_string();

            crate::logger::log(&format!("find_exe: detected v{}", version));

            return Some(ClaudeInstallation {
                app_dir,
                resources_dir,
                ion_dist_dir,
                version,
                package_name: dir_name,
            });
        }
    }

    None
}

pub fn check_patch_status(installation: &ClaudeInstallation) -> PatchStatus {
    let i18n_zh = installation.ion_dist_dir.join("i18n").join("zh-CN.json");

    if !i18n_zh.exists() {
        return PatchStatus::Unpatched;
    }

    if let Some(config_path) = config_path() {
        if let Ok(content) = fs::read_to_string(&config_path) {
            if content.contains("\"zh-CN\"") {
                return PatchStatus::Patched;
            }
        }
    }

    PatchStatus::Unpatched
}

pub fn config_path() -> Option<PathBuf> {
    let appdata = std::env::var("APPDATA").ok()?;
    let base = PathBuf::from(&appdata);

    let claude_3p = base.join("Claude-3p").join("config.json");
    if claude_3p.exists() {
        return Some(claude_3p);
    }

    let claude = base.join("Claude").join("config.json");
    if claude.exists() {
        return Some(claude);
    }

    Some(claude_3p)
}
