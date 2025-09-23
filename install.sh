#!/bin/bash
# Quick installation script for devtools

set -e

echo "🔧 Building dt..."
cargo build --release

echo "📦 Installing to ~/.local/bin..."
mkdir -p ~/.local/bin
cp target/release/dt ~/.local/bin/

echo "✅ dt installed successfully!"
echo ""
echo "Make sure ~/.local/bin is in your PATH:"
echo "export PATH=\"\$HOME/.local/bin:\$PATH\""
echo ""
echo "Test with: dt --help"