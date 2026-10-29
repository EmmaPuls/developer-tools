#!/bin/zsh
# Builds ZoomFocus.app in the zoom-watch directory. Launch at Login needs an app bundle.
set -eu

cd "$(dirname "$0")/.."

swift build -c release
BIN=$(swift build -c release --show-bin-path)/ZoomFocus

APP=ZoomFocus.app
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS"
cp "$BIN" "$APP/Contents/MacOS/ZoomFocus"

mkdir -p "$APP/Contents/Resources"
cp Resources/*.shortcut "$APP/Contents/Resources/"

cat > "$APP/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key>
    <string>ZoomFocus</string>
    <key>CFBundleDisplayName</key>
    <string>ZoomFocus</string>
    <key>CFBundleIdentifier</key>
    <string>local.zoomfocus</string>
    <key>CFBundleExecutable</key>
    <string>ZoomFocus</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleShortVersionString</key>
    <string>0.1.0</string>
    <key>CFBundleVersion</key>
    <string>1</string>
    <key>LSMinimumSystemVersion</key>
    <string>14.4</string>
    <key>LSUIElement</key>
    <true/>
</dict>
</plist>
PLIST

codesign --force --sign - "$APP"
echo "Built $PWD/$APP"
