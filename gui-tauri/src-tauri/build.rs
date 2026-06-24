// Windows 上 Claude 装在受保护目录（WindowsApps / Program Files），改文件需管理员权限。
// 给可执行嵌入 requireAdministrator 清单，让 app 启动即走 UAC 提权（与旧 GUI 行为一致）。
// 用 CARGO_CFG_TARGET_OS（目标 OS）而非 cfg(windows)（宿主 OS），这样从 mac 交叉编 Windows 也生效。
const WINDOWS_ADMIN_MANIFEST: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <trustInfo xmlns="urn:schemas-microsoft-com:asm.v3">
    <security>
      <requestedPrivileges>
        <requestedExecutionLevel level="requireAdministrator" uiAccess="false" />
      </requestedPrivileges>
    </security>
  </trustInfo>
</assembly>
"#;

fn main() {
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os == "windows" {
        let attrs = tauri_build::Attributes::new().windows_attributes(
            tauri_build::WindowsAttributes::new().app_manifest(WINDOWS_ADMIN_MANIFEST),
        );
        tauri_build::try_build(attrs).expect("failed to run tauri-build");
    } else {
        tauri_build::build();
    }
}
