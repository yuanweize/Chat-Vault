#!/bin/bash
set -euo pipefail

PROJECT_DIR="$(cd "$(dirname "$0")" && pwd)"
BUNDLE_DIR="$PROJECT_DIR/src-tauri/target/release/bundle"
RELEASE_DIR="$PROJECT_DIR/release"
PRODUCT_NAME="Chat Vault"
APP_PATH="$BUNDLE_DIR/macos/$PRODUCT_NAME.app"

# 确保 Cargo 在 PATH 中
export PATH="$HOME/.cargo/bin:$PATH"

cleanup() {
    echo "[clean] 清理编译中间产物..."
    rm -rf "$PROJECT_DIR/dist"
    rm -rf "$BUNDLE_DIR"
    echo "[clean] 完成"
}

echo "=== Chat Vault DMG 构建 ==="
echo "项目目录: $PROJECT_DIR"
echo

# ── 1. 前端构建 ──────────────────────────────────────────────────────────────
echo "[1/3] 构建前端..."
npm run build --silent
echo "      完成"

# ── 2. Tauri 构建 .app ───────────────────────────────────────────────────────
echo "[2/3] 构建 Tauri .app..."
npx tauri build --bundles app 2>&1 | grep -E "^   (Compiling|Finished|Bundling|error)" || true
echo "      完成: $APP_PATH"

# ── 3. 制作 DMG → release/ ───────────────────────────────────────────────────
echo "[3/3] 制作 DMG..."
mkdir -p "$RELEASE_DIR"

VERSION="$(plutil -p "$APP_PATH/Contents/Info.plist" | grep CFBundleShortVersionString | awk -F'"' '{print $4}')"
ARCH="$(uname -m)"
DMG_NAME="Chat-Vault_${VERSION}_${ARCH}.dmg"
DMG_PATH="$RELEASE_DIR/$DMG_NAME"

# ad-hoc 签名，避免 macOS 报 "damaged" 错误
codesign --force --deep --sign - "$APP_PATH" 2>/dev/null && \
    echo "      ad-hoc 签名完成" || \
    echo "      签名跳过（codesign 不可用）"

TMP_DMG_DIR="$(mktemp -d)"
cp -r "$APP_PATH" "$TMP_DMG_DIR/"
ln -s /Applications "$TMP_DMG_DIR/Applications"

hdiutil create \
    -volname "$PRODUCT_NAME" \
    -srcfolder "$TMP_DMG_DIR" \
    -ov \
    -format UDZO \
    "$DMG_PATH" 2>/dev/null

rm -rf "$TMP_DMG_DIR"

# ── 清理中间产物 ──────────────────────────────────────────────────────────────
cleanup

echo
echo "=== 构建完成 ==="
ls -lh "$DMG_PATH"
echo "输出: $DMG_PATH"
