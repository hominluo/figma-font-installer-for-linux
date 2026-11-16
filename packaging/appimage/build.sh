#!/usr/bin/env bash
#
# Builds the AppImage from an already compiled binary.
#
#   packaging/appimage/build.sh <binary> <version> <x86_64|aarch64> <output-dir>
#
# Needs `appimagetool` on PATH, or its location in $APPIMAGETOOL.
set -euo pipefail

[[ $# -eq 4 ]] || { echo "usage: $0 <binary> <version> <arch> <output-dir>" >&2; exit 2; }
binary="$1" version="$2" arch="$3" output="$4"

root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)"
app_id="io.github.hominluo.FigmaFontAgent"
tool="${APPIMAGETOOL:-appimagetool}"

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
appdir="$work/AppDir"
payload="$appdir/usr/lib/figma-font-agent"

# The payload is laid out like a release archive, so the bundled installer can
# install from it without downloading anything.
install -Dm755 "$binary" "$payload/figma-font-agent"
install -Dm755 "$root/install.sh" "$payload/install.sh"
install -Dm644 "$root/packaging/figma-font-agent.service" "$payload/figma-font-agent.service"
install -Dm644 "$root/packaging/figma-font-agent.socket" "$payload/figma-font-agent.socket"
install -Dm644 "$root/docs/figma-font-agent.1" "$payload/figma-font-agent.1"

install -Dm755 "$root/packaging/appimage/AppRun" "$appdir/AppRun"
install -Dm644 "$root/packaging/$app_id.desktop" "$appdir/$app_id.desktop"
install -Dm644 "$root/packaging/$app_id.desktop" "$appdir/usr/share/applications/$app_id.desktop"
install -Dm644 "$root/packaging/$app_id.svg" "$appdir/$app_id.svg"
install -Dm644 "$root/packaging/$app_id.svg" "$appdir/usr/share/icons/hicolor/scalable/apps/$app_id.svg"
install -Dm644 "$root/packaging/$app_id.metainfo.xml" "$appdir/usr/share/metainfo/$app_id.metainfo.xml"
install -Dm644 "$root/LICENSE" "$appdir/usr/share/licenses/figma-font-agent/LICENSE"
ln -s "$app_id.svg" "$appdir/.DirIcon"

mkdir -p "$output"
ARCH="$arch" "$tool" --no-appstream "$appdir" "$output/figma-font-agent-$version-$arch.AppImage"
