#!/usr/bin/env bash
# Builds the Lithe desktop product for Linux from the shared React/Tauri host.
#
# Prerequisites (Fedora package names in parentheses):
#   - WebKitGTK 4.1 and GTK3 dev headers (webkit2gtk4.1-devel, gtk3-devel)
#   - librsvg and AppIndicator dev headers (librsvg2-devel,
#     libayatana-appindicator3-devel)
#   - Base toolchain: rustup, bun, file, patch (gcc-c++, make)
# See https://tauri.app/start/prerequisites/ for the distribution matrix.
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
APP_DIR="$ROOT_DIR/windows/tauri"
BUNDLE_KIND="${1:-deb}"

command -v bun >/dev/null 2>&1 || { echo "bun is required" >&2; exit 1; }
command -v cargo >/dev/null 2>&1 || { echo "cargo is required" >&2; exit 1; }

if ! pkg-config --exists webkit2gtk-4.1 2>/dev/null; then
    echo "webkit2gtk-4.1 development headers are missing." >&2
    echo "Fedora: sudo dnf install webkit2gtk4.1-devel gtk3-devel librsvg2-devel libayatana-appindicator3-devel" >&2
    echo "Debian: sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev libayatana-appindicator3-dev" >&2
    exit 1
fi

cd "$APP_DIR"

bun install --ignore-scripts
bun run typecheck
bun test src/platform src/extensions/vsix

bunx tauri build --bundles "$BUNDLE_KIND"

echo
echo "Bundles written to $APP_DIR/src-tauri/target/release/bundle/"
