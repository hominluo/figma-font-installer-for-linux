# Changelog

All notable changes to this project are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project follows
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.2] - 2026-11-15

### Changed

- The README explains how to connect Figma to the agent: the font picker stays
  empty until Figma is connected once, and Figma's own dialog points to a
  Downloads page that has nothing for Linux. It walks through
  **I'm ready to connect** and the browser's local-network prompt, how to see
  the connection in the log, and what to do when the list stays empty.
- `install.sh` ends with the same connect steps.

No change to the agent itself.

## [0.2.1] - 2026-11-15

### Fixed

- The agent listens before its first font scan instead of after it. While the
  scan ran, which takes several seconds on a cold cache, connections were
  refused and Figma could conclude that no agent was installed. A request that
  arrives during the scan now waits for its answer.

## [0.2.0] - 2026-11-15

### Added

- `.rpm`, AppImage and Flatpak packages for x86_64 and aarch64, and a prebuilt
  Arch package for x86_64, next to the archive and the `.deb`.
- The release archive contains `install.sh` and installs itself without a
  download. The AppImage does the same with `--install`.
- The release workflow can be started by hand to build and smoke-test every
  package without publishing.

### Changed

- The font list is refreshed whenever Figma asks for it, so a font that was
  installed, removed or disabled is picked up at once instead of within five
  minutes.
- fontconfig decides which fonts exist. The user font directories are no longer
  walked separately, which had brought back fonts a font manager had disabled.
  Fonts dropped into `~/.local/share/fonts` are still found without `fc-cache`.
- Requests whose `Origin` is not Figma's are refused with `403`. Before, CORS
  kept other sites from reading a response but the request still ran.
- `/figma/desktop/can-open-url` is gone and answers `404`, as the protocol
  notes always said.
- The first browser connection is logged once; per-request lines are debug
  output.
- The `.deb` recommends `fontconfig`.

### Fixed

- `install.sh --uninstall`, `--help` and `--from-source` exited with status 1
  after succeeding.
- `install.sh` stopped without a message when `systemctl --user` had no manager
  to talk to.
- With socket activation on a port other than 44950 or 44960, every request was
  rejected for its `Host` header.
- With socket activation, `fc-list` inherited the listening socket and the
  `LISTEN_*` environment.
- Font files that cannot be parsed were read again on every rescan.

## [0.1.1] - 2026-11-15

### Fixed

- `install.sh` finds the binary inside the release archive, which keeps a
  top-level directory (`figma-font-agent/`).
- The `.deb` ships the static musl binary, so it no longer depends on
  `libc6 >= 2.39` and installs on older Debian/Ubuntu releases.
- Release artifacts include an arm64 `.deb`.

## [0.1.0] - 2026-11-15

First release.

### Added

- Local agent HTTP API on `127.0.0.1:44950` (and `44960`): `/figma/version`,
  `/figma/font-files`, `/figma/font-file`, `/figma/font-preview`,
  `/figma/health`, `/figma/clear-data`.
- Font discovery through fontconfig plus `~/.local/share/fonts` and `~/.fonts`,
  with incremental rescans based on mtime and size.
- TrueType/OpenType collection support and variable fonts, including `fvar`
  named instances exposed as individual styles.
- SVG font previews rendered from glyph outlines with `ttf-parser`.
- CORS and Private Network Access responses for `https://www.figma.com`.
- `Host` header validation and index-based file whitelisting.
- systemd user service and optional socket activation.
- `install.sh` that installs a prebuilt static release (with checksum
  verification) or builds from source, plus `--socket`, `--prefix` and
  `--uninstall`.
- Packaging for Arch Linux (`packaging/PKGBUILD`) and Debian/Ubuntu
  (`cargo-deb`, built in CI).
- Man page, protocol reference and reverse-engineering notes.

[0.2.2]: https://github.com/hominluo/figma-font-installer-for-linux/releases/tag/v0.2.2
[0.2.1]: https://github.com/hominluo/figma-font-installer-for-linux/releases/tag/v0.2.1
[0.2.0]: https://github.com/hominluo/figma-font-installer-for-linux/releases/tag/v0.2.0
[0.1.1]: https://github.com/hominluo/figma-font-installer-for-linux/releases/tag/v0.1.1
[0.1.0]: https://github.com/hominluo/figma-font-installer-for-linux/releases/tag/v0.1.0
