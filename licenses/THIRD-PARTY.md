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

## WinDivert

https://github.com/basil00/WinDivert. Redistributed unmodified as part of the
Flowseal Windows bundle; license alternatives and copyright in `WinDivert-LICENSE`.
This is a separate executable/driver component. Required corresponding-source
delivery and exact-binary/version matching must be completed before commercial
distribution; a link alone is not treated here as completed compliance.

## Rust / JavaScript / Go dependencies

Exact package versions in Cargo.lock, package-lock.json and engines/macos/go.sum.
Generate their full notices for each production artifact. No third-party license
is replaced by the application's own MIT license.
