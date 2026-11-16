#!/usr/bin/env bash
#
# Installer for figma-font-agent.
#
#   ./install.sh                     # download the latest release, install, start
#                                    # (next to a release binary: install that one)
#   ./install.sh --from-source       # build this checkout and install it
#   ./install.sh --socket            # start on demand (systemd socket activation)
#   ./install.sh --version v0.2.0    # install a specific release
#   ./install.sh --prefix /usr/local # install somewhere else
#   ./install.sh --uninstall         # stop and remove
#
# From the internet:
#   curl -fsSL https://raw.githubusercontent.com/hominluo/figma-font-installer-for-linux/main/install.sh | bash
#
set -euo pipefail

REPO="${REPO:-hominluo/figma-font-installer-for-linux}"
PREFIX="${PREFIX:-$HOME/.local}"
BIN_DIR="${BIN_DIR:-$PREFIX/bin}"
MAN_DIR="${MAN_DIR:-$PREFIX/share/man/man1}"
UNIT_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user"

SERVICE="figma-font-agent.service"
SOCKET="figma-font-agent.socket"

MODE="service"
FROM_SOURCE=0
VERSION=""
ACTION="install"

TMP_DIR=""
# Must not fail: under `set -e` a failing EXIT trap replaces the exit status.
cleanup() { if [[ -n "$TMP_DIR" ]]; then rm -rf "$TMP_DIR"; fi; }
trap cleanup EXIT

info() { printf '\033[1m==>\033[0m %s\n' "$*"; }
warn() { printf '\033[33mwarning:\033[0m %s\n' "$*" >&2; }
die() { printf '\033[31merror:\033[0m %s\n' "$*" >&2; exit 1; }

usage() {
    cat <<'EOF'
Usage: install.sh [options]

  (no options)        download the latest release, install and start it;
                      run from an unpacked release archive, install its binary
  --from-source       build this checkout with cargo and install it
  --socket            start on demand with systemd socket activation
  --version <tag>     install a specific release, e.g. --version v0.2.0
  --prefix <dir>      install under <dir> (default: ~/.local)
  --uninstall         stop the agent and remove its files
  -h, --help          show this help
EOF
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --from-source) FROM_SOURCE=1 ;;
        --socket) MODE="socket" ;;
        --version)
            [[ $# -ge 2 ]] || die "--version needs a release tag"
            VERSION="$2"
            shift
            ;;
        --prefix)
            [[ $# -ge 2 ]] || die "--prefix needs a directory"
            PREFIX="$2"
            BIN_DIR="$2/bin"
            MAN_DIR="$2/share/man/man1"
            shift
            ;;
        --uninstall) ACTION="uninstall" ;;
        -h | --help)
            usage
            exit 0
            ;;
        *) die "unknown option: $1 (try --help)" ;;
    esac
    shift
done

case "$BIN_DIR" in
    *'|'* | *'&'* | *'\'*) die "BIN_DIR must not contain |, & or \\" ;;
esac

# Where this script lives, when it was run from a file. Piped into bash
# (`curl ... | bash`) it lives nowhere, and the current directory says nothing
# about what to install.
SCRIPT_DIR=""
if [[ -n "${BASH_SOURCE[0]:-}" && -f "${BASH_SOURCE[0]}" ]]; then
    SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
fi
ROOT="${SCRIPT_DIR:-$PWD}"

is_checkout() {
    [[ -f "$1/Cargo.toml" ]] && grep -q '^name = "figma-font-agent"' "$1/Cargo.toml"
}

# A user manager that answers, not just a `systemctl` binary: containers, WSL
# and `su` sessions have the latter without the former.
have_systemd() {
    command -v systemctl >/dev/null 2>&1 \
        && [[ -n "${XDG_RUNTIME_DIR:-}" ]] \
        && systemctl --user show-environment >/dev/null 2>&1
}

systemctl_user() {
    systemctl --user "$@" 2>/dev/null
}

# ---------------------------------------------------------------------------
# Uninstall
# ---------------------------------------------------------------------------

if [[ "$ACTION" == "uninstall" ]]; then
    if have_systemd; then
        systemctl_user disable --now "$SOCKET" || true
        systemctl_user disable --now "$SERVICE" || true
    fi
    rm -f "$BIN_DIR/figma-font-agent" "$UNIT_DIR/$SERVICE" "$UNIT_DIR/$SOCKET" \
        "$MAN_DIR/figma-font-agent.1"
    if have_systemd; then
        systemctl_user daemon-reload || true
    fi
    info "figma-font-agent removed; its configuration in ${XDG_CONFIG_HOME:-$HOME/.config}/figma-font-agent was kept"
    exit 0
fi

# ---------------------------------------------------------------------------
# Install
# ---------------------------------------------------------------------------

detect_target() {
    case "$(uname -m)" in
        x86_64 | amd64) echo "x86_64-unknown-linux-musl" ;;
        aarch64 | arm64) echo "aarch64-unknown-linux-musl" ;;
        *) die "unsupported architecture: $(uname -m)" ;;
    esac
}

download() {
    if command -v curl >/dev/null 2>&1; then
        curl -fsSL --retry 2 --connect-timeout 15 "$1" -o "$2"
    elif command -v wget >/dev/null 2>&1; then
        wget -q -O "$2" "$1"
    else
        die "neither curl nor wget is available"
    fi
}

sha256_check() {
    local directory="$1" asset="$2"

    if command -v sha256sum >/dev/null 2>&1; then
        (cd "$directory" && grep -F "  $asset" SHA256SUMS | sha256sum -c -)
    elif command -v shasum >/dev/null 2>&1; then
        (cd "$directory" && grep -F "  $asset" SHA256SUMS | shasum -a 256 -c -)
    else
        warn "no sha256sum/shasum available, skipping checksum verification"
    fi
}

build_from_source() {
    local root="$1"

    is_checkout "$root" || die "--from-source needs a checkout of this repository (run ./install.sh inside it)"
    command -v cargo >/dev/null 2>&1 || die "cargo not found; install Rust from https://rustup.rs first"

    info "building from source in $root"
    (cd "$root" && cargo build --release --locked)

    SOURCE_BIN="$root/target/release/figma-font-agent"
    SOURCE_MAN="$root/docs/figma-font-agent.1"
    SOURCE_UNIT_DIR="$root/packaging"
}

# A release archive (and the AppImage) ships this script next to the binary it
# belongs to; installing that one needs no network.
use_bundled_release() {
    [[ -n "$SCRIPT_DIR" && -z "$VERSION" ]] || return 1
    [[ -f "$SCRIPT_DIR/figma-font-agent" && -x "$SCRIPT_DIR/figma-font-agent" ]] || return 1
    [[ -f "$SCRIPT_DIR/$SERVICE" ]] || return 1

    info "installing the release in $SCRIPT_DIR"
    SOURCE_BIN="$SCRIPT_DIR/figma-font-agent"
    SOURCE_MAN="$SCRIPT_DIR/figma-font-agent.1"
    SOURCE_UNIT_DIR="$SCRIPT_DIR"
}

install_release() {
    local target asset base

    target="$(detect_target)"
    asset="figma-font-agent-$target.tar.gz"

    if [[ -n "$VERSION" ]]; then
        [[ "$VERSION" == v* ]] || VERSION="v$VERSION"
        base="https://github.com/$REPO/releases/download/$VERSION"
    else
        base="https://github.com/$REPO/releases/latest/download"
    fi

    TMP_DIR="$(mktemp -d)"

    info "downloading $asset"
    if ! download "$base/$asset" "$TMP_DIR/$asset" 2>/dev/null \
        || ! download "$base/SHA256SUMS" "$TMP_DIR/SHA256SUMS" 2>/dev/null; then
        return 1
    fi

    sha256_check "$TMP_DIR" "$asset" || die "checksum verification failed for $asset"

    tar -xzf "$TMP_DIR/$asset" -C "$TMP_DIR"

    # Archives keep a top-level directory; be tolerant of both layouts.
    SOURCE_BIN="$(find "$TMP_DIR" -maxdepth 2 -type f -name figma-font-agent -perm -u+x | head -n 1)"
    [[ -n "$SOURCE_BIN" ]] || die "$asset does not contain the figma-font-agent binary"

    SOURCE_MAN="$(dirname "$SOURCE_BIN")/figma-font-agent.1"
    SOURCE_UNIT_DIR="$(dirname "$SOURCE_BIN")"

    "$SOURCE_BIN" --version >/dev/null 2>&1 || die "the downloaded binary does not run on this system"
}

SOURCE_BIN=""
SOURCE_MAN=""
SOURCE_UNIT_DIR=""

if [[ "$FROM_SOURCE" == "1" ]]; then
    build_from_source "$ROOT"
elif use_bundled_release; then
    :
elif ! install_release; then
    if is_checkout "$ROOT"; then
        warn "no release asset available for $(detect_target); falling back to a source build"
        build_from_source "$ROOT"
    else
        die "could not download a release for $(detect_target); install from source instead: git clone https://github.com/$REPO"
    fi
fi

[[ -n "$SOURCE_BIN" && -x "$SOURCE_BIN" ]] || die "no binary to install"
[[ -f "$SOURCE_UNIT_DIR/$SERVICE" ]] || die "the archive does not contain $SERVICE"

info "installing to $BIN_DIR"
install -Dm755 "$SOURCE_BIN" "$BIN_DIR/figma-font-agent"

if [[ -f "$SOURCE_MAN" ]]; then
    install -Dm644 "$SOURCE_MAN" "$MAN_DIR/figma-font-agent.1"
fi

if ! have_systemd; then
    info "systemd user session not detected; start the agent manually:"
    printf '\n    %s\n\n' "$BIN_DIR/figma-font-agent"
    exit 0
fi

info "installing systemd user units to $UNIT_DIR"
mkdir -p "$UNIT_DIR"

# The packaged unit points at /usr/bin; rewrite it for this prefix.
sed "s|^ExecStart=/usr/bin/figma-font-agent|ExecStart=$BIN_DIR/figma-font-agent|" \
    "$SOURCE_UNIT_DIR/$SERVICE" >"$UNIT_DIR/$SERVICE"

systemctl_user daemon-reload || die "systemctl --user daemon-reload failed"
systemctl_user disable --now "$SOCKET" || true
rm -f "$UNIT_DIR/$SOCKET"

if [[ "$MODE" == "socket" ]]; then
    cp "$SOURCE_UNIT_DIR/$SOCKET" "$UNIT_DIR/$SOCKET"
    systemctl_user disable --now "$SERVICE" || true
    systemctl_user daemon-reload || die "systemctl --user daemon-reload failed"
    systemctl_user enable --now "$SOCKET" || die "could not enable $SOCKET"
    info "the agent will start on the first request to 127.0.0.1:44950"
else
    systemctl_user enable "$SERVICE" || die "could not enable $SERVICE"
    systemctl_user restart "$SERVICE" || die "could not start $SERVICE"
    info "status: $(systemctl --user is-active "$SERVICE" 2>/dev/null || echo unknown)"
fi

cat <<EOF

figma-font-agent is installed.

Next steps:
  1. Reload https://www.figma.com and open a file.
  2. Open the font family picker and choose "Fonts on this device".
  3. Allow the browser's "Access other apps and services on this device" prompt.
  4. If Figma does not detect the agent: menu -> Preferences ->
     Permissions and helpers -> Apps on device -> Connect.

Logs:    journalctl --user -u $SERVICE -f
Rescan:  curl -s -X POST http://127.0.0.1:44950/figma/clear-data
Remove:  curl -fsSL https://raw.githubusercontent.com/$REPO/main/install.sh | bash -s -- --uninstall
EOF
