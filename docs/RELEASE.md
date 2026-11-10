# Release state — 2026-11-10

**Phase 0 implementation is local. Its Windows CI exit gate is not verified.**
**This is not a production release. No push was performed in this session.**

## Current scope and checkout

- Source of truth: `docs/ARCHITECTURE.md`, architecture v2, phase 0 only.
- Project: `/Users/macbook/Desktop/проекты/zapret`, branch `codex/desktop-app`.
- Repository: `mournami/zapret13231` (private), source open for now.
- The session started on local main `05340c4`; the existing desktop branch was
  advanced to that commit with a local fast-forward, preserving previous work.
- Owner-supplied AGENTS.md and ARCHITECTURE.md were adopted unchanged in the first
  session commit, `83a3f65` (`Adopt architecture v2 and phase plan`).
- Windows is the release priority. Routers/Linux, Android and macOS remain later
  phases. No phase-1 features, paid services, signing keys or privileged installs
  were added. Frozen `engines/macos/` and the Python importer were not modified.

## Phase 0 changes

- `zapret-core::api` defines the fixed tagged commands, response envelope, snapshot,
  operation, probes, Doctor and privacy-allowlisted report. Existing UI field names
  were retained. Unknown commands and extra executable/path fields are rejected.
- `cargo run -p zapret-core --bin gen-contracts --locked` produces
  `contracts/daemon-api.schema.json` and `ui/src/generated/api.ts` using schemars
  and ts-rs. `ui/src/types.ts` only reexports the generated types.
- Existing selection code moved from Tauri into `zapretd::{operation,selector}`:
  baseline without interception, ordered whole-strategy candidates, two successful
  rounds, cancellation and rollback. Selection remains whole-strategy in phase 0.
- The daemon owns the operation, cancellation flag and gate. Snapshot includes the
  operation. Probes do not hold the operation or controller lock for their duration.
  Closing the GUI no longer owns or cancels the operation.
- Selection recovery moved to service data:
  `%ProgramData%\ZapretDesktop\selection-recovery.json`. Startup attempts recovery
  before restoring the enabled preference. Failed rollback retains the file and
  exposes an error. Old per-user GUI recovery files are not imported by SYSTEM.
- `src-tauri` forwards each action as one daemon request; it only handles native
  shell concerns, unavailable-service presentation, installation and report file
  writes. Report paths never reach the daemon; the `.json` extension is checked.
- Existing winws launch, Cygwin-compatible paths, Job Object cleanup, bounded pipe
  logging and startup checks moved into `engine/winws.rs` behind `Engine`.
  Windows SCM and named-pipe security moved into `platform/windows.rs`.
- `ui/src/api.ts` uses `transport.call`. Tauri is the active native transport.
  The HTTP transport prepares POST `/api/<command>` with a Bearer token, refuses
  desktop-only actions and redirects, and is not connected to the UI. No HTTP
  server, router adapter or browser system-control flow was added.
- Golden fixtures preserve all 22 `general*.bat` files from pinned Flowseal 1.10.4,
  their hashes, exact imported JSON and normalized final winws argv. Tests exercise
  the actual catalog loader, safe payload resolution and writable-list overlay.
  Batch files are never executed. There is no Rust batch parser yet.
- Windows CI now requires fmt, strict workspace clippy, workspace tests, generated
  contract diff, frontend check/build, pinned importer comparison, installer build
  and the real installed-service/WinDivert IPC test. macOS packaging is optional,
  non-blocking and only runs on workflow_dispatch with the macos input selected.
  No repository branch-protection settings were changed.

## Actual local validation

Host: Apple Silicon macOS; rustc/cargo 1.96.0, Node 26.13.0, npm 11.20.0.
CI is configured for Node 24 and native Windows; that environment was not run.

Before every session commit the applicable AGENTS.md checks were run locally:

```sh
cargo fmt --all --check
CC=/usr/bin/clang CXX=/usr/bin/clang++ cargo clippy --workspace --all-targets -- -D warnings
CC=/usr/bin/clang CXX=/usr/bin/clang++ cargo test --workspace
npm run check
npm run build
```

- Checkpoints 01–09: all five commands passed before the respective commit.
  Logs are retained locally in ignored `work/phase0/`; they are not CI evidence.
- Final workspace tests: **15 passed** on macOS. This includes six controlled
  selection/operation tests, two daemon API/platform tests, API rejection tests,
  existing core tests and the 22-strategy golden test. The Windows-only installed
  service test has **zero runnable tests on this host**; it was not counted as passed.
- Controlled tests cover disabled baseline, two confirmations, cancellation during
  confirmation, exhausted candidates, startup failure, failed restoration retaining
  recovery, subsequent recovery and daemon ownership after dropping a caller.
  API tests cover preference persistence, the mutation gate, readable snapshot,
  report privacy and stopping even if saving the disabled preference fails.
- Frontend: **0 errors / 0 warnings**, production Vite build passed. Components,
  route diagram, styling and Russian UI strings were not redesigned.
- Contract regeneration followed by `git diff --exit-code --
  contracts/daemon-api.schema.json ui/src/generated/api.ts`: passed, no drift.
- Existing `python3 scripts/prepare_engines.py --platform windows`: passed;
  imported **22** strategies from the SHA256-verified pinned archive. Local
  normalized-text comparison with golden strategies.json passed. The Windows CI
  comparison normalizes CRLF to LF because Python write_text uses native endings.
- HTTP transport assertions using Node and mocked fetch passed: endpoint, POST,
  Bearer header, request body, error propagation, redirect refusal and rejection of
  desktop-only actions. No actual HTTP daemon or network request was involved.
- Ruby YAML parsing of `.github/workflows/build.yml` passed. This is syntax parsing,
  not a GitHub Actions run or proof that the Windows job is green.
- `cargo clippy -p zapret-core --no-default-features --all-targets
  --target x86_64-pc-windows-msvc --locked -- -D warnings`: passed after removing a
  redundant Result wrapper caught by Windows-target clippy. This checks core API,
  loader and IPC compilation only, without probes or a running service.
- `cargo run -p zapretd -- --console` on macOS exited **1** with the explicit
  unsupported-platform error, as expected. It started no service or packet engine.

### Failures and warnings observed

- The initial host clippy attempt failed in the Nix C compiler wrapper on this
  Cyrillic project path. Retrying with `/usr/bin/clang` and `/usr/bin/clang++`
  passed. No product behavior or dependency source was changed to work around it.
- Full daemon cross-clippy for `x86_64-pc-windows-msvc` could not compile ring's C
  code: Windows SDK headers are absent (`assert.h`, then `stdlib.h` in a separate
  freestanding attempt). Neither attempt verified the daemon's Windows code.
- ts-rs prints an unsupported-serde-attribute warning for `deny_unknown_fields`.
  That attribute governs Rust deserialization, not TypeScript representation;
  it remains enabled and its rejection behavior is tested. No warning was hidden.

## Regressions and unverified gates

- **macOS backend controls are unavailable in this Rust API revision**, including
  selection, probes, enable/disable, Doctor and report. The GUI presents an
  unavailable service and requests return an explicit unsupported-platform error.
  No compatibility layer was added to the frozen Go daemon. Its source and old
  packaging remain, but installing it does not add the new Rust API. macOS adapter
  work resumes in phase 5; no TCP-only proxy fallback exists.
- The current UI was not opened in a native window during this phase-0 session.
  Frontend build/type checks and Rust compilation do not replace native UI tests.
- New Windows native compilation, installer installation, real IPC/WinDivert,
  GUI-close selection continuity and service-restart recovery were **not run on
  Windows**. The updated required CI job is committed but has not run: the owner
  explicitly prohibited push in this session. Phase 0's exit gate remains pending.
- Whole-strategy behavior is covered with controlled tests, not a claim of identical
  physical Windows behavior or successful ISP circumvention after this refactor.
- YouTube playback, Discord voice, QUIC, provider diversity, sleep/wake, conflicts,
  crash/uninstall acceptance, publisher signing and full security/license review
  remain unverified release gates. `docs/ACCEPTANCE.md` is not a completed checklist.
- Existing `outputs/` binaries predate phase 0. They were not rebuilt or represented
  as artifacts of this revision. No new publisher signing or automatic updater exists.

## Historical evidence (earlier source, not phase 0)

- Native Windows and macOS CI run 42516443155 passed for revision `0e4c029`:
  https://github.com/mournami/zapret13231/actions/runs/42516443155
- Redesigned native packages passed in run 42529904118 for `82c4011`:
  https://github.com/mournami/zapret13231/actions/runs/42529904118
  That Windows run installed the package and exercised real winws/WinDivert start,
  status and stop; it was not an ISP/video/voice test.
- The redesigned Mac window was previously opened without installing the privileged
  packet engine. No local Mac root packet-path validation was performed.
- PR #1 was merged at the owner's request before this session, merge `670fd64`;
  main was then `05340c4`. These historical results do not validate the new API or
  daemon-owned selection. There was no push, merge or release in this session.

## Next step

With a separate owner request permitting push, run the committed Windows job and
resolve any native failure while preserving behavior. Do not mark phase 0 complete
or start phase 1 until its Windows CI exit gate passes.

The proposed first phase-1 task is the single Rust batch compiler and zbuild:
reproduce the current 22 golden imports and argv, reject unknown batch syntax,
then replace the existing importer. Per-service selection, composites, periodic
Doctor, bundle updates and other phase-1 features have not been started.
