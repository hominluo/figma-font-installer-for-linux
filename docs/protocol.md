# Local Figma Agent protocol

This document describes the HTTP API that Figma for the web speaks with the
official Figma Agent, as recovered from `InstallFigmaAgent.dmg` (macOS,
`126.11.9`) and cross-checked against `InstallFigmaAgent.exe` (Windows). The
implementation in this repository serves the subset needed for local fonts.

## Transport

| Property | Value |
| --- | --- |
| Bind address | `127.0.0.1` only |
| Ports | `44950` (primary), `44960` (also opened by the official agent) |
| Scheme | plain HTTP |
| URL prefix | `/figma/*` (spelling endpoints live under `/spelling/*`) |

The official macOS agent binds both ports and logs `[http] listening on …`
(`[http] bind error …` on failure). Binding `44960` is treated as optional here,
because the browser flow only requires `44950`.

### CORS

Figma is served from `https://www.figma.com`. This implementation answers a
request from any other origin with `403`, and Figma's with:

```
Access-Control-Allow-Origin: https://www.figma.com
Access-Control-Allow-Methods: GET, POST, OPTIONS
Access-Control-Allow-Headers: Content-Type, Accept
Access-Control-Max-Age: 600
Access-Control-Allow-Private-Network: true
```

`Access-Control-Allow-Private-Network` is what makes Chrome's *Local Network
Access* preflight succeed. The official agent also special-cases
`/figma/health` and inspects `Sec-Fetch-Mode`; the endpoint exists mainly so the
page can tell "agent reachable" from "agent missing".

## Endpoints used for local fonts

### `GET /figma/version`

```json
{ "package": "126.11.9", "version": 24 }
```

`package` is the marketing version of the release (`CFBundleShortVersionString`
on macOS), `version` the numeric protocol revision. Figma refuses agents that
report a version it considers too old, so both are configurable.

### `GET /figma/font-files`

The complete font index. Also serves the agent's cache-invalidation handshake.

```json
{
  "fontFiles": {
    "/usr/share/fonts/Adwaita/AdwaitaMono-Bold.ttf": [
      {
        "family": "Adwaita Mono",
        "style": "Bold",
        "postscript": "Adwaita-Mono-Bold",
        "weight": 700,
        "stretch": 5,
        "italic": false,
        "modified_at": 1788565142,
        "user_installed": true
      }
    ]
  },
  "modified_at": null,
  "modified_fonts": null,
  "package": "126.11.9",
  "version": 24
}
```

- Keys of `fontFiles` are absolute font file paths, exactly as they will be sent
  back in `?file=`. Do not normalise them between requests.
- `modified_at` (per font) is the file mtime in Unix seconds.
- `stretch` is `usWidthClass` (1–9), not a percentage.
- `weight` is `usWeightClass` (100–900).
- `user_installed` is `true` for everything found on the system. The official
  macOS agent distinguishes user vs. system locations; Figma does not seem to
  care.
- `modified_at` / `modified_fonts` at the top level implement an incremental
  cache. `null` makes the client fetch the full list, which is always correct.

A **variable font** adds `variationAxes`, and its named instances are advertised
as separate entries rather than one entry with a slider set:

```json
{
  "family": "Adwaita Sans",
  "style": "Bold Italic",
  "postscript": "InterVariableItalic-Bold",
  "weight": 700,
  "stretch": 5,
  "italic": true,
  "variationAxes": [
    { "tag": "opsz", "name": "Optical size", "value": 14, "min": 14, "max": 32, "default": 14, "hidden": false },
    { "tag": "wght", "name": "Weight",       "value": 700, "min": 100, "max": 900, "default": 400, "hidden": false }
  ],
  "modified_at": 1788565142,
  "user_installed": true
}
```

Field names are case-sensitive and intentionally inconsistent: `fontFiles` and
`variationAxes` are camelCase while `modified_at`, `user_installed` and
`postscript` are not.

### `GET /figma/font-file?file=<path>`

Returns the raw font file, `Content-Type` one of `font/ttf`, `font/otf` or
`font/collection`. The `file` value must be one of the keys from
`/figma/font-files`.

The official agent streams the path it is given. This implementation only
serves paths that exist in its scan index — a deliberate difference, so a
malicious page cannot use the agent to read arbitrary local files.

### `GET /figma/font-preview`

Query parameters (all sent by Figma):

| Parameter | Meaning |
| --- | --- |
| `file` | absolute font file path from `/figma/font-files` |
| `family` | family name |
| `style` | style / named-instance name |
| `postscript` | PostScript name, the primary matching key |
| `font_size` | size in **points** (the official agent converts pt → px as `× 96 / 72`) |

Response: an image of the family name rendered in that face. The format the
official agent answers with was not captured. This implementation returns
`image/svg+xml`, as `neetly/figma-agent-linux` does, which the browser scales
without loss. `404` means "no preview for this face", and Figma falls back to
its default tile.

### `GET /figma/health`

Liveness probe used by the page before it announces the agent. `200` with a
short body is enough.

### `POST /figma/clear-data`

Drops the agent's cached index; the next `/figma/font-files` request re-scans.
The official agent uses this when Figma's *Fonts on this device* panel is
refreshed. This implementation rescans at once, and also before every
`/figma/font-files` answer, so the call is only needed to force a rescan.

## Endpoints detected but not implemented

These appear in the official route table and are intentionally absent here:

| Route | Purpose |
| --- | --- |
| `/figma/desktop/can-open-url`, `/figma/desktop/open-url` | hand a `figma://` URL to the desktop app |
| `/figma/desktop/set-open-in-desktop-app-pref` | remember the "open links in desktop app" preference |
| `/spelling/check`, `/spelling/suggest` | spellchecker |
| `/spelling/get-languages`, `/spelling/set-language` | spellchecker languages |
| `/spelling/add-words`, `/spelling/remove-words`, `/spelling/ignore-words` | user dictionaries |

Figma degrades gracefully when they 404: local fonts keep working.

## Parameters seen in the binaries

The Windows PE contains the complete query-string name table, which is how these
were confirmed even where the macOS build had deduplicated string literals:

```
file, font_size, postscript, family, style,
machine_id, launch_source, userID, url, canOpen, opened
```

`machine_id`, `launch_source` and `userID` belong to the desktop-app and
telemetry routes, not to font loading.

## Version history observed

| Agent release | `package` | `version` |
| --- | --- | --- |
| 2025-11 macOS/Windows build | `125.9.10` | `23` |
| 126.x build reversed here | `126.11.9` | `24` |

If Figma starts rejecting the agent, raise `package`/`version` in the config file
first; the JSON schema itself has been stable across these releases.
