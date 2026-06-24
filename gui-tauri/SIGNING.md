# ClaudeCN 本地发版 / 签名 / 公证 / 自更新 — 指南

ClaudeCN 在**你自己的 Mac 上本地打包**（跟 untype 一样），不走 CI 自动发版。
证书直接用钥匙串里那张 Developer ID（不用导 .p12），公证可复用 untype 的 App Store Connect API Key。

macOS 自更新要可靠，需三组：**① 代码签名(Developer ID) ② 公证(notarization) ③ 更新签名(minisign)**。
缺公证的话，自动下载的新包会被 Gatekeeper 拦、更新损坏。

---

## 一、一次性配置

把凭据写进 `gui-tauri/scripts/signing.local.env`（已 gitignore，不会提交）：

```bash
cd gui-tauri
cp scripts/signing.local.env.example scripts/signing.local.env
# 然后编辑 scripts/signing.local.env
```

里面三组（多数已替你填好/可省略）：

```bash
# ① 代码签名 —— 已填好你的证书名（直接用钥匙串，不用导 .p12）
export APPLE_SIGNING_IDENTITY="Developer ID Application: Yonghao Huang (Q9S5VDGD8Q)"

# ② 公证 —— 从你 untype 的 signing.local.env 原样拷这三行过来（复用同一个 API Key）
export APPLE_API_ISSUER="..."        # Issuer ID
export APPLE_API_KEY="..."           # Key ID
export APPLE_API_KEY_PATH="$HOME/.appstoreconnect/AuthKey_XXXXXXXXXX.p8"

# ③ 更新签名 —— 可省略：build-mac.sh 默认读 ~/.claudecn-updater/claudecn.key（已生成）
```

> ⚠️ minisign 私钥（`~/.claudecn-updater/claudecn.key`）务必备份、勿提交。丢了 = 已装用户再也收不到后续更新。

---

## 二、发一个新版本

```bash
cd gui-tauri

# 1) 三处版本号一起改（保持一致），如 0.1.0 → 0.2.0：
#    - src-tauri/tauri.conf.json   "version"
#    - src-tauri/Cargo.toml        version
#    - package.json                version

# 2)（可选）写更新说明，会进 release 和弹窗的「更新内容」
printf '• 修复了 xxx\n• 新增 yyy\n' > RELEASE_NOTES.md

# 3) 提交并推送代码（先 push，再发版）
git add -A && git commit -m "release: v0.2.0" && git push

# 4) 构建（签名 + 公证 + updater 产物；公证要几分钟）
bash scripts/build-mac.sh

# 5) 生成 latest.json（更新清单）
bash scripts/gen-latest-json.sh

# 6) 发布到 GitHub Releases（正式版，非 draft/prerelease）
bash scripts/release-mac.sh
```

发布后，装了旧版的用户下次打开 ClaudeCN 约几秒后就会弹「发现新版本」。

> 注意：必须是「正式 release」，`releases/latest` 端点才取得到 `latest.json`。`release-mac.sh` 默认发正式版。

---

## 三、验证整条链路
1. 当前 `0.1.0` 装到 /Applications。
2. 版本号 bump 到 `0.2.0`，跑上面 4→6 发布。
3. 打开 `0.1.0` 的 ClaudeCN → 启动应弹「发现新版本 0.2.0」→「立即更新」→ 下载进度 → 自动重启到 0.2.0。
4. 关于页有「检查更新」可手动触发。

## 四、常见问题
- **更新检查不到**：确认 release 是正式版、`latest.json` 已上传、其中 `version` 比当前高、`url` 指向同 release 的 `ClaudeCN.app.tar.gz`。
- **下载完没自动重启**（Tauri v2 macOS 已知 bug）：已内置 `force_quit_and_relaunch` 兜底；仍失败会提示「手动退出后重开」。
- **只出 Apple Silicon(aarch64)**：Intel Mac 不在 `latest.json` 的 `platforms` 里，不会收到更新（跟 untype 一样）。需要的话再加 x86_64/universal。
- **没配公证**：`build-mac.sh` 会回退 ad-hoc（能出包、但自更新可能被 Gatekeeper 拦）。

## 五、Windows？
Windows 包仍建议交给 GitHub Actions 的 `ci.yml`（它会在 windows-latest 上验证编译），或后续单独加一个 Windows 构建。本地 mac 交叉编 Windows 因 `ring`(TLS) 的 C 代码需原生 MSVC/`cargo-xwin`，不方便。
