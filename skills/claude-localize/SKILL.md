---
name: claude-localize
description: 将 Claude Desktop 桌面客户端界面汉化为简体中文（zh-CN），以及一键恢复英文原版。当用户提到「汉化 Claude」「把 Claude 客户端/桌面端改成中文」「Claude 中文界面」「Claude 改中文」「localize / translate Claude Desktop to Chinese」「恢复 Claude 英文/原版」，或在维护 ClaudeCN 项目、需要适配新版 Claude 的汉化时，都应使用本 skill。即使用户没明说「汉化」二字，只要意图是让 Claude 桌面端显示中文界面，也要触发。本 skill 会在运行时读取已安装 Claude 的 i18n 结构来自适应当前版本，因此 Claude Desktop 更新后依然可用——这正是相对写死版本号的工具的关键优势。
---

# Claude Desktop 中文汉化 (claude-localize)

把本机安装的 **Claude Desktop** 桌面客户端汉化为简体中文，并可随时恢复英文原版。

**核心理念是自适应。** Claude Desktop 更新非常频繁，写死路径和正则的工具每次更新都会失效。本 skill 改为**运行时探测**：先读当前安装版本的 i18n 目录结构、语言加载机制和翻译覆盖率，再决定怎么打补丁。所以即使 Claude 改了内部结构，你（运行本 skill 的 Claude）也能现场判断、调整，而不是盲目套用旧步骤。

目前实现：**macOS**。Windows 见 `references/windows.md`（规划中）。

## 它怎么工作（为什么这么做）

Claude Desktop 的界面文案分两层：
1. **前端 UI**（Electron 渲染层）：文案在 `Contents/Resources/ion-dist/i18n/{locale}.json`，key 是**英文原文的哈希**。运行时按当前 locale 动态加载对应文件。因为 key 是内容哈希，所以只要英文原文没变，旧译文就还能对上——这让"内置精校译文 + 只翻新增"成为可能。
2. **桌面/原生层**（菜单、托盘、原生弹窗）：在 `Contents/Resources/{locale}.json` 和 `{locale}.lproj/Localizable.strings`。

汉化的关键有三点（细节见 `references/mechanism.md`）：

1. **把中文译文写进 app 真正加载的 locale 文件。** 实际生效的 locale 由账号/服务端决定（登录英文账号时常是 `en-US`），而且 app 会把它**回写**进 `config.json`——所以光设 `config.locale=zh-CN` 不管用、会被覆盖。因此本 skill 直接**覆盖 `en-US.json`**（以及 `zh-CN.json`/`zh.json`），未译 key 仍回退英文；原始英文存为 `en-US.original.json` 以备恢复与重复打补丁。
2. **必须开启 Claude 的开发者模式**（Settings → Developer）。否则 app 可能加载远程 claude.ai 网页版，本地译文文件根本不会被读取。
3. **桌面/原生层**：写 `Contents/Resources/zh-CN.json` 和 `.lproj/Localizable.strings`。

**关于语言白名单（重要，曾导致白屏）：** 新版 Claude 仍有“支持的 locale 列表”数组（`["en-US","de-DE",…]`），只是从 `index-*.js` **搬进了内容哈希的 chunk**（如 `assets/v1/ccc72bfa9-*.js`）。早期只扫 `index-*.js` 会漏看它、误以为“无需改 JS”，导致 zh-CN 从没被登记成真正的 locale、汉化全靠覆盖 `en-US.json` 硬撑——一旦某机器协商出的 locale 没有对应 i18n 文件就 404、渲染层永不就绪 → **白屏**（详见 `references/mechanism.md` 的“汉化后白屏”）。现在 skill **扫描全部 `*.js`** 找到该数组并注入 `zh-CN`，并为整组中文别名都写好 i18n 文件、换入前自检，从根上杜绝白屏。

因为修改了 app 包内文件会让 macOS 代码签名失效，所以**必须重签名**（macOS 15+ 还需带 hardened runtime + 关闭 library validation，否则改过的 Electron app 无法启动）。这一步脚本已封装。

确定性的活（备份、合并 JSON、注入白名单、重签名、改配置、校验）都在 `scripts/patch_macos.py` 里；**唯一需要你（模型）动脑的是翻译新增文案**。

## 前置确认

开始前，简要向用户确认：
- 这会修改 `/Applications/Claude.app`，**需要输入一次系统管理员密码**（在 `apply`/`restore` 的换入步骤，会弹出 macOS 原生密码框）。
- 会先**完整备份**原版到 `~/Library/Application Support/ClaudeCN/backups/`，可随时 `restore` 恢复。
- 汉化后 Claude 会自动重启。
- **开启开发者模式是前提**：先让用户在 Claude 内打开 Settings → Developer → 开发者模式，否则 app 加载远程网页版、本地汉化不生效。

## 流程

所有命令在 skill 目录下用 `python3 scripts/patch_macos.py <子命令>` 运行（脚本会自动定位 Claude.app；非标准路径用 `--app` 指定）。

### 1. 探测（detect）

```bash
python3 scripts/patch_macos.py detect
```

读 JSON 输出，重点看：
- `version`：当前 Claude 版本。
- `i18n_dir` / `assets_dir`：若为 `null`，说明结构变了——**停下，去读 `references/mechanism.md` 自适应排查**，不要硬来。
- `whitelist.needs_js_patch`：是否还需把 `zh-CN` 注入“支持 locale 列表”数组。`locale_list_files` 列出命中该数组的 JS（现版在 chunk 里，如 `ccc72bfa9-*.js`），`scanned_js` 是扫描总数。`needs_js_patch:true` 说明找到了数组且缺 zh-CN，apply 会注入；`false` 且 `locale_list_files` 非空表示已含 zh-CN。
- `coverage`：内置精校译文对当前版本的覆盖率（`pct`）和待翻译数量（`untranslated`）。
- `patched`：是否已汉化。`has_backup`：是否已有备份。

### 2. 准备增量（prepare）

```bash
python3 scripts/patch_macos.py prepare
```

它会把"当前版本里、内置精校译文还没覆盖的新增 key"写到 `.work/untranslated.json`（形如 `{"哈希key": "English source"}`）。输出里有 `untranslated_count`。

如果 `untranslated_count` 为 0，跳过第 3 步直接 apply。

### 3. 翻译增量（你来做）

读 `.work/untranslated.json`，把每个英文值翻译成自然、地道的简体中文，**key 保持不变**，结果写到 `.work/delta-zh.json`（同样的 `{key: 中文}` 结构）。

翻译要求（很重要，否则会破坏界面）：
- 先读 `assets/glossary.md` 的术语表，保证与现有译文用词一致（如 Artifacts、Connectors、Cowork、MCP 等的固定译法）。
- **保留占位符与标记**：`{name}`、`{count}`、`%s`、`{0}`、ICU 复数语法 `{count, plural, ...}`、HTML 标签 `<b>...</b>`、Markdown、换行 `\n`、以及原文首尾空格。
- 句子风格简洁、符合中文软件习惯；UI 按钮/菜单用短词（如 "Copy"→"复制"）。
- 数量大时可分批翻，或用子 agent 并行翻；但务必合并成一个完整的 `delta-zh.json`。

翻完后**强烈建议**把 `delta-zh.json` 合并回 `assets/zh-CN.base.json`（见下方"让基准译文自增长"），这样下次跑就不用重翻了——对维护者尤其值。

> 想要快而不求全？可以跳过翻译，直接 apply：未翻的 key 会回退英文，覆盖率即 detect 报的 `pct`（取决于当前 Claude 版本与内置基准译文的匹配度；基准已覆盖的版本可达 ~100%）。先让用户看到中文界面、之后再补全也是合理选择。

### 4. 应用（apply）

```bash
python3 scripts/patch_macos.py apply
```

它依次：备份（版本变了会自动重新备份，绝不降级）→ 复制 app 到临时目录 → 合并译文（en-US 兜底 + 精校 base + 本次 delta）写入前端/桌面/statsig → 探测并按需注入白名单 → 重签名 → 关闭 Claude →**弹密码框**把汉化版换入 `/Applications` → 写 `locale=zh-CN` → 重启 Claude。

输出 JSON 含 `ok`、`coverage_pct`、`whitelist` 处理结果。

### 5. 校验并汇报

- 看 apply 输出 `ok: true`。
- 让用户确认 Claude 重启后界面变中文了。**这一步也顺带验证了本版本的加载机制**——若没生效，去读 `references/mechanism.md` 的"排查"一节。
- 给用户一份简短中文汇报：版本、覆盖率、是否需开发者模式、如何恢复。

## 恢复英文原版

用户只要说「**恢复 Claude 英文 / 恢复原版**」即可触发；或直接运行：

```bash
python3 scripts/patch_macos.py restore
```

从备份还原（同样弹一次密码框），清掉 `locale` 配置并重启 Claude。备份在 `~/Library/Application Support/claude-localize/backups/`（旧 GUI 的 `ClaudeCN/backups/` 也会自动识别复用），独立于 app、不随更新丢失，目录里附 `请勿删除_README.txt`。

**就算备份被误删也不会卡住**：Claude.app 可从 claude.ai 免费重装即恢复官方原版；而聊天记录和登录都存在 `~/Library/Application Support/Claude/`（与 app 分开），重装不丢。restore 找不到备份时会直接给出这个提示，而不是报错卡死。

**防降级保护**：若当前 Claude 已不是本工具汉化的状态、且备份是更旧版本（通常意味着 Claude 自己更新过），restore 会**中止并提示**，避免把 Claude 降级。确需回到备份版本时加 `--force`。

## 关于 Claude 自动更新（重要）

Claude Desktop 用 Squirrel.Mac 自动更新，更新时会**校验代码签名**。汉化后的 app 是我们 ad-hoc 重签的，签名身份与 Anthropic 官方不一致，因此：

- **汉化期间 Claude 的自动更新基本会被跳过/失败**（更新包的官方签名与当前 ad-hoc 签名不匹配）。这是安全的——Claude 只是停在当前版本，不损坏、不丢数据。
- **想升级新版**：先 `restore` 回官方英文版 → 让 Claude 自动更新 → 再跑一次汉化。skill 是自适应的，会读取新版结构、补译新增文案后重新应用。
- 这正是做成 skill 的价值：Claude 每次更新后，一句「汉化 Claude」就能重新适配，无需等作者手动出新版。

## 让基准译文自增长（维护者推荐）

翻译完一次增量后，把它并进内置基准，使覆盖率单调提升、避免重复劳动：

```bash
python3 - <<'PY'
import json, pathlib
base = pathlib.Path("assets/zh-CN.base.json")
work = pathlib.Path(".work/delta-zh.json")
b = json.loads(base.read_text()); d = json.loads(work.read_text())
b.update(d)
base.write_text(json.dumps(b, ensure_ascii=False, indent=2, sort_keys=True))
print(f"基准从 {len(b)-len(d)} 增长到 {len(b)} 条")
PY
```

如果这个 skill 同时是 ClaudeCN GUI 项目的"翻译引擎"，更新后的 `assets/zh-CN.base.json` 可以回灌到 GUI 的 `translations/zh-CN.json` 重新打包发布。

## 当结构变了 / 出错时

不要硬套步骤。`references/mechanism.md` 记录了 Claude Desktop i18n 的完整机制和**自适应排查清单**（i18n 目录找不到、locale 文件名变了、白名单又出现了、签名后无法启动等）。先读它，再用 `detect` 的原始信息判断，必要时直接读 app 内的 `index-*.js` 现场分析。

## 平台

- **macOS**：本文件 + `scripts/patch_macos.py`。
- **Windows**：见 `references/windows.md`（规划中——原理相同，差别在提权方式 takeown/icacls、无需重签名、locale 配置在 `%APPDATA%\Claude\config.json`）。
