# Windows 汉化（规划中）

目前本 skill 的可执行实现是 macOS。Windows 原理与 macOS 完全相同，差别在系统层面，移植时照搬 `scripts/patch_macos.py` 的结构、改下面几点即可（逻辑可参考 ClaudeCN 项目 `windows/src/patcher.rs` 的成熟实现）：

| 方面 | macOS | Windows |
|---|---|---|
| Claude 安装位置 | `/Applications/Claude.app` | `%LOCALAPPDATA%\AnthropicClaude\app-*\`、`Programs\Claude` 等多处 + 注册表兜底；也支持 MSIX 包 |
| 前端资源根 | `Contents/Resources/ion-dist/` | 安装目录下 `resources\` 内的 `ion-dist\`（结构一致：`i18n\`、`assets\v1\index-*.js`） |
| 提权 | osascript "with administrator privileges" 弹框 | 需**管理员身份运行**；用 `takeown` + `icacls` 拿目录写权限 |
| 重签名 | **必须**（改文件即失效，需 hardened runtime + 证书） | **不需要**（Windows 不强制 app 签名校验，直接改文件即可） |
| locale 配置 | `~/Library/Application Support/Claude/config.json` | `%APPDATA%\Claude\config.json`（部分版本还有 `Claude-3p`，两个都写） |
| 关闭/重启 | `osascript ... quit` / `open` | `taskkill /F /IM Claude.exe` / 启动 exe 或 MSIX `shell:AppsFolder\...` |

i18n 合并、白名单自适应探测、en-US 兜底、译文增量等**核心逻辑完全一致**，可直接复用 `patch_macos.py` 里 `build_merged` / `patch_whitelist` 的思路。

实现 Windows 版时建议新建 `scripts/patch_windows.py`，并在 SKILL.md 的"平台"一节加上调用说明与 OS 判断。
