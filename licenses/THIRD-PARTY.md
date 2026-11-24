# Third-party components

## macOS engine

`engines/macos/` derives from https://github.com/naladwepo/zapret-for-mac,
commit `763790eb671b7bc5d89399b8af46ebed70a39f0d`.
MIT, copyright 2026 Egor Kulishov. Original `LICENSE` and notices retained.
Changes: desktop lifecycle/persistence, custom-domain RPC, isolated launchd guard
and log names. Original datapath source and tests retained.

## Flowseal / bol-van

Windows binary/data bundle: Flowseal release 1.10.4, SHA256 in
`resources/upstream.lock.json`. Original license copied as `Flowseal-MIT.txt`;
the revision providing that license is recorded separately in the lockfile.
Underlying engines: https://github.com/bol-van/zapret (MIT).

## tg-ws-proxy

`crates/zapretd/src/telegram/` is a Rust port of the core of
https://github.com/Flowseal/tg-ws-proxy (release 1.11.4): the MTProto
obfuscation handshake, re-encryption, packet splitting, the WebSocket client and
the gateway addresses. MIT, copyright Flowseal; license copied as
`tg-ws-proxy-MIT.txt`. Not ported: the Cloudflare proxy and worker routes, HTTP/2
media, Fake TLS, the tray application. No upstream code or binary is shipped.

## WinDivert

https://github.com/basil00/WinDivert. Redistributed unmodified as part of the
Flowseal Windows bundle; license alternatives and copyright in `WinDivert-LICENSE`.
This is a separate executable/driver component. Required corresponding-source
delivery and exact-binary/version matching must be completed before commercial
distribution; a link alone is not treated here as completed compliance.

## Cygwin runtime

The pinned Windows bundle also includes `cygwin1.dll`. Its runtime license,
exceptions and exact-version corresponding-source requirements must be included
in the redistribution review. Do not treat the complete Windows bundle as MIT.

## Rust / JavaScript / Go dependencies

Exact package versions in Cargo.lock, package-lock.json and engines/macos/go.sum.
Generate their full notices for each production artifact. No third-party license
is replaced by the application's own GPLv3 license.

## Interface fonts and icons

Bundled into the desktop interface and the installer window, nothing is loaded
at runtime:

- Fixel Text and Fixel Display by MacPaw, in `ui/src/assets/fonts/`, SIL Open
  Font License 1.1: `Fixel-OFL.txt`. Used by the desktop interface and, since
  0.4.1, by the installer window. Inter and Geist Mono are no longer bundled;
  `Inter-OFL.txt` and `GeistMono-OFL.txt` stay for copies made before 0.4.1.
- OverlayScrollbars 2.17.1 by KingSora (Rene Haas), MIT, bundled into the
  desktop interface for the scrollbars. Its notice comes from package-lock.json
  with the other JavaScript dependencies.
- Lucide icon paths, inlined in `ui/src/Icon.svelte`, ISC: `Lucide-ISC.txt`.
- YouTube, Discord and Cloudflare marks from Simple Icons 16.38.0, inlined in
  `ui/src/components/Overview.svelte`, CC0 1.0. The marks themselves are
  trademarks of their owners and only name the services being checked.
