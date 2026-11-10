# Flowseal 1.10.4 snapshot

The 22 `general*.bat` inputs are copied byte-for-byte from the release pinned in
`resources/upstream.lock.json` (archive SHA256 recorded in `source.json`). They are
never executed. `LICENSE-flowseal.txt` preserves the source notice.

`strategies.json` is the exact UTF-8 output of the existing, unchanged importer:

```sh
python3 scripts/prepare_engines.py --platform windows
```

`winws-argv.json` records the final arguments, in catalog order, after the existing
writable user-domain-list substitution. `$BUNDLE` and `$DATA` stand for absolute
installation and service-data paths; separators are normalized to `/` for the
fixture only. Real Windows arguments keep conventional Win32 paths.

The Rust golden test checks input hashes, the importer hash, catalog loading,
payload-path resolution (including spaces), user-list substitution and final argv.
It creates empty temporary payload files; it does not run winws or a batch parser.
The Python importer itself remains phase-0 build tooling. CI reruns it and compares
the resulting JSON with this snapshot after CRLF-to-LF normalization (Python
`write_text` uses native line endings). Replacing it with the single
Rust batch compiler and zbuild belongs to phase 1.
