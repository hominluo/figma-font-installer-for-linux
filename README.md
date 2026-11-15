# figma-font-installer-for-linux

Use the fonts installed on your Linux machine in **Figma for the web**.

Figma's local-font support normally comes from the "Figma Agent" helper that
Figma ships only for macOS and Windows. This project is an independent,
open-source reimplementation of that helper's local HTTP protocol, so the
browser build of Figma can list and render your system fonts on Linux.

```
Figma (browser)  ──►  http://127.0.0.1:44950/figma/*  ──►  figma-font-agent  ──►  fontconfig
```

- A single Rust binary, no HarfBuzz, no telemetry, no auto-updater.
- Reads fonts from fontconfig (`fc-list`) and serves them from `127.0.0.1` only.
- Handles TrueType/OpenType collections and variable fonts (named instances are
  exposed as individual styles).
- Font previews are generated as SVG from the glyph outlines.

## Requirements

- Linux with fontconfig (`fc-list`) — installed by default on virtually every
  desktop distribution.
- To build: Rust 1.75+ and a C toolchain.
- A Chromium-based browser recent enough to support Local Network Access, or
  Firefox. Chromium is what Figma tests against; other engines are hit and miss
  with loopback permission prompts.

## Quick start

```bash
git clone https://github.com/hominluo/figma-font-installer-for-linux
cd figma-font-installer-for-linux
./packaging/install.sh
```

`install.sh` builds the release binary, installs it to `~/.local/bin`, installs
a systemd **user** unit and starts it. Then:

1. Open or reload <https://www.figma.com> (or the Figma desktop web app).
2. Open any file and click the font family picker, then choose
   **Fonts on this device** / **Local fonts**.
3. When the browser asks for permission to *"Access other apps and services on
   this device"*, allow it. Figma stores the grant per origin, so this only
   happens once.
4. In Figma, go to the main menu → **Preferences** → **Permissions and
   helpers** → **Apps on device** and click **Connect** if the helper is not
   detected automatically.

Your installed fonts should now appear in the font picker, with previews.

### Without systemd

```bash
cargo build --release
./target/release/figma-font-agent
```

### On-demand start (socket activation)

The repository ships both a service and a socket unit. To let systemd start the
agent on the first Figma request instead of running it at login:

```bash
install -Dm644 packaging/figma-font-agent.socket ~/.config/systemd/user/figma-font-agent.socket
systemctl --user daemon-reload
systemctl --user enable --now figma-font-agent.socket
```

## Verify it works

```bash
# Protocol version the agent reports
curl -s http://127.0.0.1:44950/figma/version
# {"package":"126.11.9","version":24}

# Number of files/faces the agent indexed
curl -s http://127.0.0.1:44950/figma/font-files \
  | python3 -c 'import json,sys; d=json.load(sys.stdin); print(len(d["fontFiles"]), "files")'

# CORS preflight, exactly what the browser sends
curl -s -o /dev/null -w '%{http_code}\n' -X OPTIONS \
  http://127.0.0.1:44950/figma/font-files \
  -H 'Origin: https://www.figma.com' \
  -H 'Access-Control-Request-Method: GET' \
  -H 'Access-Control-Request-Private-Network: true'
# 204
```

If fonts are installed while the agent is running, it picks them up on its next
scan (every 5 minutes by default). To force a rescan immediately:

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
| `package` | `"126.11.9"` | `package` string reported by `/figma/version`; must match the Figma client's expectation |
| `version` | `24` | numeric protocol version reported by `/figma/version` |
| `ports` | `[44950, 44960]` | loopback ports to bind; 44950 is the one Figma probes first |
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
in the config file — no rebuild needed.

## How it works

At startup the agent asks fontconfig for every font file (`fc-list
--format=%{file}`) and also walks `~/.local/share/fonts` and `~/.fonts`, so a
font dropped into either directory shows up without running `fc-cache` first.
Each file is parsed with [`ttf-parser`](https://github.com/harfbuzz/ttf-parser),
and keeps an in-memory index of families, styles, PostScript names, weights,
widths and variation axes. `/figma/font-files` returns that index; `/figma/font-file`
streams a file the index knows about; `/figma/font-preview` renders the family
name in the requested face as SVG.

Rescans are incremental: a file whose mtime and size did not change keeps its
parsed faces, so a rescan of a typical desktop font library takes tens of
milliseconds.

A preview generated by the agent (`/figma/font-preview` for Adwaita Sans Bold
Italic):

![Example font preview](docs/preview-example.png)

The full wire format, including every endpoint and JSON field, is documented in
[`docs/protocol.md`](docs/protocol.md). How it was recovered from the official
binaries is documented in
[`docs/reverse-engineering.md`](docs/reverse-engineering.md).

## Security

The agent is a loopback HTTP service, so it is deliberately conservative:

- **Binds only to `127.0.0.1`.** Nothing is reachable from the network.
- **Only indexed files are served.** `/figma/font-file?file=/etc/passwd` returns
  404: the `file` parameter is a key into the scan index, not a path to open.
- **`Host` header is validated** against `localhost`, `127.0.0.1` and `[::1]` on
  the configured ports. This defeats DNS-rebinding attacks, where a hostile page
  resolves a domain it controls to `127.0.0.1` and talks to your services.
- **CORS is restricted** to `https://www.figma.com` (and `https://figma.com`).
- The systemd unit runs with `NoNewPrivileges`, `ProtectSystem=full` and a
  read-only home directory.

Unlike the official agent, this one has no telemetry, no crash reporting, no
self-updater and no desktop-app integration.

## Limitations

- **Preview shape, not paragraph layout.** Previews ignore kerning and
  ligatures and always use the family name as the sample text, which matches
  what Figma shows in its font picker.
- **Variable fonts:** named instances are listed, but the `HVAR` advance-width
  deltas are not applied in previews. Outlines do use the instance's
  coordinates.
- **No font *installation*.** This serves fonts already installed on the
  system; install fonts the usual way (`~/.local/share/fonts` + `fc-cache -f`).
- **No spelling service.** The official agent also exposes `/spelling/*` for
  Figma's spellchecker; it is not implemented.
- Figma may still ask to connect after a browser restart if you clear site data
  or revoke the local-network permission.

## Troubleshooting

**Figma does not see the agent**

```bash
systemctl --user status figma-font-agent
journalctl --user -u figma-font-agent -n 50
ss -tlnp | grep -E '44950|44960'
```

Look for `listening on http://127.0.0.1:44950` in the log. If the port is taken,
another font agent (for example the official one under Wine, or
`figma-agent-linux`) is already running; stop it first.

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

## Development

```bash
cargo test
cargo run -- --print-index | head -40   # inspect what Figma would receive
cargo run -- -v                          # run with request logging
```

## Credits

- Protocol recovered from Figma's own `InstallFigmaAgent` binaries, using the
  [REA](https://github.com/morluto/rea) reverse-engineering methodology.
- [`neetly/figma-agent-linux`](https://github.com/neetly/figma-agent-linux)
  proved the protocol works from a third-party implementation and was used to
  cross-check the recovered route table.
- Font parsing by [`ttf-parser`](https://github.com/harfbuzz/ttf-parser).

## License

MIT — see [LICENSE](LICENSE). Not affiliated with or endorsed by Figma, Inc.
