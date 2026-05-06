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
    let base = PathBuf::from(r"C:\Program Files\WindowsApps");

    if !base.exists() {
        return None;
    }

    let mut candidates: Vec<_> = fs::read_dir(&base)
        .ok()?
        .filter_map(|e| e.ok())
        .filter(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            name.starts_with("Claude_") && name.contains("_x64__")
        })
        .collect();

    candidates.sort_by(|a, b| b.file_name().cmp(&a.file_name()));

    for entry in candidates {
        let app_dir = entry.path().join("app");
        let resources_dir = app_dir.join("resources");
        let en_us = resources_dir.join("en-US.json");

        if en_us.exists() {
            let ion_dist_dir = resources_dir.join("ion-dist");
            let package_name = entry.file_name().to_string_lossy().to_string();
            let version = package_name
                .split('_')
                .nth(1)
                .unwrap_or("unknown")
                .to_string();

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
