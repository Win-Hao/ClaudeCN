# Claude Desktop i18n 机制 & 自适应排查

这份文档记录 Claude Desktop 的本地化内部机制，以及当 `detect`/`apply` 出问题时怎么自适应排查。
**结构变了不要硬套步骤**——先用这里的方法现场判断。

## 目录

- [机制总览](#机制总览)
- [关键路径](#关键路径)
- [版本演化（已知）](#版本演化已知)
- [排查清单](#排查清单)

## 机制总览

界面文案分两层：

1. **前端 UI（Electron 渲染层，占绝大多数）**
   - 文案在 `Contents/Resources/ion-dist/i18n/{locale}.json`。
   - key 是**英文原文的内容哈希**（如 `+0AXIvgEHO`），value 是该语言的译文。
   - 同一英文原文在任何版本里哈希相同 → **旧译文跨版本仍有效**（前提是英文原文没变）。这是"内置精校 + 只翻新增"策略成立的根基。
   - 运行时按当前 locale 动态 `fetch('/i18n/{locale}.json')`，未命中的 key 回退英文。所以合并时一定要 **en-US 为底**，保证未译 key 显示英文而不是显示哈希。

2. **桌面/原生层（菜单栏、托盘、原生弹窗）**
   - `Contents/Resources/{locale}.json`（同样是哈希 key）。
   - `Contents/Resources/{locale}.lproj/Localizable.strings`（经典 `"English"="中文";` 格式，按英文原文做 key）。

**locale 配置**：`~/Library/Application Support/Claude/config.json` 里的 `locale` 字段。设为 `zh-CN` 即触发加载 `zh-CN.json`。

**签名**：改了 app 包内任何文件，macOS 代码签名即失效，必须重签名。**关键坑（实测 Darwin 25 / macOS 26）**：重签名用 **ad-hoc + hardened runtime**，并且必须**剥掉绑定 Team ID/Apple 授权的 entitlements**——`keychain-access-groups`、`com.apple.application-identifier`、所有 `com.apple.developer.*`。这些需 Apple provisioning 授权，ad-hoc 重签却保留它们会让 AMFI 在启动时拒绝 spawn（`open` 报 error 163 "Launchd job spawn failed"）。而 `com.apple.security.device.*`（摄像头/麦克风等）、`personal-information.*`、`virtualization` 在 ad-hoc 下**可以保留**、不影响启动，应原样留着以免砍掉语音/截屏等能力。脚本 `resign()` 已封装"提取原 entitlements → 过滤 → ad-hoc 签 + `disable-library-validation`"。无需自签名证书（旧 GUI 版用证书是把问题复杂化了）。

## 关键路径

| 用途 | 相对 Claude.app 的路径 |
|---|---|
| 前端 i18n 目录 | `Contents/Resources/ion-dist/i18n/` |
| 前端入口 JS | `Contents/Resources/ion-dist/assets/v1/index-*.js` |
| 动态 i18n（新版） | `Contents/Resources/ion-dist/i18n/dynamic/{locale}.json` |
| overrides（新版） | `Contents/Resources/ion-dist/i18n/{locale}.overrides.json` |
| 桌面菜单文案 | `Contents/Resources/{locale}.json` |
| 原生 strings | `Contents/Resources/{locale}.lproj/Localizable.strings` |
| 主进程代码 | `Contents/Resources/app.asar` |
| locale 配置 | `~/Library/Application Support/Claude/config.json`；**3p/自定义模型模式用 `Claude-3p/config.json`，两个都要写** |

`patch_macos.py` 的 `find_i18n_dir` / `find_assets_dir` 会先试标准路径，找不到就在 `Contents/Resources` 下按"含 en-US.json 的目录""含 index-*.js 的目录"自适应搜索。

## 版本演化（已知）

- **~v1.6608.0 及更早**：前端 `index-*.js` 里有**硬编码语言白名单数组** `["en-US","de-DE",...]`，必须往里注入 `"zh-CN"` 该语言才会被加载/可选。这是当年最易随版本失效的一步。
- **v1.15200.0（2026-06 实测）**：白名单数组**已移除**，改为按 locale 动态 fetch（`/i18n/{e}.json` + `/i18n/dynamic/{e}.json` + `/i18n/{e}.overrides.json`）。`index.js` 中 `"zh-CN"` 字面量为 0，`detect` 的 `needs_js_patch` 为 `false`——**无需改 JS**。新增了 `i18n/dynamic/` 子目录，i18n 不再 zstd 压缩（明文 .json）。locale 归一化映射里有 `"zh-cn":"zh"`，但前端 i18n 实际用的是 `localeOverride ?? fallback` 即原始 locale，所以文件名仍用 `zh-CN.json`、config 仍设 `zh-CN`。

脚本对两代机制都兼容：`patch_whitelist` 命中数组就注入、没有就 `skipped`（不报错）。

## 排查清单

**`detect` 里 `i18n_dir` 或 `assets_dir` 为 null：**
结构变了。手动在 app 里找：`find /Applications/Claude.app/Contents/Resources -name en-US.json` 和 `-name 'index-*.js'`。确认前端 i18n 是否搬家、文件名是否还叫 `en-US.json`。必要时改脚本里的探测逻辑或 `LOCALE` 常量。

**汉化后界面仍是英文：**
1. **最常见**：locale 写错了**数据目录**。Claude 用 `~/Library/Application Support/Claude/`，但接入第三方/自定义模型时用 `Claude-3p/`——活动目录是 `config.json` 里 `updaterLastSeenVersion` 与当前版本一致、或 `~/Library/Logs/Claude*/main.log` 正在写的那个。脚本已对所有 `Claude`/`Claude-*` 目录写 locale；手查时两个都看，确认活动目录的 `locale` 确实是 `zh-CN`。
2. 确认 `i18n/zh-CN.json` 写进去了且是合法 JSON。
3. 读 `index-*.js` 看 i18n 加载那段（搜 `i18n_public` 或 ``fetch(`/i18n/``），确认它用的 locale 变量到底取自哪、有没有把 `zh-CN` 归一成别的（如 `zh`）。如果归一成 `zh`，把 `LOCALE` 改成 `zh` 并相应命名文件。
4. 若用户登录了账号、服务端语言覆盖了本地：让用户在 Claude 内开**开发者模式**（Settings → Developer）。
5. 某些版本可能又引入了"可选语言列表"门槛：在 `index.js` 里搜语言相关数组，必要时恢复白名单注入。

**汉化后 Claude 无法启动（`open` 报 error 163 / "Launchd job spawn failed"）：**
AMFI 在启动时拒绝了重签名后的 app，几乎都是 entitlements 没剥干净。确认 `resign()` 剥掉了 `keychain-access-groups` 与所有 `com.apple.developer.*`：`codesign -d --entitlements - --xml /Applications/Claude.app/Contents/MacOS/Claude` 看是否残留。两个易误导的点：① `codesign --verify` 显示 "valid / satisfies its Designated Requirement" 只代表签名结构没坏，**不代表能启动**，启动与否由 AMFI 决定；② `spctl -a` 显示 rejected 是正常的（ad-hoc/自签本就过不了 Gatekeeper 评估），只要 app 没有 `com.apple.quarantine` 属性就不影响本地启动。必要时 `restore` 后重来。

**桌面菜单没汉化：**
桌面层由 `app.asar` 主进程加载，机制可能与前端不同。确认 `Contents/Resources/zh-CN.json` 和 `zh-CN.lproj/Localizable.strings` 在位；若仍不生效，需解包 `app.asar` 看主进程怎么读 locale 文件（这是已知的次要待完善点）。
