# Handoff

State of the project as of **2026-11-15**, at release **v0.2.1**. Written so
that someone (or some agent) can pick this up without re-deriving anything.

- Repo: <https://github.com/hominluo/figma-font-installer-for-linux> (public, MIT)
- Local checkout: `~/Github/Personal/figma-font-installer-for-linux`
- Releases: <https://github.com/hominluo/figma-font-installer-for-linux/releases>
- CI: `ci.yml` (fmt, clippy, test, on push/PR), `release.yml` (on `v*` tags, and
  by hand as a dry run)

## What it is

Figma's "local fonts" feature is served by a helper Figma ships only for macOS
and Windows. This project is an independent Rust reimplementation of that
helper's loopback HTTP protocol, so Figma in the browser can list and render the
fonts installed on a Linux machine.

```
Figma (browser) → http://127.0.0.1:44950/figma/* → figma-font-agent → fontconfig + ttf-parser
```

The protocol was recovered from Figma's own installers; the route table, JSON
shapes and headers are documented in `docs/protocol.md`, and how they were
recovered in `docs/reverse-engineering.md`. No Figma code is used.

## Code map

| File | Lines | Purpose |
| --- | --- | --- |
| `src/main.rs` | ~253 | CLI parsing, config load, systemd socket activation, startup |
| `src/config.rs` | ~149 | JSON config, defaults, `--init-config` |
| `src/fonts.rs` | ~574 | fontconfig discovery, `ttf-parser` face parsing, `fvar` named instances, incremental rescan |
| `src/proto.rs` | ~204 | wire types, payload construction, weight/width mapping |
| `src/preview.rs` | ~151 | glyph outlines → SVG preview |
| `src/server.rs` | ~545 | routes, origin and `Host` checks, CORS/PNA headers, rescans |
| `src/logging.rs` | ~70 | minimal logger |

Runtime behaviour worth knowing:

- Fonts come from `fc-list --format=%{file}`. fontconfig is authoritative: it
  finds fonts dropped into `~/.local/share/fonts` without `fc-cache`, and it
  hides fonts a font manager disabled with a `rejectfont` rule. The standard
  directories are walked only when `fc-list` is missing or returns nothing.
- The index is rebuilt before every `/figma/font-files` answer (at most once
  per two seconds, one scan at a time) and on the background timer. Rescans are
  incremental (mtime + size); one that changes nothing logs at `debug`.
- The ports are bound before the first scan, which takes seconds on a cold
  cache (6 s was seen here under memory pressure). A request that arrives
  meanwhile waits in the listen queue instead of being refused.
- Variable fonts are exposed as their `fvar` named instances, one entry each.
- Only files present in the index are served; `file` is a key, not a path.
- A request with a foreign `Origin` gets `403` before any handler runs. `Host`
  is checked against the ports actually being served.
- `LISTEN_FDS`/`LISTEN_PID`/`LISTEN_FDNAMES` are consumed before the first scan
  and the passed sockets are re-opened close-on-exec, so `fc-list` inherits
  neither.
- In a Flatpak sandbox (`/.flatpak-info` exists) only fonts under `/run/host`
  are listed, not the runtime's own.

## Distribution

`install.sh` (repo root, also inside the archive and the AppImage) supports:

| Invocation | Behaviour |
| --- | --- |
| `./install.sh` | download latest release, verify `SHA256SUMS`, install, start |
| `./install.sh` next to a release binary | install that binary, download nothing |
| `./install.sh --version v0.2.0` | download that specific tag |
| `./install.sh --from-source` | `cargo build --release --locked` and install that |
| `./install.sh --socket` | install socket unit instead of service (start on first request) |
| `./install.sh --prefix DIR` | install under `DIR` (default `~/.local`) |
| `./install.sh --uninstall` | stop and remove binary, units, man page; keeps config |

Details that matter if you edit it:

- The unit ships with `ExecStart=/usr/bin/figma-font-agent`; the installer
  rewrites that line to the chosen prefix with `sed`.
- "Next to a release binary" means the script was run from a file whose
  directory holds an executable `figma-font-agent` and the service unit. Piped
  into bash there is no such directory, so `curl | bash` always downloads.
- `have_systemd` requires a non-empty `XDG_RUNTIME_DIR` **and** a user manager
  that answers `systemctl --user show-environment`; otherwise it installs the
  binary and prints the manual start command. Unset `XDG_RUNTIME_DIR` for tests
  so you never touch the real user service.
- The `EXIT` trap must not fail: under `set -e` a failing trap replaces the
  script's exit status.

### Release artifacts (produced by `.github/workflows/release.yml`)

One static musl binary per architecture goes into every package:

- `figma-font-agent-<target>.tar.gz` — binary, `install.sh`, man page, both
  units, README, LICENSE (x86_64, aarch64)
- `figma-font-agent_<version>-1_<amd64|arm64>.deb` — `Depends:` empty,
  `Recommends: fontconfig`
- `figma-font-agent-<version>-1.<x86_64|aarch64>.rpm` — unsigned
- `figma-font-agent-<version>-<x86_64|aarch64>.AppImage` — runs the agent;
  `--install`/`--uninstall` run the bundled `install.sh`
- `figma-font-agent-<version>-<x86_64|aarch64>.flatpak` — single-file bundle,
  app id `io.github.hominluo.FigmaFontAgent`, runtime
  `org.freedesktop.Platform//26.08`, only `--share=network`
- `figma-font-agent-<version>-1-x86_64.pkg.tar.zst` — Arch package built from
  `packaging/PKGBUILD` (dynamic glibc build, runs `cargo test` in `check()`)
- `SHA256SUMS` over all of the above

Jobs: `packages` (matrix x86_64/aarch64: build, package, smoke-test, upload),
`arch` (`archlinux:base-devel` container, `makepkg`, installs the result),
`release` (tags only: merge artifacts, checksum, publish).

### How each package is built

- `.deb`: `cargo deb --no-build --target <triple>` from
  `[package.metadata.deb]`. Assets resolve through cargo-deb's `target/release`
  prefix, which `--target` redirects to the cross-compiled directory.
- `.rpm`: `cargo generate-rpm --target <triple>` from
  `[package.metadata.generate-rpm]`, same prefix rule.
- AppImage: `packaging/appimage/build.sh` lays out an AppDir whose payload
  directory looks like a release archive, then runs `appimagetool` 1.9.1.
- Flatpak: `packaging/flatpak/build.sh` writes the build directory and its
  `metadata` by hand and uses `flatpak build-finish`, `build-export` and
  `build-bundle`. Nothing is compiled against the SDK, so neither SDK nor
  runtime has to be installed to build.
- Arch: the workflow creates the source tarball with `git archive` under the
  name the PKGBUILD's `source` entry expects, so `makepkg` does not download
  the tag archive, and passes `--skipchecksums`. `packaging/PKGBUILD` itself is
  a **source** package from the GitHub tag archive with a pinned `sha256sums`.
  It is not in the AUR.

The desktop entry, icon and AppStream metadata shared by the AppImage and the
Flatpak are `packaging/io.github.hominluo.FigmaFontAgent.{desktop,svg,metainfo.xml}`.

## Decisions and why

| Decision | Reason |
| --- | --- |
| musl static binaries | one artifact runs on any distro; no glibc floor (the first `.deb` had `libc6 >= 2.39` and was rebuilt) |
| every package wraps the same binary | identical bytes everywhere; one build to trust |
| `install.sh` downloads releases by default | users should not need Rust |
| systemd **user** units, no root | the agent only reads the user's fonts |
| fontconfig is authoritative | parity with the desktop's font pickers, including fonts a manager disabled |
| rescan when Figma asks | a new font must not take five minutes; an incremental scan costs tens of milliseconds |
| foreign `Origin` → `403` | CORS only withholds the response; the request itself would still run |
| `Host` header validation | defeats DNS rebinding against a loopback service |
| `--version`/`--package` configurable | future Figma clients can demand a newer pair without a rebuild |
| no telemetry, no updater | deliberate difference from the official agent, documented in the README |
| Flatpak as a bundle, not on Flathub | Flathub wants a build from source and a review; the bundle needs neither |

## Verify a release (what was actually run for v0.2.0 and v0.2.1)

```bash
# download path, no systemd involvement
rm -rf /tmp/t && mkdir -p /tmp/t
env -u XDG_RUNTIME_DIR HOME=/tmp/t ./install.sh --version v0.2.1
/tmp/t/.local/bin/figma-font-agent --version      # 0.2.1

# live service on this machine
systemctl --user is-active figma-font-agent       # active
curl -sS http://127.0.0.1:44950/figma/version     # {"package":"126.11.9","version":24}
curl -sS http://127.0.0.1:44950/figma/font-files \
  | python3 -c 'import json,sys; print(len(json.load(sys.stdin)["fontFiles"]), "files")'

# CORS preflight, traversal guard, foreign origin
curl -s -o /dev/null -w '%{http_code}\n' -X OPTIONS \
  http://127.0.0.1:44950/figma/font-files \
  -H 'Origin: https://www.figma.com' \
  -H 'Access-Control-Request-Method: GET' \
  -H 'Access-Control-Request-Private-Network: true'   # 204
curl -s -o /dev/null -w '%{http_code}\n' \
  'http://127.0.0.1:44950/figma/font-file?file=/etc/passwd'   # 404
curl -s -o /dev/null -w '%{http_code}\n' -X POST \
  -H 'Origin: https://evil.example' http://127.0.0.1:44950/figma/clear-data   # 403
```

Reference numbers on this machine: **747 files, 813 faces** (829 style entries
in the payload; the log counts deduplicated faces).

Also verified for this release:

- **A real browser.** Headless Chrome 157 on this machine, on the real
  `https://www.figma.com` origin: every request fails with
  `LocalNetworkAccessPermissionDenied` until the `loopback-network` permission
  is granted, then `/figma/version`, `/figma/font-files`, `/figma/font-file`,
  `/figma/font-preview` and `/figma/clear-data` answer and a `FontFace` loads
  the served file. From `https://example.com` with the same grant, the request
  is refused.
- **Packages in clean containers.** `.deb` on Debian 11 and Ubuntu 20.04
  (glibc 2.31), `.rpm` on current Fedora via `dnf`, AppImage and archive
  self-install on Ubuntu 20.04 with networking off, Flatpak installed for the
  user here (sees the same 747 files through `/run/host/fonts`).
- **arm64.** The workflow installs and runs the arm64 `.deb`, AppImage and
  archive natively on GitHub's ARM runner. The arm64 binary also runs here under
  qemu-user and produces an index byte-identical to x86_64.
- **Font changes.** With isolated XDG directories: a font copied in appears on
  the next `/figma/font-files`, a `rejectfont` rule hides it, removing the rule
  or the file is followed too; 30 concurrent requests share one scan.
- **Socket activation** with `systemd-socket-activate` and a fake `fc-list`
  that records what it inherits: no `LISTEN_*` variables, no sockets.
- **Startup.** A request sent 0.2 s after the process starts is answered once
  the scan finishes; 0.2.0 refused it.
- `cargo fmt --check`, `cargo clippy --release --all-targets -- -D warnings` and
  `cargo test --release` are clean.

## Release runbook

```bash
# 1. bump Cargo.toml, packaging/PKGBUILD, docs/figma-font-agent.1,
#    packaging/*.metainfo.xml (<release>), CHANGELOG.md
cargo fmt --check && cargo clippy --release --all-targets -- -D warnings && cargo test --release

# 2. commit, push, and run the whole pipeline without publishing
gh workflow run release.yml --ref <branch>
gh run watch                               # all three build jobs must pass
gh run download <run-id>                   # the packages, for a closer look

# 3. tag, push
git tag -a vX.Y.Z -m "figma-font-agent X.Y.Z"
git push origin main vX.Y.Z

# 4. wait for the release workflow, then confirm assets (12 files)
gh run list --workflow release.yml --limit 1
gh release view vX.Y.Z --json assets --jq '.assets[].name'

# 5. pin the Arch checksum (the tag archive is immutable, so this is safe)
curl -fsSL https://github.com/hominluo/figma-font-installer-for-linux/archive/vX.Y.Z.tar.gz \
  | sha256sum
# put that hash in packaging/PKGBUILD, commit with "fix(pkgbuild): pin vX.Y.Z checksum"

# 6. smoke-test the published installer into a scratch prefix
env -u XDG_RUNTIME_DIR HOME=/tmp/t ./install.sh --version vX.Y.Z
```

The tag archive's top directory is `figma-font-installer-for-linux-<version>`,
which is why the PKGBUILD `cd`s there rather than to `$pkgname-$pkgver`.

## Gotchas hit while building this

- Release tarballs keep a `figma-font-agent/` directory; flat-file assumptions
  in the installer broke the first published release.
- `cargo deb` on a glibc build records `libc6 >= 2.39` from the runner; static
  musl removes the dependency (and `dpkg-shlibdeps` is not needed).
- GitHub archive directory names are `<repo>-<version>`, not `<pkgname>-<version>`.
- `install.sh` tests with a fake `HOME` must keep `RUSTUP_HOME`/`CARGO_HOME`, or
  rustup has no default toolchain.
- `cargo-deb` locally (Arch) warns that `dpkg-shlibdeps` is missing and emits an
  empty `Depends:`; that is a local artifact, not a packaging bug.
- Testing GUI/state with `wtype` or similar is unreliable here. A GTK4 app can
  be looked at without touching the screen: `gtk4-broadwayd :N -p PORT`, start
  the app with `GDK_BACKEND=broadway BROADWAY_DISPLAY=:N`, screenshot the page
  with headless Chrome.
- A GTK app started that way is still **the** instance of that app: a launch
  from the desktop is forwarded to the hidden copy. Font Manager 0.9.4 crashed
  on exactly that. Stop the test copy before the user can click.
- Never wrap such a test in `dbus-run-session`. Its document portal takes over
  `/run/user/<uid>/doc` and unmounts it on exit, after which no Flatpak app
  starts (`bwrap: Can't find source path …/doc/by-app/…`). Repair:
  `systemctl --user restart xdg-document-portal.service`.
- `pkill -f PATTERN` matches the shell that runs it when the pattern is in its
  own command line. Kill by PID or write the pattern as `'[p]attern'`.
- The Claude-in-Chrome extension may be connected to a browser on another
  machine, where `127.0.0.1` is not this one. For the real thing, start
  `google-chrome-stable --headless=new --remote-debugging-port=…` with a
  scratch profile and grant `loopbackNetwork` through `Browser.grantPermissions`.
- Figma's CDN answers headless Chrome with an error page on file URLs, so the
  editor cannot be driven that way; `/login` loads and is the right origin.
- Since Chrome 157 a loopback request needs the `loopback-network` permission
  and no longer sends a preflight.
- `rpm -i` on rpm 6 (current Fedora) refuses unsigned packages; `dnf install
  ./file.rpm` accepts them. Fedora container images are built with `nodocs`, so
  the man page looks missing there.
- qemu's binfmt handler does not recognise an aarch64 AppImage (the runtime
  puts magic bytes in the ELF header); the binary inside runs fine.

## Not verified / open work

- **The Connect click inside Figma.** Everything a browser does is verified, on
  the real origin with the real permission model, but opening Preferences →
  Permissions and helpers → Apps on device → Connect needs a logged-in Figma
  session in a browser on this machine. That is the one step a person has to do.
- **Hardened unit on restrictive distributions.** The unit's sandboxing needs an
  unprivileged user namespace. systemd is documented to skip it when it cannot
  create one, but the unit has only been run on Arch.
- The `.rpm` is unsigned; signing needs a key and a place to keep it.
- The Flatpak is a bundle on the release page, not on Flathub, and has no
  autostart of its own (the README shows the one-line copy to `~/.config/autostart`).
- No `/spelling/*` endpoint (Figma's spellchecker), no desktop-app integration.
- Variable-font previews apply instance coordinates but not `HVAR` advance
  deltas; Figma shows one entry per named instance, not axis sliders.
- The preview's size and baseline were never compared with the official agent's.
- `syscall` filtering in the unit is intentionally absent (fontconfig spawns
  processes); hardening is limited to filesystem, device and namespace options.
- PKGBUILD is not in the AUR. Homebrew and Nix packaging are unexplored.

## Where things run on this machine

- Installed binary: `~/.local/bin/figma-font-agent`, installed by `install.sh`
  from the v0.2.1 release (the static musl build).
- Unit: `~/.config/systemd/user/figma-font-agent.service`, enabled and running.
- Config: none. `~/.config/figma-font-agent/config.json` does not exist, so the
  defaults apply; `figma-font-agent --init-config` creates it.
- Logs: `journalctl --user -u figma-font-agent -f`.
- Font manager: GNOME Fonts (`gnome-font-viewer`), which follows the Omarchy
  theme's light/dark mode and is the default handler for font files. Font
  Manager (`font-manager`) was tried and removed: Arch ships 0.9.4, which needs
  its own dark switch and crashed.
