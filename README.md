# figma-font-installer-for-linux

[![CI](https://github.com/hominluo/figma-font-installer-for-linux/actions/workflows/ci.yml/badge.svg)](https://github.com/hominluo/figma-font-installer-for-linux/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/hominluo/figma-font-installer-for-linux?display_name=tag)](https://github.com/hominluo/figma-font-installer-for-linux/releases/latest)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

Use the fonts installed on your Linux machine in **Figma for the web**.

Figma's local-font support normally comes from the "Figma Agent" helper that
Figma ships only for macOS and Windows. This project is an independent,
open-source reimplementation of that helper's local HTTP protocol, so the
browser build of Figma can list and render your system fonts on Linux.

```
Figma (browser)  ──►  http://127.0.0.1:44950/figma/*  ──►  figma-font-agent  ──►  fontconfig
```

- One small binary, no HarfBuzz, no telemetry, no auto-updater.
- Static builds for x86_64 and aarch64 — works on any distribution.
- TrueType/OpenType collections and variable fonts (named instances appear as
  individual styles).
- Font previews generated as SVG from the glyph outlines.
- Listens on loopback only, validates the `Host` header, and serves only files
  from its own font index.

## Requirements

- Linux with fontconfig (`fc-list`). Every desktop distribution ships it.
- A Chromium-based browser recent enough to support Local Network Access, or
  Firefox. Chromium is what Figma tests against.
- systemd (optional, for running the agent as a user service).

You do **not** need Rust unless you build from source.

## Install

### Any distribution (prebuilt binary)

```bash
curl -fsSL https://raw.githubusercontent.com/hominluo/figma-font-installer-for-linux/main/install.sh | bash
```

This downloads the latest static release for your architecture, verifies its
SHA-256 checksum, installs it to `~/.local/bin`, installs a systemd **user**
unit and starts the agent. Nothing needs root.

Useful flags (pass them with `bash -s -- <flag>` when piping):

```bash
# start on the first Figma request instead of at login
curl -fsSL …/install.sh | bash -s -- --socket

# install to /usr/local from a checkout instead (needs write access)
./install.sh --prefix /usr/local

# remove the agent again, keeping the config file
./install.sh --uninstall
```

### Arch Linux

```bash
git clone https://github.com/hominluo/figma-font-installer-for-linux
cd figma-font-installer-for-linux
cd packaging && makepkg -si
systemctl --user enable --now figma-font-agent.service
```

The package installs the binary to `/usr/bin`, the user units to
`/usr/lib/systemd/user` and a man page. The PKGBUILD is not in the AUR yet.

### Debian / Ubuntu

Download the `.deb` from the [latest release](https://github.com/hominluo/figma-font-installer-for-linux/releases/latest)
and install it:

```bash
sudo apt install ./figma-font-agent_0.1.1-1_amd64.deb   # or _arm64.deb
systemctl --user enable --now figma-font-agent.service
```

The package contains a static binary, so it has no libc or library
dependencies and installs on any Debian or Ubuntu release.

### From source

```bash
git clone https://github.com/hominluo/figma-font-installer-for-linux
cd figma-font-installer-for-linux
./install.sh --from-source
```

Or build and run it without installing anything:

```bash
cargo build --release
./target/release/figma-font-agent
```

### On-demand start (socket activation)

The release archive and every package contain `figma-font-agent.socket`. To let
systemd start the agent on the first Figma request:

```bash
./install.sh --socket          # or: systemctl --user enable --now figma-font-agent.socket
```

Do not enable the socket and the service at the same time — they compete for
port 44950.

## Use it with Figma

1. Open or reload <https://www.figma.com> (or the Figma desktop web app).
2. Open any file, click the font family picker and choose **Fonts on this
   device** / **Local fonts**.
3. When the browser asks to *"Access other apps and services on this device"*,
   allow it. Figma stores the grant per origin, so this happens once.
4. If Figma does not detect the agent, go to the main menu → **Preferences** →
   **Permissions and helpers** → **Apps on device** and click **Connect**.

Your installed fonts now show up in the picker, with previews.

## Verify it works

```bash
# protocol version the agent reports
curl -s http://127.0.0.1:44950/figma/version
# {"package":"126.11.9","version":24}

# how many files/faces were indexed
curl -s http://127.0.0.1:44950/figma/font-files \
  | python3 -c 'import json,sys; d=json.load(sys.stdin); print(len(d["fontFiles"]), "files")'

# the CORS preflight a browser sends
curl -s -o /dev/null -w '%{http_code}\n' -X OPTIONS \
  http://127.0.0.1:44950/figma/font-files \
  -H 'Origin: https://www.figma.com' \
  -H 'Access-Control-Request-Method: GET' \
  -H 'Access-Control-Request-Private-Network: true'
# 204
```

If the agent sees the browser connect, its log shows a `/figma/font-files`
request:

```bash
journalctl --user -u figma-font-agent -f
```

After installing a font, refresh the index immediately instead of waiting for
the next scan:

```bash
curl -s -X POST http://127.0.0.1:44950/figma/clear-data
```

## Configuration

The agent reads `$XDG_CONFIG_HOME/figma-font-agent/config.json`
(`~/.config/figma-font-agent/config.json` by default). Create a starter file
with:

```bash
figma-font-agent --init-config
```

| Key | Default | Meaning |
| --- | --- | --- |
| `package` | `"126.11.9"` | `package` string reported by `/figma/version` |
| `version` | `24` | numeric protocol version reported by `/figma/version` |
| `ports` | `[44950, 44960]` | loopback ports to bind; 44950 is what Figma probes first |
| `font_directories` | `[]` | extra directories to scan in addition to fontconfig |
| `rescan_interval_secs` | `300` | how often to look for new/changed fonts, `0` disables |
| `enable_preview` | `true` | serve `/figma/font-preview` |

Command-line options override the file:

```
-c, --config <PATH>   Config file to read
    --init-config     Write an example config and exit
-p, --port <PORT>     Listen on <PORT>; repeat for several ports
    --no-preview      Disable font previews
    --print-index     Print the /figma/font-files JSON payload and exit
-v, --verbose         Debug logging
```

The reported `package`/`version` are the values recovered from the macOS agent
release `126.11.9`. If a future Figma client insists on a newer pair, bump them
in the config file — no rebuild needed. See [`man figma-font-agent`](docs/figma-font-agent.1)
for the full reference.

## How it works

At startup the agent asks fontconfig for every font file (`fc-list
--format=%{file}`) and also walks `~/.local/share/fonts` and `~/.fonts`, so a
font dropped into either directory shows up without running `fc-cache` first.
Each file is parsed with [`ttf-parser`](https://github.com/harfbuzz/ttf-parser),
and an in-memory index of families, styles, PostScript names, weights, widths
and variation axes is built. `/figma/font-files` returns that index;
`/figma/font-file` streams a file the index knows about; `/figma/font-preview`
renders the family name in the requested face as SVG.

Rescans are incremental: a file whose mtime and size did not change keeps its
parsed faces, so a rescan of a typical desktop font library takes tens of
milliseconds and a quiet rescan logs nothing.

A preview generated by the agent (`/figma/font-preview` for Adwaita Sans Bold
Italic):

![Example font preview](docs/preview-example.png)

The full wire format, including every endpoint and JSON field, is documented in
[`docs/protocol.md`](docs/protocol.md). How it was recovered from the official
binaries is documented in
[`docs/reverse-engineering.md`](docs/reverse-engineering.md).

## Security model

The agent is a loopback HTTP service, so it is deliberately conservative:

- **Binds only to `127.0.0.1`.** Nothing is reachable from the network.
- **Only indexed files are served.** `/figma/font-file?file=/etc/passwd` returns
  404: the `file` parameter is a key into the scan index, not a path to open.
- **`Host` header is validated** against `localhost`, `127.0.0.1` and `[::1]` on
  the configured ports. This defeats DNS-rebinding attacks, where a hostile page
  resolves a domain it controls to `127.0.0.1` and talks to your services.
- **CORS is restricted** to `https://www.figma.com` (and `https://figma.com`).
- The systemd unit runs with `NoNewPrivileges`, `PrivateDevices`,
  `ProtectSystem=full`, `ProtectHome=read-only`, a restricted set of address
  families and no kernel-tunable or module access.

Unlike the official agent, this one has no telemetry, no crash reporting, no
self-updater and no desktop-app integration.

To inspect what is exposed without starting the service:

```bash
figma-font-agent --print-index | jq '.fontFiles | keys[0:5]'
```

## Limitations

- **Preview shape, not paragraph layout.** Previews ignore kerning and
  ligatures and always use the family name as the sample text, which matches
  what Figma shows in its font picker.
- **Variable fonts:** named instances are listed, but the `HVAR` advance-width
  deltas are not applied in previews. Outlines do use the instance's
  coordinates.
- **No font installation.** This serves fonts already installed on the system;
  install fonts the usual way (`~/.local/share/fonts` + `fc-cache -f`, your
  package manager, or Omarchy's `omarchy install font`).
- **No spelling service.** The official agent also exposes `/spelling/*` for
  Figma's spellchecker; it is not implemented.
- **Variable-font sliders:** Figma shows one entry per named instance rather
  than free-form axis sliders.
- Figma may ask to connect again after a browser restart if you clear site data
  or revoke the local-network permission.

## Troubleshooting

**Figma does not see the agent**

```bash
systemctl --user status figma-font-agent
journalctl --user -u figma-font-agent -n 50
ss -tlnp | grep -E '44950|44960'
```

Look for `listening on http://127.0.0.1:44950` in the log. If the port is taken,
another font agent (the official one under Wine, or `figma-agent-linux`) is
already running; stop it first.

**Fonts appear but previews are missing**

The preview endpoint returns 404 when the font's cmap cannot render its own
family name, or when `enable_preview` is false. Check directly:

```bash
curl -s -o /tmp/p.svg -w '%{http_code} %{content_type}\n' --get \
  http://127.0.0.1:44950/figma/font-preview \
  --data-urlencode 'file=/usr/share/fonts/Adwaita/AdwaitaMono-Bold.ttf' \
  --data-urlencode 'family=Adwaita Mono' --data-urlencode 'style=Bold' \
  --data-urlencode 'postscript=Adwaita-Mono-Bold' --data-urlencode 'font_size=16'
```

**A newly installed font does not show up**

```bash
fc-cache -f && curl -s -X POST http://127.0.0.1:44950/figma/clear-data
```

**The browser blocks the request**

Chrome's *Local Network Access* feature requires an explicit grant. If you
dismissed the prompt, reset it in `chrome://settings/content/all` → search for
`figma.com` → remove the site, then reload Figma.

**Omarchy users**

Omarchy's font tooling lives in `/usr/share/omarchy/bin/omarchy-font-*` and is
driven by `omarchy font list|set|current|cjk`. It only *lists* monospace
families, because it configures the terminal/UI font — it is not a general font
manager, and Figma is unaffected by which one you select there. Fonts installed
with `omarchy install font <name> <package> <family>` are regular pacman
packages under `/usr/share/fonts`, so the agent picks them up on the next scan;
run the `clear-data` command above to see them immediately.

## Uninstall

```bash
# installed with install.sh
curl -fsSL https://raw.githubusercontent.com/hominluo/figma-font-installer-for-linux/main/install.sh | bash -s -- --uninstall

# installed from a package
sudo pacman -R figma-font-agent      # Arch
sudo apt remove figma-font-agent     # Debian/Ubuntu
```

The configuration in `~/.config/figma-font-agent` is never deleted for you.

## Development

```bash
cargo test
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo run -- --print-index | head -40   # inspect what Figma would receive
cargo run -- -v                          # run with request logging
```

The code is deliberately small:

| File | Purpose |
| --- | --- |
| `src/main.rs` | CLI, socket activation, startup |
| `src/config.rs` | JSON config + defaults |
| `src/fonts.rs` | discovery, parsing, index, named instances |
| `src/proto.rs` | wire types and payload construction |
| `src/preview.rs` | glyph outlines → SVG |
| `src/server.rs` | routes, CORS/PNA, `Host` validation |

## Releasing

Releases are built by GitHub Actions when a `v*` tag is pushed:

```bash
# bump Cargo.toml, packaging/PKGBUILD, docs/figma-font-agent.1 and CHANGELOG.md
git tag v0.1.1 && git push origin v0.1.1
```

The workflow produces:

- `figma-font-agent-x86_64-unknown-linux-musl.tar.gz` (static)
- `figma-font-agent-aarch64-unknown-linux-musl.tar.gz` (static)
- `figma-font-agent_<version>-1_<arch>.deb` (static, amd64 and arm64)
- `SHA256SUMS` over all of the above

`install.sh` verifies the checksum before installing. To build the `.deb`
locally: `cargo install cargo-deb && cargo deb`.

## Credits

- Protocol recovered from Figma's own `InstallFigmaAgent` binaries, using the
  [REA](https://github.com/morluto/rea) reverse-engineering methodology.
- [`neetly/figma-agent-linux`](https://github.com/neetly/figma-agent-linux)
  proved the protocol works from a third-party implementation and was used to
  cross-check the recovered route table.
- Font parsing by [`ttf-parser`](https://github.com/harfbuzz/ttf-parser).

## License

MIT — see [LICENSE](LICENSE). Not affiliated with or endorsed by Figma, Inc.
