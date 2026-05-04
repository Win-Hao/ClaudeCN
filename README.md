# ClaudeCN - Claude Desktop 汉化助手

一键将 Claude Desktop 切换为中文界面的 macOS 菜单栏工具。

![ClaudeCN Screenshot](screenshots/menubar.png)

## 功能

- 一键汉化 Claude Desktop 界面（前端、桌面端、原生菜单）
- 一键恢复原版（完整备份，安全无损）
- 自动检测 Claude Desktop 安装状态和汉化状态
- 状态栏实时显示处理进度（旋转动画 + 进度文字）
- 系统通知反馈操作结果
- 菜单栏常驻，不占用 Dock 栏

## 安装

### 方式一：DMG 安装（推荐）

1. 从 [Releases](../../releases) 页面下载最新的 `.dmg` 文件
2. 打开 DMG，将 ClaudeCN 拖入 Applications 文件夹
3. 首次打开前，在终端执行以下命令解除 macOS 安全限制（仅需一次）：
   ```bash
   xattr -cr /Applications/ClaudeCN.app
   ```
4. 从启动台或 Applications 打开 ClaudeCN

### 方式二：源码构建

需要 [XcodeGen](https://github.com/yonaskolb/XcodeGen) 和 Xcode 16+。

```bash
git clone https://github.com/Win-Hao/ClaudeCN.git
cd ClaudeCN
xcodegen generate
open ClaudeCN.xcodeproj
```

在 Xcode 中选择 Release 配置，Build 即可。

## 使用方法

1. 确保已安装 [Claude Desktop](https://claude.ai/download)
2. 打开 ClaudeCN，菜单栏会出现地球图标
3. 点击图标打开面板，点击「一键汉化」按钮
4. 输入系统密码授权（需要修改 Claude.app 文件）
5. 处理过程中状态栏会显示旋转动画和进度提示，完成后通过系统通知告知结果
6. Claude Desktop 会自动重启为中文界面

如需恢复，点击「恢复原版」即可还原。

## 工作原理

ClaudeCN 通过以下方式实现汉化：

1. 备份原版 Claude.app（首次汉化时创建 zip 备份）
2. 将中文翻译文件注入 Claude.app 的 i18n 目录
3. 在语言白名单中添加 `zh-CN`
4. 设置 Claude Desktop 的语言偏好为中文
5. 重新签名应用，保留原有权限

## 安全性

- 首次汉化时自动备份原版 Claude.app（zip 格式），Claude 更新后自动重新备份
- 恢复功能从备份还原，确保与原版完全一致
- 使用原子替换操作（mv），失败时自动回滚
- 不修改任何 Claude 的核心功能代码
- 不收集任何用户数据

## 系统要求

- macOS 13.0 (Ventura) 或更高版本
- Claude Desktop 已安装

## 声明

本项目为社区开源工具，与 Anthropic 公司无关，非官方产品。

## 许可证

[MIT License](LICENSE)
