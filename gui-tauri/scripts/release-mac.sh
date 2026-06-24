#!/usr/bin/env bash
# 发布一个版本到 GitHub Releases：上传 .dmg + ClaudeCN.app.tar.gz + latest.json。
# 流程：改版本号 → bash scripts/build-mac.sh → bash scripts/gen-latest-json.sh → 本脚本。
# 依赖 gh CLI 已登录（gh auth status）。endpoint 走 releases/latest，故发布为正式 release（非 prerelease）。
set -euo pipefail
cd "$(dirname "$0")/.."

command -v gh >/dev/null || { echo "✗ 需要 gh CLI：brew install gh && gh auth login" >&2; exit 1; }

VERSION="$(python3 -c "import json;print(json.load(open('src-tauri/tauri.conf.json'))['version'])")"
TAG="v${VERSION}"
BUNDLE_MACOS="src-tauri/target/release/bundle/macos"
BUNDLE_DMG="src-tauri/target/release/bundle/dmg"
LATEST="src-tauri/target/release/bundle/latest.json"

TARGZ="$BUNDLE_MACOS/ClaudeCN.app.tar.gz"
DMG="$(ls "$BUNDLE_DMG"/ClaudeCN_*_aarch64.dmg 2>/dev/null | head -1 || true)"

for f in "$TARGZ" "$LATEST"; do
  [ -f "$f" ] || { echo "✗ 缺少 $f（先跑 build-mac.sh + gen-latest-json.sh）" >&2; exit 1; }
done
[ -n "$DMG" ] || echo "⚠ 未找到 .dmg，只发 tar.gz + latest.json"

NOTES_ARG=(--notes "ClaudeCN $TAG")
[ -f RELEASE_NOTES.md ] && NOTES_ARG=(--notes-file RELEASE_NOTES.md)

echo "→ 发布 $TAG 到 Win-Hao/ClaudeCN ..."
# shellcheck disable=SC2086
gh release create "$TAG" \
  --repo Win-Hao/ClaudeCN \
  --title "ClaudeCN $TAG" \
  --latest \
  "${NOTES_ARG[@]}" \
  "$TARGZ" "$LATEST" ${DMG:+"$DMG"}

echo "✓ 已发布 ${TAG} —— 已安装的旧版本下次启动会自动检测到更新。"
