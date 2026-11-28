#!/usr/bin/env bash
set -euo pipefail

# Wraps an installed `ironmark` binary in /Applications/Ironmark.app so macOS
# Launch Services (`open file.md` and Finder) can associate .md files with it.

APP_DIR="/Applications/Ironmark.app"
LSREGISTER="/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister"

BIN_PATH="${1:-$(command -v ironmark || true)}"
if [[ -z "${BIN_PATH}" || ! -x "${BIN_PATH}" ]]; then
  echo "Error: 'ironmark' binary not found in PATH." >&2
  echo "Install it first (e.g. 'brew install kevinfra/tap/ironmark') or pass the binary path as an argument:" >&2
  echo "  $0 /path/to/ironmark" >&2
  exit 1
fi

mkdir -p "${APP_DIR}/Contents/MacOS"
cp "${BIN_PATH}" "${APP_DIR}/Contents/MacOS/ironmark"
chmod +x "${APP_DIR}/Contents/MacOS/ironmark"

cat > "${APP_DIR}/Contents/Info.plist" << 'EOF'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key>
  <string>Ironmark</string>
  <key>CFBundleDisplayName</key>
  <string>Ironmark</string>
  <key>CFBundleIdentifier</key>
  <string>com.kevinfra.ironmark</string>
  <key>CFBundleVersion</key>
  <string>0.1.0</string>
  <key>CFBundleShortVersionString</key>
  <string>0.1.0</string>
  <key>CFBundleExecutable</key>
  <string>ironmark</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>NSHighResolutionCapable</key>
  <true/>
  <key>CFBundleDocumentTypes</key>
  <array>
    <dict>
      <key>CFBundleTypeName</key>
      <string>Markdown Document</string>
      <key>CFBundleTypeRole</key>
      <string>Viewer</string>
      <key>LSHandlerRank</key>
      <string>Alternate</string>
      <key>CFBundleTypeExtensions</key>
      <array>
        <string>md</string>
        <string>markdown</string>
      </array>
      <key>LSItemContentTypes</key>
      <array>
        <string>net.daringfireball.markdown</string>
      </array>
    </dict>
  </array>
</dict>
</plist>
EOF

codesign --force --deep -s - "${APP_DIR}"

if [[ -x "${LSREGISTER}" ]]; then
  "${LSREGISTER}" -f "${APP_DIR}"
fi

echo "Installed ${APP_DIR} using ${BIN_PATH}"
echo ""
echo "To set Ironmark as the default app for .md files:"
echo "  - Finder: Right-click any .md file -> Get Info -> Open with -> Ironmark -> Change All..."
echo "  - CLI (via duti): brew install duti && duti -s com.kevinfra.ironmark .md all"
