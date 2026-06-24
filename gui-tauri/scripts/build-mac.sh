#!/usr/bin/env bash
# macOS 本地打包（签名 + 公证 + 自动更新产物）。
#
# 凭据全部放 scripts/signing.local.env（已 gitignore，不提交），本脚本 source 它。
# 需要的变量见 scripts/signing.local.env.example，分三组：
#   ① 代码签名： APPLE_SIGNING_IDENTITY="Developer ID Application: 你的名字 (TEAMID)"
#   ② 公证(API Key)： APPLE_API_ISSUER / APPLE_API_KEY / APPLE_API_KEY_PATH（或 APPLE_ID/APPLE_PASSWORD/APPLE_TEAM_ID）
#   ③ 更新签名(minisign)： 缺省自动读 ~/.claudecn-updater/claudecn.key
#
# 三组齐全 → 产出已签名+已公证的 .dmg，以及自动更新用的 ClaudeCN.app.tar.gz(+.sig)。
# 缺哪组就少哪步：无签名=ad-hoc；无公证=Gatekeeper 拦自动更新；无更新签名=不产出 updater 产物。
set -euo pipefail
cd "$(dirname "$0")/.."

if [ -f scripts/signing.local.env ]; then
  set -a; . scripts/signing.local.env; set +a
else
  echo "→ 无 scripts/signing.local.env，回退 ad-hoc 签名（仅本机可用、无公证、无更新签名）"
fi

# 更新签名私钥：未显式给 TAURI_SIGNING_PRIVATE_KEY 时，自动从默认密钥文件读取。
DEFAULT_KEY="$HOME/.claudecn-updater/claudecn.key"
if [ -z "${TAURI_SIGNING_PRIVATE_KEY:-}" ] && [ -f "$DEFAULT_KEY" ]; then
  export TAURI_SIGNING_PRIVATE_KEY="$(cat "$DEFAULT_KEY")"
  export TAURI_SIGNING_PRIVATE_KEY_PASSWORD="${TAURI_SIGNING_PRIVATE_KEY_PASSWORD:-}"
fi

# 状态自检
echo "→ 代码签名: ${APPLE_SIGNING_IDENTITY:-(未设, ad-hoc)}"
if [ -n "${APPLE_API_KEY:-}" ] && [ -n "${APPLE_API_ISSUER:-}" ]; then
  echo "→ 公证: App Store Connect API Key (key ${APPLE_API_KEY})"
elif [ -n "${APPLE_ID:-}" ] && [ -n "${APPLE_PASSWORD:-}" ]; then
  echo "→ 公证: Apple ID (${APPLE_ID})"
else
  echo "→ 公证: (未配置, 自动更新的新包会被 Gatekeeper 拦——见 scripts/signing.local.env.example)"
fi
if [ -n "${TAURI_SIGNING_PRIVATE_KEY:-}" ]; then echo "→ 更新签名: 已就绪"; else echo "→ 更新签名: (未设, 不产出 updater 产物)"; fi

npm run tauri -- build "$@"

# Tauri 只公证 .app、不公证 .dmg 容器——而下载时拿到 quarantine 标记的是 DMG，
# 不公证 DMG 的话别人双击下载的 DMG 会被 Gatekeeper 拦。这里补上对 .dmg 的公证 + staple。
notarize_dmg() {
  local dmg="$1"
  echo "→ 公证 DMG: $(basename "$dmg")"
  if [ -n "${APPLE_API_KEY:-}" ] && [ -n "${APPLE_API_ISSUER:-}" ] && [ -n "${APPLE_API_KEY_PATH:-}" ]; then
    xcrun notarytool submit "$dmg" --key "$APPLE_API_KEY_PATH" --key-id "$APPLE_API_KEY" --issuer "$APPLE_API_ISSUER" --wait
  elif [ -n "${APPLE_ID:-}" ] && [ -n "${APPLE_PASSWORD:-}" ] && [ -n "${APPLE_TEAM_ID:-}" ]; then
    xcrun notarytool submit "$dmg" --apple-id "$APPLE_ID" --password "$APPLE_PASSWORD" --team-id "$APPLE_TEAM_ID" --wait
  else
    echo "  （未配公证凭据，跳过 DMG 公证——下载版会被 Gatekeeper 拦）"; return 0
  fi
  xcrun stapler staple "$dmg"
}

shopt -s nullglob
for dmg in src-tauri/target/release/bundle/dmg/*.dmg; do
  notarize_dmg "$dmg"
done
