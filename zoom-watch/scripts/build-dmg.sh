#!/bin/zsh
# Packages ZoomFocus.app into dist/ZoomFocus.dmg, with an Applications shortcut so users can drag it in.
set -eu

cd "$(dirname "$0")/.."

./scripts/build-app.sh

STAGING=$(mktemp -d)
trap 'rm -rf "$STAGING"' EXIT

cp -R ZoomFocus.app "$STAGING/"
ln -s /Applications "$STAGING/Applications"

mkdir -p dist
rm -f dist/ZoomFocus.dmg
hdiutil create -volname "ZoomFocus" -srcfolder "$STAGING" -ov -format UDZO dist/ZoomFocus.dmg

echo "Built $PWD/dist/ZoomFocus.dmg"
