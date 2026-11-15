#!/usr/bin/env bash
#
# Builds and installs figma-font-agent as a systemd user service.
#
#   ./packaging/install.sh              # build, install, start at login
#   ./packaging/install.sh --socket     # build, install, start on first request
#   ./packaging/install.sh --uninstall  # stop and remove
#
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN_DIR="${BIN_DIR:-$HOME/.local/bin}"
UNIT_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user"

SERVICE="figma-font-agent.service"
SOCKET="figma-font-agent.socket"

info() { printf '\033[1m==>\033[0m %s\n' "$*"; }
fail() { printf 'error: %s\n' "$*" >&2; exit 1; }

uninstall() {
    if command -v systemctl >/dev/null 2>&1; then
        systemctl --user disable --now "$SOCKET" 2>/dev/null || true
        systemctl --user disable --now "$SERVICE" 2>/dev/null || true
    fi
    rm -f "$UNIT_DIR/$SERVICE" "$UNIT_DIR/$SOCKET" "$BIN_DIR/figma-font-agent"
    if command -v systemctl >/dev/null 2>&1; then
        systemctl --user daemon-reload || true
    fi
    info "figma-font-agent removed (config in \${XDG_CONFIG_HOME:-~/.config}/figma-font-agent was kept)"
}

mode="service"
case "${1:-}" in
    --socket) mode="socket" ;;
    --uninstall) uninstall; exit 0 ;;
    --help | -h)
        sed -n '3,8p' "$0" | sed 's/^# \{0,1\}//'
        exit 0
        ;;
    "") ;;
    *) fail "unknown option: $1 (try --help)" ;;
esac

command -v cargo >/dev/null 2>&1 || fail "cargo not found; install Rust first: https://rustup.rs"

info "building release binary"
cargo build --release --manifest-path "$ROOT/Cargo.toml"

info "installing binary to $BIN_DIR"
install -Dm755 "$ROOT/target/release/figma-font-agent" "$BIN_DIR/figma-font-agent"

if ! command -v systemctl >/dev/null 2>&1; then
    info "systemctl not found; run $BIN_DIR/figma-font-agent manually"
    exit 0
fi

info "installing systemd user units to $UNIT_DIR"
mkdir -p "$UNIT_DIR"
install -Dm644 "$ROOT/packaging/$SERVICE" "$UNIT_DIR/$SERVICE"

case "$mode" in
    service)
        # The socket and the always-on service compete for the same port.
        systemctl --user disable --now "$SOCKET" 2>/dev/null || true
        rm -f "$UNIT_DIR/$SOCKET"
        systemctl --user daemon-reload
        systemctl --user enable --now "$SERVICE"
        info "status: $(systemctl --user is-active "$SERVICE")"
        ;;
    socket)
        systemctl --user disable --now "$SERVICE" 2>/dev/null || true
        install -Dm644 "$ROOT/packaging/$SOCKET" "$UNIT_DIR/$SOCKET"
        systemctl --user daemon-reload
        systemctl --user enable --now "$SOCKET"
        info "the agent will start on the first request to 127.0.0.1:44950"
        ;;
esac

cat <<EOF

Next steps:
  1. Reload https://www.figma.com and open a file.
  2. Open the font family picker and choose "Fonts on this device".
  3. Allow the browser's "Access other apps and services on this device" prompt.
  4. If Figma does not detect the agent: menu -> Preferences ->
     Permissions and helpers -> Apps on device -> Connect.

Logs:    journalctl --user -u $SERVICE -f
Rescan:  curl -s -X POST http://127.0.0.1:44950/figma/clear-data
EOF
