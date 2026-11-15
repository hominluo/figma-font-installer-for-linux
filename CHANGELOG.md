# Changelog

All notable changes to this project are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project follows
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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

[0.1.0]: https://github.com/hominluo/figma-font-installer-for-linux/releases/tag/v0.1.0
