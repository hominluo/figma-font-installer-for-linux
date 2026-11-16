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
- Static builds for x86_64 and aarch64, packaged as `.deb`, `.rpm`, AppImage,
  Flatpak, an Arch package and a plain archive.
- A font you install, remove or disable is there the next time Figma asks.
- TrueType/OpenType collections and variable fonts (named instances appear as
  individual styles).
- Font previews generated as SVG from the glyph outlines.
- Listens on loopback only, answers `figma.com` only, and serves only files
  from its own font index.

## Requirements

- Linux with fontconfig (`fc-list`). Every desktop distribution ships it.
- A Chromium-based browser recent enough to support Local Network Access, or
  Firefox. Chromium is what Figma tests against.
- systemd (optional, for running the agent as a user service).

You do **not** need Rust unless you build from source.

## Install

Every [release](https://github.com/hominluo/figma-font-installer-for-linux/releases/latest)
ships the same static binary in several forms. Pick the one that suits your
system; all of them exist for x86_64 and aarch64 except the Arch package
(x86_64).

| Format | For | File |
| --- | --- | --- |
| [Install script](#any-distribution-install-script) | any distribution | none, it downloads the archive |
| [`.deb`](#debian--ubuntu) | Debian, Ubuntu, Mint, Pop!_OS | `figma-font-agent_<version>-1_amd64.deb` |
| [`.rpm`](#fedora--rhel--opensuse) | Fedora, RHEL, openSUSE | `figma-font-agent-<version>-1.x86_64.rpm` |
| [Arch package](#arch-linux--omarchy) | Arch, Omarchy, Manjaro | `figma-font-agent-<version>-1-x86_64.pkg.tar.zst` |
| [AppImage](#appimage) | trying it without installing | `figma-font-agent-<version>-x86_64.AppImage` |
| [Flatpak](#flatpak) | a sandboxed install | `figma-font-agent-<version>-x86_64.flatpak` |
| [Archive](#archive) | installing by hand, offline | `figma-font-agent-x86_64-unknown-linux-musl.tar.gz` |

Whichever you choose, finish with [Use it with Figma](#use-it-with-figma):
Figma has to be connected to the agent once.

`SHA256SUMS` in the release covers every file. The commands below are meant to
be run in the directory you downloaded the file to; the `*` stands for the
version.

### Any distribution (install script)

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

### Debian / Ubuntu

```bash
sudo apt install ./figma-font-agent_*_amd64.deb   # or _arm64.deb
systemctl --user enable --now figma-font-agent.service
```

The package contains a static binary, so it has no libc or library
dependencies and installs on any Debian or Ubuntu release.

### Fedora / RHEL / openSUSE

```bash
sudo dnf install ./figma-font-agent-*.x86_64.rpm   # or .aarch64.rpm
systemctl --user enable --now figma-font-agent.service
```

The package is not signed. `dnf` installs a local file regardless; with
`zypper` add `--allow-unsigned-rpm`, and with plain `rpm -i` on rpm 6 add
`--nosignature`.

### Arch Linux / Omarchy

```bash
sudo pacman -U ./figma-font-agent-*-x86_64.pkg.tar.zst
systemctl --user enable --now figma-font-agent.service
```

Or build it yourself from the PKGBUILD, which is what the release package is
built from:

```bash
git clone https://github.com/hominluo/figma-font-installer-for-linux
cd figma-font-installer-for-linux/packaging && makepkg -si
```

The package installs the binary to `/usr/bin`, the user units to
`/usr/lib/systemd/user` and a man page. It is not in the AUR.

### AppImage

```bash
chmod +x figma-font-agent-*-x86_64.AppImage
./figma-font-agent-*-x86_64.AppImage              # runs until you stop it
```

The agent has no window: it serves fonts for as long as it runs. To keep it,
let the AppImage install itself as a user service, after which the file is no
longer needed:

```bash
./figma-font-agent-*-x86_64.AppImage --install    # same as the install script
./figma-font-agent-*-x86_64.AppImage --uninstall
```

If your system has no FUSE, add `--appimage-extract-and-run`.

### Flatpak

```bash
flatpak install --user ./figma-font-agent-*-x86_64.flatpak
flatpak run io.github.hominluo.FigmaFontAgent
```

The bundle needs the `org.freedesktop.Platform` runtime from Flathub, which
Flatpak offers to install. To start the agent with your session:

```bash
mkdir -p ~/.config/autostart
cp ~/.local/share/flatpak/exports/share/applications/io.github.hominluo.FigmaFontAgent.desktop ~/.config/autostart/
```

The sandbox has no access to your files. It sees the fonts Flatpak shows every
app: the system's and those in `~/.local/share/fonts`. Its config file lives in
`~/.var/app/io.github.hominluo.FigmaFontAgent/config/figma-font-agent/`, and a
directory named in `font_directories` has to be opened up first with
`flatpak override --user --filesystem=<dir>:ro io.github.hominluo.FigmaFontAgent`.

### Archive

```bash
tar -xzf figma-font-agent-x86_64-unknown-linux-musl.tar.gz
./figma-font-agent/install.sh
```

Run from the unpacked archive, the install script installs the binary next to
it and downloads nothing.

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

The archive and the `.deb`, `.rpm` and Arch packages contain
`figma-font-agent.socket`. To let systemd start the agent on the first Figma
request:

```bash
./install.sh --socket          # or: systemctl --user enable --now figma-font-agent.socket
```

Do not enable the socket and the service at the same time — they compete for
port 44950.

## Use it with Figma

Installing the agent is not the last step: Figma has to be connected to it
once, in the browser. Until then the font picker stays empty, even with the
agent running and your fonts installed.

### Connect Figma (once per browser)

1. Make sure the agent is running:

   ```bash
   curl -s http://127.0.0.1:44950/figma/version
   # {"package":"126.11.9","version":24}
   ```

2. Open any design file on <https://www.figma.com>.
3. Start the connection. Either way works:
   - In the font picker, choose **Installed by you** and click **Use local
     fonts**. A **Use local fonts** dialog opens.
   - Or open the main menu (the Figma logo) → **Preferences** → **Permissions
     and helpers**, and under **Apps on device** click **Show me how to
     connect**.
4. Click **I'm ready to connect** (or **Connect**). Do **not** click
   **Downloads page**: Figma's instructions assume its own font installer or
   desktop app, which do not exist for Linux. The agent takes their place.
5. The browser asks whether figma.com may *"Access other apps and services on
   this device"* (the wording differs between browsers). Click **Allow**. The
   choice is remembered for figma.com, so this happens once.
6. **Apps on device** now shows **Connected**. Open the font picker again and
   your fonts are listed under **Installed by you**, with previews.

You do not need to change your browser's user agent, and **Clipboard access**
or **Microphone access** can stay blocked: they are separate permissions.

To watch the connection happen, follow the agent's log while you click
**I'm ready to connect**. The line `https://www.figma.com is connected` means
the browser got through:

```bash
journalctl --user -u figma-font-agent -f
```

### After that

- Figma reconnects by itself whenever the agent is running. Nothing has to be
  done per file or per session.
- A font you install shows up the next time Figma asks for the font list;
  reload the Figma tab if it is not there yet. See
  [Installing and managing fonts](#installing-and-managing-fonts).
- Figma asks to connect again if you clear figma.com's site data or reset its
  permissions.

### Still empty after connecting?

| What you see | What to do |
| --- | --- |
| The "Use local fonts" dialog keeps coming back | The agent is not reachable. Check `systemctl --user status figma-font-agent` and see [Troubleshooting](#troubleshooting). |
| No browser prompt appeared | Click the icon left of the address bar → **Site settings** → set **Local network access** (or **Apps on device**) to **Allow**, then reload the tab. |
| You clicked **Block** by mistake | Same place: set it back to **Allow** and reload. |
| The agent's log shows nothing when you click connect | An ad blocker or privacy extension is blocking `127.0.0.1`. Allow it on figma.com. |
| Connected, but one font is missing | Check `fc-list \| grep -i 'name'`. If fontconfig does not list it, it is not installed where the desktop looks, or a font manager disabled it. |

## Installing and managing fonts

The agent serves what fontconfig lists, so anything that installs a font for
your desktop installs it for Figma:

- **By hand:** put the font files anywhere under `~/.local/share/fonts`.
- **With a font manager.** [GNOME Fonts](https://apps.gnome.org/FontViewer/)
  (`gnome-font-viewer`) is small, follows the desktop's light or dark theme, and
  installs a font file with one click. [Font Manager](https://fontmanager.github.io)
  (`font-manager`) adds collections and can switch fonts off; a font disabled
  there disappears from Figma too, because the agent honours fontconfig's
  reject rules.
- **With your package manager**, for fonts your distribution packages.

A change is picked up the next time Figma asks for the font list. If a new
font is not in the picker yet, reload the Figma tab.

**Omarchy** has no font manager of its own: `omarchy font list|set` and
`omarchy install font` only choose the monospace font of the terminal and the
interface. Install one with
`omarchy install app "GNOME Fonts" gnome-font-viewer`.

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

The first time the browser gets through, the agent's log says
`https://www.figma.com is connected`:

```bash
journalctl --user -u figma-font-agent -f
```

To force a rescan instead of waiting for Figma to ask:

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
| `rescan_interval_secs` | `300` | how often to rescan in the background; `0` turns off all automatic rescans, including the one before each font list |
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

The agent asks fontconfig for every font file (`fc-list --format=%{file}`).
fontconfig notices new files in its directories by itself, so a font dropped
into `~/.local/share/fonts` shows up without running `fc-cache`, and it leaves
out fonts that a font manager has disabled. Each file is parsed with
[`ttf-parser`](https://github.com/harfbuzz/ttf-parser), and an in-memory index
of families, styles, PostScript names, weights, widths and variation axes is
built. `/figma/font-files` returns that index; `/figma/font-file` streams a
file the index knows about; `/figma/font-preview` renders the family name in
the requested face as SVG.

The index is refreshed whenever Figma asks for the font list, and on a
background timer. Rescans are incremental: a file whose mtime and size did not
change keeps its parsed faces, so a rescan of a typical desktop font library
takes tens of milliseconds, and a rescan that finds nothing new logs nothing.

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
  the ports being served. This defeats DNS-rebinding attacks, where a hostile
  page resolves a domain it controls to `127.0.0.1` and talks to your services.
- **Only Figma's origin is answered.** A request with any `Origin` other than
  `https://www.figma.com` (or `https://figma.com`) gets `403` before it reaches
  a handler, so another site can neither read a response nor trigger a rescan.
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
- **It serves fonts, it does not install them.** See
  [Installing and managing fonts](#installing-and-managing-fonts).
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

Check that the desktop itself sees it, then make the agent look again:

```bash
fc-list | grep -i 'part of the font name'
curl -s -X POST http://127.0.0.1:44950/figma/clear-data
```

If `fc-list` does not show the font, it is not in a directory fontconfig reads,
or a font manager has it disabled. Directories fontconfig does not know can be
added with `font_directories`.

**The browser blocks the request**

Chrome's *Local Network Access* feature requires an explicit grant. If you
dismissed or blocked the prompt, open the site settings from the icon left of
the address bar and allow **Local network access**, or reset figma.com
entirely in `chrome://settings/content/all` (search for `figma.com`, remove
the site), then reload Figma and connect again as in
[Connect Figma](#connect-figma-once-per-browser).

## Uninstall

```bash
# installed with install.sh, the archive or the AppImage's --install
curl -fsSL https://raw.githubusercontent.com/hominluo/figma-font-installer-for-linux/main/install.sh | bash -s -- --uninstall

# installed from a package
sudo apt remove figma-font-agent                                # Debian/Ubuntu
sudo dnf remove figma-font-agent                                # Fedora/RHEL
sudo pacman -R figma-font-agent                                 # Arch/Omarchy
flatpak uninstall --user io.github.hominluo.FigmaFontAgent      # Flatpak
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
| `src/server.rs` | routes, origin and `Host` checks, rescans |

Maintainer notes — release procedure, decisions, known gaps — live in
[`docs/handoff.md`](docs/handoff.md).

## Releasing

Releases are built by GitHub Actions when a `v*` tag is pushed:

```bash
# bump Cargo.toml, packaging/PKGBUILD, docs/figma-font-agent.1, the metainfo
# file and CHANGELOG.md, then check the pipeline without publishing
gh workflow run release.yml

git tag vX.Y.Z && git push origin vX.Y.Z
```

The workflow builds one static binary per architecture and packages it as:

- `figma-font-agent-<target>.tar.gz`, with the install script inside
- `figma-font-agent_<version>-1_<arch>.deb`
- `figma-font-agent-<version>-1.<arch>.rpm`
- `figma-font-agent-<version>-<arch>.AppImage`
- `figma-font-agent-<version>-<arch>.flatpak`
- `figma-font-agent-<version>-1-x86_64.pkg.tar.zst`, built from the PKGBUILD
- `SHA256SUMS` over all of the above

Each package is installed or run once in the workflow before it is uploaded,
and `install.sh` verifies the checksum before installing.

## Credits

- Protocol recovered from Figma's own `InstallFigmaAgent` binaries, using the
  [REA](https://github.com/morluto/rea) reverse-engineering methodology.
- [`neetly/figma-agent-linux`](https://github.com/neetly/figma-agent-linux)
  proved the protocol works from a third-party implementation and was used to
  cross-check the recovered route table.
- Font parsing by [`ttf-parser`](https://github.com/harfbuzz/ttf-parser).

## License

MIT — see [LICENSE](LICENSE). Not affiliated with or endorsed by Figma, Inc.
