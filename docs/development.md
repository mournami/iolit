# Development

Iolit was called Zapret Desktop until 2026-11-17. New installations go to
`Program Files\Iolit`; installations made before that stay and are updated in
`Program Files\Zapret Desktop`. The service key `ZapretDesktop`, the data
folder and the crate names keep the old name for now.

How the repository is laid out and how to build and check it. For what the
application does and how to use it, see the [README](../README.md).

## Components

- `ui/`: Svelte 5 interface, Russian language, local assets only.
- `src-tauri/`: unprivileged Rust desktop shell: window, tray, file dialogs and the full report. All logic runs in the service.
- `crates/zapret-core/`: daemon API types, strategy compiler, composite, probes, bundle signatures, report masking. No OS calls.
- `crates/zapretd/`: Windows SCM service: strategy selection, checks, Doctor, bundle and app updates, secure DNS, winws supervision, protected local named pipe.
- `crates/zbuild/`: CI tool that compiles Flowseal `.bat` profiles and builds and signs bundles and app update manifests.
- `crates/zsetup/`: the Windows installer and uninstaller.
  `src/telegram/` is an optional local MTProto proxy for Telegram Desktop, a Rust
  port of the core of `Flowseal/tg-ws-proxy`.
- `engines/macos/`: MIT-licensed fork of `naladwepo/zapret-for-mac`, pinned to commit
  `763790eb671b7bc5d89399b8af46ebed70a39f0d`. Native PF-log/BPF packet transport;
  **no silent fallback to the TCP-only proxy**. Original attribution and tests retained.
- `resources/upstream.lock.json`: immutable versions and archive hashes.
- `packaging/`, `scripts/`: native installers and reproducible build preparation.


## macOS (frozen until phase 5)

Prerequisites: Rust stable, Node 22+, Go 1.26+, Apple command-line tools.

```sh
npm ci
python3 scripts/prepare_engines.py --platform macos
sh scripts/package_macos.sh
CC=/usr/bin/clang CXX=/usr/bin/clang++ npm run desktop:dev
```

The app opens without privileges. Packet processing only becomes available after
installing the included service package. macOS Installer asks for administrator
authorization. A fresh install starts with packet processing **disabled**.

Build distributable app and disk image:

```sh
CC=/usr/bin/clang CXX=/usr/bin/clang++ npm run desktop:build -- --bundles app
sh scripts/distribute_macos.sh
```

For a signed build set `APPLE_SIGNING_IDENTITY` for Tauri and
`ZAPRET_INSTALLER_IDENTITY` for the helper package. Signing/notarization must use
the publisher's real Apple Developer account. No signing secrets are stored here.

## Windows build

Run in Windows 10/11 x64 with Rust MSVC, Node and Visual Studio Build Tools:

```powershell
./scripts/build_windows.ps1
```

On macOS or Linux, `sh scripts/build_windows_cross.sh` cross-builds the same
installer with `cargo-xwin` (on this repository's macOS setup also set
`CC=/usr/bin/clang CXX=/usr/bin/clang++`). Both write
`outputs/Iolit-Setup.exe`.

The installer is our own Rust program (`crates/zsetup`), the only installer we
ship (no NSIS, MSI or Inno). One UAC prompt, then a small window in the app's
style (WebView2 page embedded in the exe, `crates/zsetup/ui/index.html`)
installs at once with a progress bar and opens the app; there are no pages or
choices. It installs to a fixed folder in Program Files with a per-machine
Windows service and Start menu and desktop shortcuts; the GUI itself is not
elevated. On failure the window shows the step and error with Retry and Copy.
Without WebView2 it offers the Microsoft download page. `--quiet` installs
without a window and without opening the app, printing each step and its
duration to the calling console. Source profiles are imported without running upstream batch scripts:
`zbuild` compiles the pinned Flowseal `.bat` files with the Rust strategy
compiler. Windows on ARM is not supported by this WinDivert build.

## Checks

```sh
npm run check
npm run build
cargo test -p zapret-core
cargo check -p zapretd --target x86_64-pc-windows-msvc
cd engines/macos && go test ./... && go test -race ./internal/...
```

Privileged packet-path tests are separate from unit tests and must be run on real
machines. A successful TLS/STUN probe does not prove video playback or a Discord
voice call. Release criteria are checked on real machines before each release.

## Recovery and uninstall

On macOS, stop the engine in the GUI. If the GUI is unavailable, the guard flushes
only this application's PF anchor after the daemon dies. Manual recovery:

```sh
sudo "/Library/PrivilegedHelperTools/io.zapret.desktop.engine" guard \
  --data "/Library/Application Support/Zapret Desktop" --anchor zapret-desktop
```

`scripts/uninstall_macos.sh` unregisters the service and guard without deleting
user data. On Windows, use the ordinary installed-apps uninstaller (it runs
`uninstall.exe --uninstall` in the same window, with one confirmation; add
`--quiet` for no window). Data is retained
under `%ProgramData%/ZapretDesktop`.

