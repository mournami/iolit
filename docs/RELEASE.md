# Release state — 2026-11-08

**Not production-ready. Do not publish as a fully verified commercial release.**

## User requirements

- Project directory: `/Users/macbook/Desktop/проекты/zapret`.
- Open source for now; commercial quality is required.
- Windows and macOS required, including all TCP/UDP service functions.
- User has a Windows machine and will perform Windows testing.

## Completed implementation

- Native Tauri/Svelte application with tray, service control, strategy selection,
  on-demand HTTPS/STUN checks, doctor, custom domains and local report export.
- Windows root service with a local-only named pipe and a kill-on-close job.
- Pinned Flowseal import: no upstream batch files are executed.
- Pinned, attributed macOS PF/BPF engine; GUI forces packet transport, not proxy.
- Mac desktop integration: isolated data/socket/guard names; first start disabled;
  enable/disable persisted; custom host list validation at both privilege boundaries.
- Helper `.pkg`, per-machine Windows installer hooks, build scripts.
- Signed manifest verification primitive. **Automatic update delivery is not yet implemented.**

## Verified so far

- Frontend type check: 0 errors, 0 warnings.
- Frontend production build: passed.
- Rust core tests: 4 passed (domain validation, traversal/symlink escape, signature tampering).
- Go engine and added desktop integration tests: passed without root.
- 22 pinned Windows strategies imported with payload existence checks.
- Native Apple Silicon Go engine/CLI built; macOS helper package built.

## Open gates

- Native GUI and bundle build / visual check: in progress.
- Windows service cross-check: in progress; full MSVC application build must run on Windows.
- Real macOS packet-path verification: blocked on administrator authorization;
  `sudo -n -v` reports that a password is required. No system service or PF changes
  have been installed/applied during this development run.
- Publisher identity: `security find-identity -v -p codesigning` found 0 identities.
  Current local packages are unsigned/ad-hoc development artifacts.
- Intel macOS build and runtime verification are not complete.
- Real YouTube playback, Discord voice, QUIC and network recovery not yet verified.
- Signed update distribution/rollback, telemetry-free support flow and code audit.
- License/source packaging review for redistributed WinDivert and engine dependencies.
- npm audit reports two low-severity development dependency findings in the Svelte
  build plugin/deepmerge; no patched release was offered by npm at this point.

Evidence must be updated after each check; do not convert skipped privileged tests
to a pass. Read `docs/ACCEPTANCE.md` before marking a production release complete.
