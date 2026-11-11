# Flowseal 1.10.4 snapshot

The 22 `general*.bat` inputs are copied byte-for-byte from the release pinned in
`resources/upstream.lock.json` (archive SHA256 recorded in `source.json`). They are
never executed. `LICENSE-flowseal.txt` preserves the source notice.

`strategies.json` is the exact UTF-8 output of the Rust strategy compiler
(`zapret_core::compiler`), as written by:

```sh
cargo run -p zbuild --locked -- prepare-windows
```

It was first captured from the phase-0 Python importer; the Rust compiler reproduces
it byte for byte.

`winws-argv.json` records the final arguments, in catalog order, after the existing
writable user-domain-list substitution. `$BUNDLE` and `$DATA` stand for absolute
installation and service-data paths; separators are normalized to `/` for the
fixture only. Real Windows arguments keep conventional Win32 paths.

The Rust golden test checks input hashes, compiles every input and compares the
catalog with `strategies.json`, then checks catalog loading, payload-path
resolution (including spaces), user-list substitution and final argv. It creates
empty temporary payload files; it does not run winws. CI also runs `zbuild` against
the real pinned archive and compares the resulting `strategies.json` byte for byte.
