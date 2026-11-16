# Handoff

State of the project as of **2026-11-15**, at release **v0.1.1**. Written so
that someone (or some agent) can pick this up without re-deriving anything.

- Repo: <https://github.com/hominluo/figma-font-installer-for-linux> (public, MIT)
- Local checkout: `~/Github/Personal/figma-font-installer-for-linux`
- Releases: <https://github.com/hominluo/figma-font-installer-for-linux/releases>
- CI: `ci.yml` (fmt, clippy, test, on push/PR), `release.yml` (on `v*` tags)

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
| `src/main.rs` | ~242 | CLI parsing, config load, systemd socket activation, startup |
| `src/config.rs` | ~149 | JSON config, defaults, `--init-config` |
| `src/fonts.rs` | ~545 | fontconfig + user-dir discovery, `ttf-parser` face parsing, `fvar` named instances, incremental rescan |
| `src/proto.rs` | ~204 | wire types, payload construction, weight/width mapping |
| `src/preview.rs` | ~151 | glyph outlines → SVG preview |
| `src/server.rs` | ~469 | routes, CORS/PNA, `Host` validation, rescan timer |
| `src/logging.rs` | ~70 | minimal logger |

Runtime behaviour worth knowing:

- Fonts come from `fc-list --format=%{file}` **plus** `~/.local/share/fonts` and
  `~/.fonts`, so a dropped-in font appears without `fc-cache`.
- Rescans are incremental (mtime + size); an unchanged rescan logs at `debug`
  so the journal stays quiet.
- Variable fonts are exposed as their `fvar` named instances, one entry each.
- Only files present in the index are served; `file` is a key, not a path.
- `LISTEN_FDS`/`LISTEN_PID` are consumed and removed so children (`fc-list`)
  never inherit them.

## Distribution

`install.sh` (repo root) supports:

| Invocation | Behaviour |
| --- | --- |
| `./install.sh` | download latest release, verify `SHA256SUMS`, install, start |
| `./install.sh --version v0.1.1` | same for a specific tag |
| `./install.sh --from-source` | `cargo build --release --locked` and install that |
| `./install.sh --socket` | install socket unit instead of service (start on first request) |
| `./install.sh --prefix DIR` | install under `DIR` (default `~/.local`) |
| `./install.sh --uninstall` | stop and remove binary, units, man page; keeps config |

Details that matter if you edit it:

- The unit ships with `ExecStart=/usr/bin/figma-font-agent`; the installer
  rewrites that line to the chosen prefix with `sed`.
- It looks for the binary with `find … -name figma-font-agent -perm -u+x`,
  because release archives contain a top-level `figma-font-agent/` directory.
- `have_systemd` requires a non-empty `XDG_RUNTIME_DIR`; otherwise it installs
  the binary and prints the manual start command. Use that for tests so you
  never touch the real user service.
- A `TMP_DIR` cleanup trap removes the download directory on every exit path.

### Release artifacts (produced by `.github/workflows/release.yml`)

- `figma-font-agent-x86_64-unknown-linux-musl.tar.gz` — static binary, man page,
  both units, README, LICENSE
- `figma-font-agent-aarch64-unknown-linux-musl.tar.gz` — same, arm64
- `figma-font-agent_<version>-1_amd64.deb` and `…_arm64.deb` — static binary, so
  `Depends:` is empty and there is no libc version floor
- `SHA256SUMS` over all of the above

Jobs: `binaries` (matrix x86_64/aarch64 musl), `deb` (same matrix, `cargo-deb
--no-build --target`), `release` (merge artifacts, checksum, publish).

### Packages

- Debian/Ubuntu: built in CI with `[package.metadata.deb]` in `Cargo.toml`.
  Assets resolve through cargo-deb's `target/release` magic prefix, so
  `--target <triple>` redirects them to the cross-compiled directory.
- Arch: `packaging/PKGBUILD` is a **source** package from the GitHub tag archive
  (not published to the AUR — no credentials, and it is not packaged for
  `x86_64`/`aarch64` review). `sha256sums` is pinned to the tag archive hash.

## Decisions and why

| Decision | Reason |
| --- | --- |
| musl static binaries | one artifact runs on any distro; no glibc floor (the first `.deb` had `libc6 >= 2.39` and was rebuilt) |
| `.deb` built from the musl binary | avoids a libc dependency entirely; arm64 package comes free from the arm runner |
| `install.sh` downloads releases by default | users should not need Rust |
| systemd **user** units, no root | the agent only reads the user's fonts |
| `Host` header validation | defeats DNS rebinding against a loopback service |
| CORS limited to `figma.com` | same |
| `--version`/`--package` configurable | future Figma clients can demand a newer pair without a rebuild |
| no telemetry, no updater | deliberate difference from the official agent, documented in the README |

## Verify a release (what was actually run for v0.1.1)

```bash
# download path, no systemd involvement
rm -rf /tmp/t && mkdir -p /tmp/t
env -u XDG_RUNTIME_DIR HOME=/tmp/t ./install.sh --version v0.1.1
/tmp/t/.local/bin/figma-font-agent --version      # 0.1.1

# live service on this machine
systemctl --user is-active figma-font-agent       # active
curl -sS http://127.0.0.1:44950/figma/version     # {"package":"126.11.9","version":24}
curl -sS http://127.0.0.1:44950/figma/font-files \
  | python3 -c 'import json,sys; print(len(json.load(sys.stdin)["fontFiles"]), "files")'

# CORS preflight and traversal guard
curl -s -o /dev/null -w '%{http_code}\n' -X OPTIONS \
  http://127.0.0.1:44950/figma/font-files \
  -H 'Origin: https://www.figma.com' \
  -H 'Access-Control-Request-Method: GET' \
  -H 'Access-Control-Request-Private-Network: true'   # 204
curl -s -o /dev/null -w '%{http_code}\n' \
  'http://127.0.0.1:44950/figma/font-file?file=/etc/passwd'   # 404
```

Reference numbers on this machine: **747 files, 813 faces** (829 style entries
in the payload; the log counts deduplicated faces).

Also verified: `makepkg` builds the Arch package from the published tarball,
`cargo fmt --check`, `cargo clippy --release --all-targets -- -D warnings` and
`cargo test --release` are clean, and `systemd-analyze --user verify` accepts
the rewritten unit.

## Release runbook

```bash
# 1. bump Cargo.toml, packaging/PKGBUILD, docs/figma-font-agent.1, CHANGELOG.md
cargo fmt --check && cargo clippy --release --all-targets -- -D warnings && cargo test --release

# 2. commit, tag, push
git tag -a vX.Y.Z -m "figma-font-agent X.Y.Z"
git push origin main vX.Y.Z

# 3. wait for the release workflow, then confirm assets
gh run list --workflow release.yml --limit 1
gh release view vX.Y.Z --json assets --jq '.assets[].name'

# 4. pin the Arch checksum (the tag archive is immutable, so this is safe)
curl -fsSL https://github.com/hominluo/figma-font-installer-for-linux/archive/vX.Y.Z.tar.gz \
  | sha256sum
# put that hash in packaging/PKGBUILD, commit with "fix(pkgbuild): pin vX.Y.Z checksum"

# 5. smoke-test the published installer into a scratch prefix
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
- Testing GUI/state with `wtype` or similar is unreliable here; verification is
  done with `curl` and `systemctl`.

## Not verified / open work

- **The Figma handshake itself.** Everything up to the browser is verified, but
  Figma → Preferences → Permissions and helpers → Apps on device → Connect, plus
  the Chrome Local Network Access prompt, needs a logged-in Figma session. That
  is the one end-to-end step a maintainer has to do by hand.
- **arm64 packages** build in CI but were never executed on arm64 hardware.
- `syscall` filtering in the unit is intentionally absent (fontconfig spawns
  processes); hardening is limited to filesystem, device and namespace options.
- No `/spelling/*` endpoint (Figma's spellchecker), no desktop-app integration.
- Variable-font previews apply instance coordinates but not `HVAR` advance
  deltas; Figma shows one entry per named instance, not axis sliders.
- PKGBUILD is not in the AUR; the README says so explicitly.
- Homebrew/Nix/Flatpak packaging is unexplored.

## Where things run on this machine

- Installed binary: `~/.local/bin/figma-font-agent` (installed from the v0.1.1
  release, i.e. the static musl build).
- Units: `~/.config/systemd/user/figma-font-agent.{service,socket}`.
- Config (untouched by the installer): `~/.config/figma-font-agent/config.json`.
- Logs: `journalctl --user -u figma-font-agent -f`.
- Rescan: `curl -X POST http://127.0.0.1:44950/figma/clear-data`.
