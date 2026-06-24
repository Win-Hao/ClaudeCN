# ClaudeCN GUI 重做执行文档（Tauri v2 · 对齐 untype 技术栈）

> 本文件是给**新 session 执行者**的自包含施工说明。读完即可从零把 ClaudeCN 的图形界面用 Tauri 重做、同时出 macOS 和 Windows 安装包。
>
> 作者背景：ClaudeCN 是 Claude Desktop 桌面端中文汉化工具，原有 macOS(Swift) + Windows(Rust) 两套**分离**的 GUI，难维护、难统一发布。现要用 Tauri v2 重做成**一套代码**。

---

## 0. 执行者快速上手（先读这段）

- **你要做的事**：在 `ClaudeCN/gui-tauri/`（新建）里做一个 Tauri v2 桌面应用，UI 提供「一键汉化 / 一键恢复 / 状态显示 / 进度」，后端用 Rust 实现汉化逻辑，能在 macOS 出 `.dmg`、在 Windows 出 `.exe/.msi`。
- **三个真相源（务必先读）**：
  1. **技术栈样板** = `/Users/huangyonghao/vibe-coding/untype/`（Tauri v2 + React 项目，照它的栈和惯例搭壳）。
  2. **汉化逻辑真相源** = `ClaudeCN/skills/claude-localize/`，尤其：
     - `scripts/patch_macos.py` —— macOS 汉化/恢复的**完整正确逻辑**，逐函数移植成 Rust。
     - `references/mechanism.md` —— Claude i18n 机制 + 所有踩坑结论（**先通读**）。
     - `assets/zh-CN.base.json`（18,686 条精校译文，直接打包进 GUI）、`desktop-zh-CN.base.json`、`statsig-zh-CN.base.json`、`claude-Localizable.strings`。
  3. **Windows 旧逻辑可复用** = `ClaudeCN/windows/src/`（`patcher.rs` / `detector.rs` / `backup.rs`），大部分能直接搬进新后端，但**要打上和 skill 相同的修正**（见 §4）。
- **怎么算成功**：mac 上点「一键汉化」→ Claude 重启显示中文且能正常启动；点「一键恢复」→ 回到英文原版；Windows 同等。CI 矩阵能产出两端安装包。

> ✅ 关于"交叉编译"：Tauri v2 **可以**在 Mac 上交叉编出 Windows 安装包（官方支持）——`cargo-xwin`(自动下载 MSVC SDK) + NSIS，一条命令出 `-setup.exe`。两个要点：① 用 **MSVC** 目标（因 Tauri 依赖 WebView2），**不是**你现在编纯 Rust GUI 用的 mingw/GNU 目标；② 只能交叉出 NSIS `-setup.exe`，MSI 不能（WiX 仅 Windows）。官方称交叉编"测试较少、宜作后备"，故**正式发布更稳走 CI 矩阵**（windows-latest 原生编）。§7 两条路都给，命令齐全。

---

## 1. 技术栈（照搬 untype，删掉无关部分）

| 层 | 用什么 | 来源/备注 |
|---|---|---|
| 后端 | **Rust + Tauri v2**（`tauri = "2"`，feature: `tray-icon`） | 同 untype；命令用 `#[tauri::command]`，在 `lib.rs` 的 `tauri::generate_handler!` 注册 |
| 前端 | **React 19 + TypeScript + Vite 6 + Tailwind v4 + shadcn/ui**（Radix）+ `framer-motion` + `lucide-react` + `@tanstack/react-query` | 同 untype `package.json` |
| IPC | 前端 `src/lib/ipc.ts` 用 `invoke<T>("cmd", {args})` 做**类型化封装**；后端 emit 事件，前端用 `useTauriEvent` 监听 | 照 untype `src/lib/ipc.ts` + `useTauriEvent.ts` 模式 |
| 构建 | `npm run build`(tsc+vite) → `../build`；`npm run tauri build` 出包 | 同 untype `tauri.conf.json` `frontendDist:"../build"` |
| 签名(mac) | **ad-hoc**（`signingIdentity:"-"`，`hardenedRuntime:true` + `entitlements.plist`）即可本机分发；可选 Developer ID + 公证 | 同 untype；untype 自己也是 ad-hoc 默认 |
| 自动更新 | `tauri-plugin-updater`（GitHub releases `latest.json` + minisign pubkey）——**GUI 自身的更新**，与"给 Claude 汉化"无关 | 同 untype；可后置 |
| CI | GitHub Actions：`macos-latest` + `windows-latest` 矩阵 | untype 现状只有 mac，本项目要补 win，见 §7 |
| release profile | `opt-level="s"` + `lto="thin"` + `strip="symbols"`（优化体积） | 抄 untype `Cargo.toml` |

**从 untype 删掉的（汉化工具不需要）**：所有 ASR/语音相关 —— `sherpa-rs`、`hound`、`cpal`、`enigo`、`pinyin`、`rustfft`、`tungstenite`/`flate2`/`base64`（云 ASR）、`macos-accessibility-client`、`core-graphics` 键盘监听、`models/`、capsule 窗口、热键、词典、LLM 整理等。后端只保留「设置持久化(prefs)」「托盘(可选)」「单窗口」的骨架。

**新增依赖（后端汉化逻辑要用）**：
```toml
serde = { version = "1", features = ["derive"] }
serde_json = "1"
regex = "1"            # 语言白名单自适应探测
dirs = "5"            # 跨平台定位 home/config 目录
zip = "0.6"           # 备份/恢复（或 mac 直接调 ditto、win 用 zip crate）
plist = "1"           # 读写 Info.plist / 提取-过滤 entitlements（mac）
walkdir = "2"         # 目录遍历（找 index-*.js / en-US.json）
[target.'cfg(target_os = "windows")'.dependencies]
winreg = "0.52"       # 注册表兜底定位 Claude（旧 detector.rs 已有思路）
```

---

## 2. 项目落位与脚手架（Phase 0）

1. **新建目录** `ClaudeCN/gui-tauri/`。最省事的起步：把 untype 的壳拷过来再精简（保证栈/惯例一致），或 `npm create tauri-app@latest`（React+TS 模板）后对齐 untype 结构。推荐**拷壳**：
   - 拷 untype 的：`package.json`、`tsconfig.json`、`vite.config.ts`、`components.json`、`index.html`、`src/lib/{utils.ts,ipc.ts,useTauriEvent.ts}`、`src/index.css`、`src-tauri/{Cargo.toml,tauri.conf.json,build.rs,entitlements.plist}`、`.github/workflows/`、`scripts/`、`branding/gen-icon.cjs`。
   - **删**：untype 的业务前端（`src/settings/*` 的 ASR UI、`src/capsule/*`）、后端业务 `.rs`（asr/audio/cloud_asr/recorder/vad/viz/vocab/learn/llm/keytap/inject/pipeline/activation）、`models/`、capsule 窗口配置。
2. **改身份**：`tauri.conf.json` 的 `productName`（如 `ClaudeCN`）、`identifier`（如 `com.claudecn.gui`）、`version`、窗口标题/尺寸；`package.json` name；图标用 `branding/gen-icon.cjs` 生成（沿用 ClaudeCN 现有 logo `ClaudeCN/logo_final*.png`）。
3. **精简 `Cargo.toml`**：按 §1 删 ASR 依赖、加汉化依赖，保留 `[profile.release]`。
4. **窗口**：单个主窗口（参考 untype 主窗口配置：`width:560,height:520` 左右，`titleBarStyle:"Overlay"`, `hiddenTitle:true`；汉化工具用不上 capsule/常驻，但**可保留托盘**让它像旧版菜单栏应用，二选一即可）。
5. **验收**：`npm install && npm run tauri dev` 能起一个空白窗口。

---

## 3. 后端架构（Rust）

```
src-tauri/src/
├── main.rs                # 入口（同 untype，调 lib::run()）
├── lib.rs                 # Tauri builder + 命令注册 + AppState + emit 进度
├── prefs.rs               # 设置持久化（抄 untype prefs.rs 模式，极简）
├── localize/
│   ├── mod.rs             # 跨平台公共逻辑：探测、合并译文、校验、备份策略
│   ├── assets.rs          # 内置译文资源的读取（Tauri resource_dir）
│   ├── macos.rs           # #[cfg(macos)] 重签名 + osascript 提权换入
│   └── windows.rs         # #[cfg(windows)] takeown/icacls 提权 + 直接替换
```

- **公共逻辑**（mod.rs）：`detect()`、`build_merged()`、`write_translations()`、`patch_whitelist()`、`verify()`、`backup()/existing_backup()`、locale 配置读写。这些与 OS 无关，**直接对照 `patch_macos.py` 移植**。
- **OS 差异**只在两处：①「换入已修改的 app」的提权方式；② mac 要重签名、win 不要。用 `#[cfg(target_os=...)]` 分流，公共逻辑共用。
- **AppState**：极简，主要放「当前是否正在处理」的锁 + 进度通道。参考 untype 的 `AppState`/`Mutex` 模式。
- **进度**：长流程里用 `app.emit("localize-progress", "正在合并译文…")` 往前端推（照 untype `lib.rs` 的 `app.emit(...)` 模式）；前端 `useTauriEvent("localize-progress", cb)` 接。

---

## 4. 汉化逻辑（核心 · 逐条移植自 claude-localize skill）

> **方法**：打开 `skills/claude-localize/scripts/patch_macos.py`，把每个函数翻成 Rust。下面是映射表 + 必须保留的关键机制（每条都有"为什么"，别图省事删）。Windows 端把 `windows/src/patcher.rs` 搬过来再补上 ①③ 两个修正。

### 4.0 ⚠️ 踩坑点：定位 Claude 安装路径（运行时自动探测，别让用户填）

汉化改的是 **Claude 安装后实际运行的文件**，跟"安装包(.dmg/.exe)下载到哪"无关（安装包用完可删，工具不碰）。所以**第一步永远是自动找到「安装位置」**，不要要求用户输入路径。

- **macOS（简单）**：按序查 `/Applications/Claude.app` → `~/Applications/Claude.app`，基本就这两处。
- **Windows（杂，检测器要厚）**：安装位置不统一，必须多路兜底——
  - `%LOCALAPPDATA%\AnthropicClaude\app-<版本>\`（最常见的 per-user 安装；**`app-<版本>` 子目录每次更新都变，必须 glob 匹配，别写死版本号**）
  - `%LOCALAPPDATA%\Programs\Claude`、`Program Files\Claude` 等多个变体
  - **注册表**兜底查询
  - **微软商店 MSIX 版**：路径受保护，用 `Get-AppxPackage *Claude*` 查包、`shell:AppsFolder\<PackageFamilyName>!Claude` 启动
  - 这套逻辑 `windows/src/detector.rs` 已经写好，**直接搬**。
- **找不到时**：明确报「未检测到 Claude，请先安装」，**别瞎操作**；并留**手动指定路径**的兜底（skill 用 `--app` 参数；GUI 做成"自动没找到 → 弹文件选择器让用户指定 Claude 位置"）。
- 探测结果（安装路径、版本、是否已汉化、有无备份）由 `detect_claude` 命令一次性返回，驱动前端状态机。

### 4.1 Python → Rust 函数映射（macOS）

| patch_macos.py | 新 Rust | 说明 |
|---|---|---|
| `find_app / app_version` | `detect::find_app / app_version` | mac 读 `Contents/Info.plist` 的 `CFBundleShortVersionString`（用 `plist` crate） |
| `find_i18n_dir / find_assets_dir` | 同名 | 优先标准路径，否则 `walkdir` 找含 `en-US.json` 的目录 / 含 `index-*.js` 的目录（**自适应**，别写死） |
| `en_source` | 同名 | 纯净英文源：有 `en-US.original.json` 用它，否则用 `en-US.json` |
| `build_merged` | 同名 | en-US 为底 + 内置 base（+ 未来 delta）→ merged；未译 key 回退英文 |
| `write_frontend` | `write_translations` | **见 §4.2 关键①** |
| `write_desktop / write_statsig` | 同名 | 桌面菜单 `Contents/Resources/zh-CN.json` + `zh-CN.lproj/Localizable.strings`；statsig |
| `patch_whitelist` | 同名 | **见 §4.2 关键②** |
| `filter_entitlements / resign` | `macos::resign` | **见 §4.2 关键③（mac 专属）** |
| `osascript_admin` | `macos::privileged_swap` | mac 用 `osascript -e 'do shell script "…" with administrator privileges'` |
| `backup / existing_backup` | 同名 | **见 §4.2 关键④** |
| `write_locale_config / remove_locale_config / claude_config_dirs` | 同名 | **见 §4.2 关键⑤** |
| `is_patched` | 同名 | 用 `en-US.original.json` 是否存在作标记（比 config.locale 可靠，后者被 app 回写） |
| `cmd_apply / cmd_restore` | `localize() / restore()` | 编排；恢复带**防降级保护**（§4.2 关键④） |

### 4.2 五个必须保留的关键机制（含"为什么"）

**① 把中文写进 app 真正加载的 locale 文件——尤其覆盖 `en-US.json`**
- 实际生效的 locale 由**账号/服务端**决定（登录英文账号常是 `en-US`），且 app 会把它**回写**进 `config.json`。所以"只设 config.locale=zh-CN"会被覆盖、**无效**。
- 因此 `write_translations` 要把 merged 中文**同时写入** `en-US.json`(覆盖!)、`zh-CN.json`、`zh.json`（外加各自的 `dynamic/<loc>.json` 兜底 与 `<loc>.overrides.json` 空占位）。未译 key 已在 merged 里回退英文，覆盖 en-US **不丢英文兜底**。
- 覆盖前把原始 `en-US.json` 存一份为 `en-US.original.json`（仅首次），供重复打补丁/合并时作纯净英文源、并作"已汉化"标记。

**② 语言白名单自适应**
- 新版 Claude（≥1.15200）已无硬编码语言数组，`patch_whitelist` 探测不到就 **skip（不报错）**。仅当某版本又出现 `["en-US",...]` 数组时才注入 `"zh-CN"`。三套正则见 `patch_macos.py`。

**③ 重签名（仅 macOS）= ad-hoc + 过滤 entitlements**
- 改了包内文件 → 官方签名失效 → **必须重签**。新版 macOS 的 AMFI 在启动时校验：用 ad-hoc/自签却保留**绑定 Team 的 entitlements** 会被拒（`open` 报 error 163 "Launchd job spawn failed"）。
- 做法：`codesign -d --entitlements - --xml` 提取原 entitlements → **剥掉** `keychain-access-groups`、`com.apple.application-identifier`、所有 `com.apple.developer.*` → **补上** `com.apple.security.cs.{allow-jit, allow-unsigned-executable-memory, disable-library-validation, allow-dyld-environment-variables}` → 其余（`device.*`/`personal-information.*`/`virtualization` 等）**原样保留**（这些在 ad-hoc 下可用，保住摄像头/麦克风/截屏能力）→ `codesign --force --deep --options runtime --entitlements <过滤后> --sign - <app>`。先 `codesign --remove-signature --deep`。
- 顺带把 `Info.plist` 的 `ElectronTeamID` 设为 `"not set"`（与 ad-hoc 对齐）。
- **Windows 不需要重签名**（不强制校验），直接改文件即可。

**④ 备份 + 防降级 + 缺失友好提示**
- 备份成 zip（mac 用 `ditto -ck --keepParent`；win 用 zip crate），放 `~/Library/Application Support/claude-localize/backups/`（mac）/ `%APPDATA%\claude-localize\backups\`（win），目录里放 `请勿删除_README.txt`。**复用**：同版本已存在就不重复占空间；**兼容**旧 GUI 的 `ClaudeCN/backups/` 位置（恢复时也去那找）。
- `restore` 三重保护：① 找不到备份 → 不报错，提示"Claude 可从 claude.ai 免费重装，数据/登录在 `~/Library/Application Support/Claude/` 与 app 分开，重装不丢"；② **防降级**：当前 app 已不是本工具汉化状态、且备份是更旧版本（多半 Claude 自己更新过）→ 中止并提示，除非 `--force`；③ 正常则从 zip 还原 + 清 locale + 重启。

**⑤ locale 配置写到所有 Claude 数据目录**
- 关键坑：接第三方/自定义模型时 Claude 用 `Claude-3p/` 目录（活动配置可能只在这里）。所以遍历 `~/Library/Application Support/Claude` 和 `Claude-*`（**排除我们自己的 `ClaudeCN`/`claude-localize`**），每个的 `config.json` 都写 `locale=zh-CN`。Windows 对应 `%APPDATA%\Claude` 和 `Claude-3p`（旧 `patcher.rs` 已这么做）。
- 注：因为有了 ① 的 en-US 覆盖，locale 配置其实是次要的（兜底/未登录场景用），但仍要写。

### 4.3 翻译资源 & 覆盖率（GUI 与 skill 的分工）

- GUI **打包内置** `zh-CN.base.json` 等为 Tauri `bundle.resources`，运行时从 `app.path().resource_dir()` 读（参考 untype `load_vocab` 的 resource_dir 模式）。
- GUI **不做现场翻译**（没有 LLM）。覆盖率 = 内置 base 与当前 Claude 版本的匹配度。当前 base 对 1.15200.0 是 ~100%。
- **新版译文的维护流程**：Claude 出新版、新增英文文案时，由维护者跑 **claude-localize skill**（它能用模型翻译增量、自增长 base），再把更新后的 `zh-CN.base.json` **回灌**进 GUI 资源、发新版 GUI。即：**skill = 翻译引擎，GUI = 消费端**。这条要写进 GUI 的发布 checklist。

### 4.4 用户须知（UI 要主动提示）

- **必须开启 Claude 开发者模式**（Settings → Developer）：否则 app 可能加载远程 claude.ai 网页版，本地汉化不被读取。汉化前 UI 要引导用户开。
- **汉化期间 Claude 自动更新会被跳过/失败**（Squirrel 校验签名，ad-hoc ≠ 官方）。想升级：先「恢复」→ 让 Claude 更新 → 再「汉化」。UI 给一句说明。
- **首次可能要重新授权一次系统权限**（重签改了签名身份，TCC 可能重弹屏幕录制/麦克风）。

---

## 5. 前端 UI（React · 照 untype 风格）

- **单窗口**，整体走 untype 的视觉（tailwind v4 + shadcn + framer-motion + lucide 图标）。可保留**托盘**让它像旧菜单栏应用。
- **状态机**（后端 `detect` 返回，前端 react-query 拉）：
  - `claude_not_installed` → 提示去 claude.ai 下载
  - `not_patched` → 显示「一键汉化」主按钮 + 覆盖率预览 + 开发者模式提示
  - `patched` → 显示「已汉化 ✓」+「一键恢复」按钮
  - `working` → 进度条 + 实时进度文字（监听 `localize-progress` 事件）+ 完成/失败结果
- **组件清单**：状态卡、主操作按钮（汉化/恢复）、进度区、开发者模式引导卡、版本/覆盖率信息、**复制日志按钮**（旧 GUI v1.2.3 有，便于用户反馈）、关于页（作者信息、免费声明，沿用现 README）。
- **IPC**（`src/lib/ipc.ts`，照 untype 封装）：
  ```ts
  export const detectClaude = () => invoke<ClaudeStatus>("detect_claude");
  export const applyLocalization = () => invoke<LocalizeResult>("apply_localization");
  export const restoreOriginal = (force=false) => invoke<void>("restore_original", { force });
  export const openDevModeHelp = () => invoke<void>("open_dev_mode_help");
  export const copyLogs = () => invoke<string>("collect_logs");
  ```
- **事件**：`localize-progress`(string)、`localize-done`(result)、`localize-error`(string)。用 `useTauriEvent` 监听。

---

## 6. IPC 契约（前后端命令清单）

| 命令 | 入参 | 返回 | 作用 |
|---|---|---|---|
| `detect_claude` | — | `ClaudeStatus{installed,version,patched,has_backup,coverage_pct,dev_mode_hint}` | 探测，驱动 UI 状态机 |
| `apply_localization` | — | `LocalizeResult{ok,coverage_pct,version}` | 执行汉化（内部 emit 进度），mac 触发一次提权 |
| `restore_original` | `{force:bool}` | `void` | 恢复（含防降级/缺失提示，提权一次） |
| `open_dev_mode_help` | — | `void` | 打开开发者模式说明（或直接 `tauri-plugin-opener` 开网页） |
| `collect_logs` | — | `String` | 汇总最近一次处理日志，供「复制日志」 |

事件：`localize-progress` / `localize-done` / `localize-error`。

---

## 7. 跨平台 & CI（Windows 怎么出 · 重点）

两条路都给，按场景选：本地交叉编（开发/快速出包）+ CI 矩阵（正式发布）。

### 7A. 本地从 macOS 交叉编译 Windows（官方支持）

Tauri v2 **能**在 Mac 上交叉编出 Windows NSIS 安装包，用 `cargo-xwin` 自动下载 MSVC SDK：
```bash
brew install nsis llvm
export PATH="/opt/homebrew/opt/llvm/bin:$PATH"     # 提供 llvm-rc（编 Windows 资源/图标）与 lld
rustup target add x86_64-pc-windows-msvc
cargo install --locked cargo-xwin
# 在 gui-tauri/ 下：
npm run tauri build -- --runner cargo-xwin --target x86_64-pc-windows-msvc
# 产物：src-tauri/target/x86_64-pc-windows-msvc/release/bundle/nsis/*-setup.exe
# 可设 XWIN_CACHE_DIR 让多项目共享下载的 Windows SDK
```
**三个关键差异/限制（务必知道）**：
1. **目标是 MSVC（`x86_64-pc-windows-msvc`），不是你现在编纯 Rust GUI 用的 mingw/GNU 目标**——Tauri 依赖 WebView2，官方只支持 MSVC，故旧的 `x86_64-pc-windows-gnu`+mingw 那套**不适用**，改用 cargo-xwin。
2. **只能出 NSIS `-setup.exe`，MSI(WiX) 不能交叉编**（WiX 仅 Windows）。本工具用 exe 安装器足够。
3. **交叉编出的 exe 若要签名需外部签名工具**（mac 上没有 Windows signtool）；本工具本就不签 Windows，跳过即可。官方提示交叉编"测试较少、宜作后备"，故**正式发布建议走 7B**。

### 7B. GitHub Actions 矩阵（原生编译 · 正式发布最稳）

各 OS 用原生 runner 编：

- `macos-latest` → `.app` + `.dmg`（ad-hoc 或 Developer ID）
- `windows-latest` → `.exe`(NSIS) / `.msi`(WiX)，原生编译，无需交叉

**推荐用官方 `tauri-apps/tauri-action`** 一把梭（编译 + 出包 + 建 release）。`.github/workflows/release.yml` 草案：
```yaml
name: release
on: { push: { tags: ['v*'] } }
jobs:
  build:
    strategy:
      matrix:
        include:
          - { os: macos-latest,  args: '--target aarch64-apple-darwin' }
          - { os: macos-latest,  args: '--target x86_64-apple-darwin' }
          - { os: windows-latest, args: '' }
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4 with: { node-version: 22, cache: npm }
      - uses: dtolnay/rust-toolchain@stable
      - run: npm ci
      - uses: tauri-apps/tauri-action@v0
        env: { GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }} }
        with:
          projectPath: gui-tauri
          tagName: ${{ github.ref_name }}
          releaseName: 'ClaudeCN ${{ github.ref_name }}'
          args: ${{ matrix.args }}
```
（另保留一个 `ci.yml` 抄 untype：macos 上 `tsc` + `clippy -D warnings` + `cargo test`，但**删掉 untype 那些 sherpa/onnx 模型占位的步骤**——本项目没有模型依赖，CI 干净很多。）

**Windows 特有注意**：
- **提权**：Claude 装在受保护目录，整个 app 需**管理员运行**。在 Tauri 的 Windows 配置里给 NSIS/可执行加 `requireAdministrator` manifest（或运行时检测+自提升）。旧 `patcher.rs` 已有 `takeown`/`icacls` 拿目录写权限的逻辑，直接复用。
- **WebView2**：Tauri Windows 依赖 WebView2 运行时（Win10/11 多半自带；NSIS 安装器可配置自动引导安装）。
- **MSIX 安装版 Claude**：旧 `detector.rs`/`patcher.rs` 已处理（PowerShell 查 `Get-AppxPackage *Claude*` + `shell:AppsFolder` 启动），搬过来。
- **签名**：Windows 可不签名（用户可能遇杀软误报，UI 给提示 + 旧版"复制日志"按钮便于反馈）。

**mac 提权 vs win 提权差异**：mac 用 `osascript` **局部**提权（仅换入那一步弹密码框，app 本身普通权限运行）；Windows 习惯是**整个 app 管理员运行**。两端 UX 文案要分别写。

---

## 8. 迁移映射（从旧代码搬什么）

| 旧 | 新 | 怎么办 |
|---|---|---|
| `Sources/ClaudeCN/ClaudePatcher.swift` | `localize/macos.rs` + `localize/mod.rs` | 逻辑已被 `patch_macos.py` 取代且**修正过**（en-US 覆盖、ad-hoc 过滤签名、Claude-3p）。**以 skill 为准**，Swift 仅作交叉参考。 |
| `Sources/ClaudeCN/{AppState,MenuBarView,AppDelegate}.swift` | React 前端 + `lib.rs` | UI 重写；状态机/进度/通知概念照搬。 |
| `windows/src/patcher.rs` | `localize/windows.rs` + 公共 `mod.rs` | **大量直接复用**；但要补 skill 的修正：①en-US.json 覆盖(+zh/zh-CN/dynamic/overrides)、③win 无需签名、⑤已有 Claude+Claude-3p 多目录。把"合并/写文件/白名单"上提到公共 `mod.rs` 与 mac 共用。 |
| `windows/src/{detector,backup,logger}.rs` | 对应模块 | detector（多路径+注册表+MSIX）、backup 直接搬。 |
| `translations/zh-CN.json` 等 | `gui-tauri/src-tauri/resources/` | 改用 skill 的 `assets/*.base.json`（已自增长到 18,686 条、更新）。 |

---

## 9. 分阶段任务清单（按序执行，每阶段有验收）

- **Phase 0 · 脚手架**：拷 untype 壳 → 精简 → 改名/图标 → `npm run tauri dev` 起空窗口。✅ 空壳能跑。
- **Phase 1 · mac 后端**：移植 `patch_macos.py` → `localize/{mod,macos,assets}.rs`，打包内置译文资源。先做 `detect_claude`，再 `apply_localization`（含 en-US 覆盖、ad-hoc 过滤签名、Claude-3p locale、提权换入、进度 emit），再 `restore_original`（含防降级）。**写 Rust 单测**覆盖：合并逻辑（en 全覆盖、未译回退英文）、entitlements 过滤清单、config 目录枚举（排除 ClaudeCN）。✅ 命令行/dev 下能把本机 Claude 汉化成中文并能启动、能恢复。
- **Phase 2 · mac 前端**：React UI + ipc.ts + 事件，打通状态机与全流程；开发者模式引导、复制日志、关于页。✅ 纯点界面能完成汉化/恢复。
- **Phase 3 · Windows 后端**：搬 `windows/src/*` → `localize/windows.rs`，补 §4.2 的 ①③⑤ 修正；提权 manifest；MSIX/exe 双安装形态。✅ Windows 机/VM 上 `npm run tauri build` 出 exe 且能汉化/恢复。
- **Phase 4 · CI & 发布**：`ci.yml`(检查) + `release.yml`(矩阵出 dmg+exe)；GUI 自身 updater(可选，后置)。✅ 打 tag 自动产出两端安装包。

> 建议执行者用 TaskCreate 把以上 5 个 Phase 建成任务清单跟踪。

---

## 10. 验收与回归测试

- **mac**：`apply` → Claude 重启**显示中文**且正常启动（`spctl` rejected 正常、无 quarantine 即可启动）；`restore` → 英文官方原版（`codesign --verify` 过、TeamIdentifier 回到 Anthropic）；防降级：手动换新版 Claude 后 `restore` 应中止提示而非降级。
- **win**：管理员运行 → `apply` 中文、`restore` 英文；MSIX 与 exe 两种安装都覆盖。
- **回归**：覆盖率显示正确；开发者模式未开时给提示；汉化期间自动更新行为已在 UI 说明。

---

## 附录 A · 关键事实速查（本次调研沉淀，权威细节见 skill `references/mechanism.md`）

- **Claude 前端 i18n**：`Contents/Resources/ion-dist/i18n/<locale>.json`（key=英文原文哈希，跨版本稳定）；运行时按 `localeOverride ?? config.locale` 动态 `fetch('/i18n/<loc>.json')` + `dynamic/<loc>.json` + `<loc>.overrides.json`；locale 受**服务端 bootstrap 覆盖**并回写 config。→ 故**覆盖 en-US.json** 最稳。
- **桌面/原生层**：`Contents/Resources/<loc>.json` + `<loc>.lproj/Localizable.strings`；主进程在 `app.asar`。
- **加载前提**：必须开**开发者模式**（否则走远程 claude.ai）。
- **签名 entitlements 过滤**：剥 `keychain-access-groups` / `com.apple.application-identifier` / `com.apple.developer.*`；补 `cs.allow-jit` / `cs.allow-unsigned-executable-memory` / `cs.disable-library-validation` / `cs.allow-dyld-environment-variables`；留其余 `security.*`。
- **配置目录**：`~/Library/Application Support/Claude` 和 `Claude-3p`（win 在 `%APPDATA%`）。
- **备份**：`~/Library/Application Support/claude-localize/backups/`（兼容旧 `ClaudeCN/backups/`）。
- **自动更新**：Claude 用 Squirrel.Mac，校验签名 → 汉化(ad-hoc)期间不自动更新；升级要先恢复。

## 附录 B · untype 参考文件清单（照着抄惯例）

- 栈/配置：`package.json`、`src-tauri/{Cargo.toml,tauri.conf.json,entitlements.plist,build.rs}`
- 后端惯例：`src-tauri/src/lib.rs`（Builder、`generate_handler!`、`app.emit`、tray、prefs 加载、resource_dir）、`prefs.rs`、`settings.rs`
- 前端惯例：`src/lib/{ipc.ts,useTauriEvent.ts,utils.ts}`、`src/settings/{App.tsx,main.tsx}`、`src/index.css`、`components.json`
- 构建/发布：`scripts/{build-mac.sh,release-mac.sh,gen-latest-json.sh}`、`.github/workflows/ci.yml`、`scripts/UPDATE_SETUP.md`
- 图标：`branding/gen-icon.cjs`
