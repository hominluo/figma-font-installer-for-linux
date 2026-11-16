#!/usr/bin/env bash
#
# Builds a single-file Flatpak bundle from an already compiled binary.
#
#   packaging/flatpak/build.sh <binary> <version> <x86_64|aarch64> <output-dir>
#
# Only the `flatpak` command is needed. The binary is static, so nothing is
# compiled against the SDK and the runtime does not have to be installed to
# build: it is only named in the metadata, for whoever installs the bundle.
set -euo pipefail

[[ $# -eq 4 ]] || { echo "usage: $0 <binary> <version> <arch> <output-dir>" >&2; exit 2; }
binary="$1" version="$2" arch="$3" output="$4"

root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)"
app_id="io.github.hominluo.FigmaFontAgent"
runtime_version="26.08"

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
build="$work/build"

mkdir -p "$build/files" "$build/var"
cat >"$build/metadata" <<METADATA
[Application]
name=$app_id
runtime=org.freedesktop.Platform/$arch/$runtime_version
sdk=org.freedesktop.Sdk/$arch/$runtime_version
METADATA

install -Dm755 "$binary" "$build/files/bin/figma-font-agent"
install -Dm644 "$root/packaging/$app_id.desktop" "$build/files/share/applications/$app_id.desktop"
install -Dm644 "$root/packaging/$app_id.svg" "$build/files/share/icons/hicolor/scalable/apps/$app_id.svg"
install -Dm644 "$root/packaging/$app_id.metainfo.xml" "$build/files/share/metainfo/$app_id.metainfo.xml"
install -Dm644 "$root/LICENSE" "$build/files/share/licenses/$app_id/LICENSE"

# The host's fonts are visible to every Flatpak without a permission. The
# network is the only thing to ask for: the agent listens on loopback.
flatpak build-finish "$build" --command=figma-font-agent --share=network

flatpak build-export --arch="$arch" "$work/repo" "$build" stable

mkdir -p "$output"
flatpak build-bundle --arch="$arch" \
    --runtime-repo=https://dl.flathub.org/repo/flathub.flatpakrepo \
    "$work/repo" "$output/figma-font-agent-$version-$arch.flatpak" "$app_id" stable
