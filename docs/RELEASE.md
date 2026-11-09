# Release state — 2026-11-08

**Development/test builds are available. The commercial product is not finished.**

## Requirements and location

- Project: `/Users/macbook/Desktop/проекты/zapret`.
- User wants commercial quality, open source for now, Windows and macOS including TCP/UDP.
- User has a Windows computer and agreed to test there.
- Existing private repository: `mournami/zapret13231`, branch `codex/desktop-app`.
- Draft PR #1: https://github.com/mournami/zapret13231/pull/1. Nothing merged or publicly released.

## Verified build and runtime evidence

Code revision: `0e4c029b921d218ed850f96c0d8ccaa220becc1d`.
Full native CI run **42516443155 passed on both Windows and macOS**:
https://github.com/mournami/zapret13231/actions/runs/42516443155

- Frontend type check: 0 errors/warnings; production build passed.
- Rust core: 5 tests on macOS; Windows omits the Unix-only symlink test.
- Windows service cross-check and strict clippy passed locally.
- macOS Go engine, desktop integration and race tests passed.
- Windows NSIS installer built and installed in Windows Server 2022 CI.
- Real service IPC test passed, including rejection of invalid commands and actual
  winws/WinDivert start, status and stop. This is not a real ISP/video/voice test.
- Native Apple Silicon app, helper package, full `.pkg`, `.dmg` and app `.zip` built.
- Mac GUI opened; five actual HTTPS/STUN probes and native report export passed
  **without installing or running the packet engine**. Exported report was checked
  for omission of raw logs, user domains and address-bearing error details.
- Mac installer BOM confirmed root-owned executable files (0/0, mode 0755).

Local packages are in `outputs/`; copies of Windows `.exe`, Mac `.pkg`/`.dmg`,
SHA256 sums, BUILD-INFO.json, test instructions and screenshot are in this chat's
Codex outputs folder. Windows artifact ID **13631178771**, ZIP SHA256
`78d61c74d22f0976c7482f61e059950f19cfe7db95edea4e8a9579d42090bcd2` was verified.

## Material remaining work

- **Mac root packet path has not been tested.** A user-input request asks whether
  to open the concrete installer for administrator-authorized service installation
  and TCP/UDP tests. It is still pending. Do not treat elapsed time as authorization.
- `sudo -n -v` requires a password; no root service or PF/network change has been
  installed/applied on this Mac during the session. User must enter any password
  themselves in the native macOS window, never in chat.
- Real video playback, Discord voice, QUIC, network changes, sleep/wake, crash
  recovery and uninstall need the acceptance checks in `docs/ACCEPTANCE.md`.
- Intel Mac binary/runtime validation is incomplete; current local package is arm64.
- Signed automatic updates with health checks/rollback are not implemented.
  Signature-verification primitives exist; that is not an updater.
- Selector currently tests whole strategies with baseline checks and two successful
  rounds, cancellation and persisted recovery. Per-service composite compilation
  and automatic network-change repair are not finished.
- Publisher certificates/notarization are absent (0 local code-signing identities).
  Packages are unsigned/ad-hoc test builds.
- Exact corresponding-source/notice packaging review is pending for WinDivert,
  Cygwin and other dependencies. Do not describe the entire bundle as MIT.
- Full security review and physical-device acceptance remain release gates.
- npm audit recorded two low-severity development-only findings in the Svelte
  build plugin/deepmerge; npm offered no patched release at the time.

## Important fixes and investigation evidence

- Explicit UTF-8 for Windows strategy export; macOS CI moved off retired macos-14.
- Cygwin rejected Win32 canonical verbatim path prefixes. Canonical paths are still
  used for containment checks; only the CLI rendering is converted afterwards.
- Diagnostic branch `codex/windows-diagnostic` reproduced the failure and verified
  the real SYSTEM service after conventional path substitution (run 42516317747).
  Its workflow only runs on pushes to that branch, not on a schedule.
- Windows output is captured through pipes, bounded to 2 MiB; log-tail reads bounded.
- Disabling interception no longer depends on successfully saving a preference.
- Mac service/socket/anchor/hosts markers are isolated; fresh install disabled;
  enabled state persisted; upgrades preserve user host lists, including failed retries.
- Source synchronization used the GitHub connector because shell Git has no HTTPS
  credential. Local Git history matches connector-created commits. A future source
  update must materialize its local tree before importing a connector-created commit.

## UI redesign — 2026-11-08 (not yet verified in a native window)

- New interface: top tab bar instead of sidebar; overview built around a route
  diagram (computer → provider filter → YouTube / Discord / UDP / Cloudflare) that
  shows probe results and the on/off state; on/off is a persistent switch.
  Code: `ui/src/App.svelte`, `ui/src/store.svelte.ts`, `ui/src/components/*`,
  `ui/src/style.css`, `ui/src/Icon.svelte`. Tauri commands and arguments unchanged.
- Verified: `svelte-check` 0 errors / 0 warnings (run off-device with a stub for the
  `vite/client` CSS typing), every component compiles with the Svelte 5.57 compiler,
  and all states were rendered in Chrome against a mocked `invoke`
  (no service, off, on, checking, strategy selection, winner, failure, Windows,
  browser preview, 800 px window).
- Not verified: `npm run build` (Vite/rolldown native binaries were unavailable in
  the test environment) and the real Tauri window on macOS and Windows. Run
  `npm run check && npm run build` and open the app before handing off.

## Next step

The final native-UI attempt was blocked because the Mac screen is locked.
The user must unlock it manually. The installation decision is also still pending.

Wait for the existing Mac installation decision. If authorized, open the already
built installer, hand native password entry to the user, then verify the live
packet path and recovery before claiming functional macOS DPI circumvention.
Continue the unfinished product work and release gates afterwards; do not mark the
original commercial-ready objective complete from build/CI results alone.

## 2026-11-09 — redesigned UI builds and main integration

- Claude's existing commit `82c4011c2d942115c96ee8d29874c94d6f4a8466` was preserved.
- Both native platform builds passed in CI run 42529904118; Windows package
  installation and real service/WinDivert start-stop checks passed.
- Local frontend check reported 0 errors/warnings, production frontend build and
  Apple Silicon app/pkg/dmg build passed. The new top-tab/route-diagram UI was
  opened in the native Mac app and visually checked. No root installation or
  packet-path claim was made.
- Both CI artifact archives were downloaded with verified SHA256 and extracted
  into project outputs and chat outputs. BUILD-INFO.json records the source revision.
- At the user's explicit request PR #1 was merged into main, merge commit
  `670fd64eb1b2d6667de4a5fde8f2df16d703aac9`. Local checkout now tracks main.
- The commercial release gates above remain; merging the source is not a
  production release and the distributed packages remain unsigned development builds.
