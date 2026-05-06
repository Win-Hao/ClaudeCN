use std::fs;
use std::path::PathBuf;

use crate::detector::ClaudeInstallation;

#[derive(Debug)]
pub struct BackupError(pub String);

impl std::fmt::Display for BackupError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

fn backup_dir() -> Result<PathBuf, BackupError> {
    let local = std::env::var("LOCALAPPDATA")
        .map_err(|_| BackupError("无法获取 LOCALAPPDATA".to_string()))?;
    let dir = PathBuf::from(local).join("ClaudeCN").join("backups");
    fs::create_dir_all(&dir).map_err(|e| BackupError(format!("创建备份目录失败: {}", e)))?;
    Ok(dir)
}

pub fn create_backup(installation: &ClaudeInstallation) -> Result<(), BackupError> {
    let backup = backup_dir()?;
    let version_file = backup.join("backup-version.txt");

    let need_backup = if version_file.exists() {
        let saved = fs::read_to_string(&version_file).unwrap_or_default();
        saved.trim() != installation.version
    } else {
        true
    };

    if !need_backup {
        let backup_assets = backup.join("assets").join("v1");
        let has_files = backup_assets.exists()
            && fs::read_dir(&backup_assets)
                .map(|mut d| d.next().is_some())
                .unwrap_or(false);
        if has_files {
            return Ok(());
        }
    }

    let assets_dir = installation.ion_dist_dir.join("assets").join("v1");
    if assets_dir.exists() {
        let backup_assets = backup.join("assets").join("v1");
        fs::create_dir_all(&backup_assets).map_err(|e| BackupError(e.to_string()))?;

        for entry in fs::read_dir(&assets_dir)
            .map_err(|e| BackupError(e.to_string()))?
            .flatten()
        {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with("index-") && name.ends_with(".js") {
                let content = fs::read_to_string(entry.path()).unwrap_or_default();
                if content.contains("\"zh-CN\"") {
                    continue;
                }
                fs::copy(entry.path(), backup_assets.join(&name))
                    .map_err(|e| BackupError(format!("备份 {} 失败: {}", name, e)))?;
            }
        }
    }

    fs::write(&version_file, &installation.version)
        .map_err(|e| BackupError(e.to_string()))?;

    Ok(())
}

pub fn restore_backup(installation: &ClaudeInstallation) -> Result<(), BackupError> {
    let backup = backup_dir()?;
    let backup_assets = backup.join("assets").join("v1");

    if backup_assets.exists() {
        let assets_dir = installation.ion_dist_dir.join("assets").join("v1");
        for entry in fs::read_dir(&backup_assets)
            .map_err(|e| BackupError(e.to_string()))?
            .flatten()
        {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with("index-") && name.ends_with(".js") {
                let dest = assets_dir.join(&name);
                clear_readonly(&dest);
                fs::copy(entry.path(), &dest)
                    .map_err(|e| BackupError(format!("恢复 {} 失败: {}", name, e)))?;
            }
        }
    }

    Ok(())
}

fn clear_readonly(path: &PathBuf) {
    if path.exists() {
        if let Ok(meta) = fs::metadata(path) {
            let mut perms = meta.permissions();
            #[allow(clippy::permissions_set_readonly_false)]
            perms.set_readonly(false);
            let _ = fs::set_permissions(path, perms);
        }
    }
}
