# Reverse engineering the Figma Agent

Notes on how the local agent protocol was recovered, and what was and was not
verified. This is documentation of an interoperability effort: no Figma code is
copied into this repository, and the implementation is written from the observed
wire format.

## Targets

| Artifact | Format | Notes |
| --- | --- | --- |
| `InstallFigmaAgent.dmg` | HFS+ disk image | contains `Install Figma Agent.app/Contents/MacOS/figma_agent` |
| `InstallFigmaAgent.exe` | NSIS installer | contains `figma_agent.exe` (PE32, 10.5 MB) |

Both were unpacked with [7-Zip](https://www.7-zip.org/) 25.01 (`7zz x`), which
handles HFS+ images and NSIS archives without needing macOS or Wine.

The macOS binary is the primary target: a 11.8 MB x86_64 Mach-O, symbols
stripped down to 438 entries, built from Rust. The Windows PE is not stripped and
was used to confirm query-parameter names that the macOS build had folded into
unrelated string literals.

## Tooling

- `llvm-objdump -d` for the full disassembly (≈2.06M lines), plus `nm`, `otool`
  and Python for extraction.
- A small script walks the disassembly for the pattern Rust emits when building
  a `&'static str`: `lea` of a `__const` address followed by a `mov $imm` length.
  Pairing the two recovers exact literals, which matters because **Rust string
  literals are not NUL-terminated** — plain `strings` glues adjacent literals
  together and hides boundaries.
- `__TEXT` VMAs map to file offsets by subtracting the `0x100000000` image base;
  `__const` data can then be read directly to confirm contents.

## Fingerprinting the binary

Adjacent literals in `__const` identified the stack immediately:

```
actix_web::types::json
sec-fetch-mode
ContentType Deserialize Serialize Payload Content type error
text/plain; charset=utf-8
```

plus `reqwest`/`hyper` (outbound HTTP), `mime_guess`, `sentry` (crash
reporting), and an embedded **HarfBuzz** — the later used for font preview
rasterisation. This is an actix-web server that also talks to Figma's update
service (`https://desktop.figma.com/agent/RELEASE.json`).

## Listening sockets

Two listen addresses are constructed in the startup path:

```
movl $0x100007f, …     ; 127.0.0.1
movw $0xaf96, …        ; 44950
movw $0xafa0, …        ; 44960
```

with the log literals `"[http] bind error "` and `"[http] listening on "`
nearby. So the agent binds **127.0.0.1:44950** and **127.0.0.1:44960**, loopback
only. The web flow works with 44950 alone; 44960 is kept as a fallback.

## Recovering the route table

actix-web registers resources by building a path and a set of method guards.
Scanning for `(ptr, len)` string pairs in `__const` and correlating them with the
call sites produced this table:

```
/figma
  /desktop/can-open-url
  /desktop/open-url                 (registered twice)
  /desktop/set-open-in-desktop-app-pref
  /font-files
  /font-file
  /font-preview
  /version
  /health
  /clear-data
/spelling
  /check  /suggest  /get-languages  /set-language
  /add-words  /remove-words  /ignore-words
```

An interesting artefact: on macOS the string `file` only appears as a substring
of the `/font-file` path, because the compiler deduplicated it. The Windows PE
still has the full query-string table, which is where the parameter list was
confirmed:

```
file, font_size, postscript, family, style,
machine_id, launch_source, userID, url, canOpen, opened
```

## JSON shapes

Struct field names survive in `__const` next to their `serde` `Deserialize`
implementations, giving the exact casing:

```
fontFiles  modified_at  modified_fonts  package  version
family  style  postscript  weight  stretch  italic
variationAxes  user_installed
tag  name  value  min  max  default  hidden
```

Note the deliberate inconsistency: `fontFiles`/`variationAxes` are camelCase,
everything around them is snake_case or lowercase. The payloads in
[`protocol.md`](protocol.md) are transcribed from these field lists plus the
`Cache` struct that the agent persists to `font_cache.json`.

`/figma/version` returns `{"package": <marketing version>, "version": <numeric
revision>}`. In this build the marketing version is `126.11.9`
(`CFBundleShortVersionString`) and the numeric revision is `0x18` = `24`.

## CORS behaviour

The agent's middleware embeds these literals together:

```
Content-Type, Accept
Access-Control-Max-Age: 600
access-control-request/allow-private-network
```

and the allowed origin is `https://www.figma.com`. `/figma/health` is special
cased and inspects `sec-fetch-mode`, which is why a plain `curl` against it
behaves slightly differently from a browser request.

This is also the mechanism behind Chrome's permission prompt: a page on
`https://www.figma.com` making a request to `127.0.0.1` triggers the *Local
Network Access* check, and the agent has to answer the preflight with
`Access-Control-Allow-Private-Network: true`.

## Cross-validation

[`neetly/figma-agent-linux`](https://github.com/neetly/figma-agent-linux) is an
independent MIT-licensed Rust implementation of the same protocol. Its route
list (`/figma/{version,font-files,font-file,font-preview}`), bind port (44950),
allowed origin and private-network handling match the recovered table exactly.
It also demonstrates the one thing static analysis cannot: that a third-party
agent is accepted by the current Figma client. It was used as a reference point
for payload semantics (for example that `modified_at`/`modified_fonts` may be
`null` and that `user_installed` is not enforced).

## Not verified

- The spelling endpoints were located but not exercised; their request/response
  bodies are unknown and unimplemented here.
- The desktop-app routes (`can-open-url`, `open-url`) depend on macOS/Windows
  URL handlers and are meaningless on Linux.
- The exact raster format of the official preview response is unknown to us; we
  return SVG instead, which the browser accepts. Figma only needs *an image*.
- The update-manifest flow (`RELEASE.json`) was not exercised. This
  implementation has no updater.

## Deliberate differences from the official agent

| Area | Official | This project |
| --- | --- | --- |
| File serving | streams the requested path | serves only paths present in the scan index |
| `Host` header | not validated | validated against loopback names + known ports (DNS-rebinding defence) |
| Telemetry | Sentry + update checks | none |
| Previews | HarfBuzz raster | `ttf-parser` outlines → SVG |
| Desktop app / spelling | implemented | not implemented |
| Platform font discovery | DirectWrite / CoreText | fontconfig (`fc-list`) |

None of these change what Figma sees on the font-loading path.

## Method reference

The workflow followed the [REA](https://github.com/morluto/rea) methodology:
fingerprint the artifact, map its capabilities, recover the observable contract,
then validate the contract against an independent implementation before writing
code.
