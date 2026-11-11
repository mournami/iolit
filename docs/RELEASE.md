# Release state — 2026-11-10

**The required phase-0 Windows CI job passed on 2026-11-10.**
**This is a development build; manual Windows acceptance remains unverified.**

## Current scope and checkout

- Source of truth: `docs/ARCHITECTURE.md`, architecture v2, phase 0 only.
- Project: `/Users/macbook/Desktop/проекты/zapret`, branch `codex/desktop-app`.
- Repository: `mournami/zapret13231` (private), source open for now.
- The initial phase-0 session started on local main `05340c4`; the existing desktop branch was
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
The initial local evidence below predates native Windows CI. The follow-up
section records the successful Node 24 / Windows Server 2022 run.

Before each initial phase-0 commit the applicable AGENTS.md checks were run locally:

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
- Native Windows compilation, installer installation and real IPC/WinDivert now
  passed in the follow-up CI run below. GUI-close selection continuity and
  service-restart recovery still require manual Windows testing; controlled unit
  tests do not establish those physical-device results.
- Whole-strategy behavior is covered with controlled tests, not a claim of identical
  physical Windows behavior or successful ISP circumvention after this refactor.
- YouTube playback, Discord voice, QUIC, provider diversity, sleep/wake, conflicts,
  crash/uninstall acceptance, publisher signing and full security/license review
  remain unverified release gates. `docs/ACCEPTANCE.md` is not a completed checklist.
- Local `outputs/` binaries predate phase 0 and were not rebuilt in this follow-up.
  The current Windows installer is the Actions artifact linked below. No publisher
  signing or automatic updater was added.

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
  daemon-owned selection. The initial local phase-0 session performed no push, merge
  or release. The later branch-only push and CI are recorded below.

## 2026-11-10 — authorized branch push and native Windows CI

The owner authorized pushing only to `codex/desktop-app`, with no main push,
merge, release or phase-1 work. The existing phase-0 commits were pushed intact.
The required job passed on its **first attempt**. No Windows step failed, no retry
was needed, and **no source-code or workflow fix commit was required**. Product
behavior and the mandatory checks were not changed to obtain this result.

- Run: https://github.com/mournami/zapret13231/actions/runs/42841680373
- Tested source: `5efa2390e36ed2734faca4b0c7a8076a7fc58a77`.
- Required job: **Windows checks and service IPC**, ID `129918256319`,
  conclusion **success**. The workflow also completed with conclusion success.
- Optional frozen macOS job: **skipped**, as intended for a branch push; no macOS
  build/runtime result is claimed for this follow-up.

| Native Windows check | Actual result |
| --- | --- |
| `cargo fmt --all --check` | Passed |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | Passed |
| `cargo test --workspace --locked` | 13 passed, 0 failed; installed-service test initially ignored as configured |
| Contract generation and `git diff --exit-code` | Passed, no generated-file drift |
| `npm ci`, `npm run check`, `npm run build` | Passed; svelte-check 0 errors / 0 warnings |
| Pinned Flowseal import and golden comparison | Passed, 22 strategies |
| Release service and NSIS desktop build | Passed |
| Silent installation of the generated NSIS package | Passed |
| Explicit installed-service test (`--ignored --test-threads=1`) | 1 passed, 0 failed, 0 ignored |

The explicit integration test verified fresh-install disabled state, catalog and
snapshot, rejection of invalid commands/domains/paths without state changes,
actual winws/WinDivert enable, running state, disable, final stopped state and
report retrieval. This is **14 passing Windows test executions in total** across
the workspace and installed-service steps, not an ISP/video/voice test. Full job
logs were inspected to confirm the integration test actually ran and was not
merely skipped.

Installer artifact: [Zapret-Desktop-Windows-x64-DEVELOPMENT](https://github.com/mournami/zapret13231/actions/runs/42841680373/artifacts/13777651209),
artifact ID `13777651209`, archive digest reported by GitHub:
`sha256:bca69ab4638fb3fdd0c95d5410dcfa54f38e73d0a0f76e7876c8524ceca41165`.
It remains an unsigned development package; no release was published. Local
installer bytes were not downloaded or independently rehashed in this follow-up.

Before committing this report, the five local AGENTS.md commands listed above
were rerun and passed (15 macOS Rust tests; frontend 0 errors / 0 warnings).
Local logs: ignored `work/phase0/ci-followup-docs-*.log`. These local checks are
separate from the native Windows evidence above. The follow-up change is
restricted to this report and the manual checklist below.

## Manual Windows checklist (not executed)

Use the installer artifact from the successful run above. Record Windows version,
provider, tested strategy and pass/fail for each item. These checks are not implied
by CI success.

- [ ] **Install:** on a clean Windows 10/11 x64 machine, install the development
  NSIS package with UAC, then run the GUI as a standard user. The service is
  available, strategies are listed, and interception starts disabled.
- [ ] **Enable/disable:** enable and verify the GUI reports running and winws.exe
  appears in Task Manager; disable and verify it stops. Repeat the cycle and
  confirm ordinary internet access remains available.
- [ ] **Selection after GUI exit:** click «Подобрать стратегию». While it is busy,
  exit through the tray's «Выйти из приложения» and confirm the GUI process closes.
  Reopen it during selection: progress must continue without restarting; wait for
  a winner, baseline notice or a clear failure with the original state restored.
  Separately cancel an active selection and check restoration. A baseline that
  already passes may finish before the window can be closed; that alone does not
  verify continuity of an active operation.
- [ ] **Reboot:** test reboot with interception enabled, then with it disabled.
  The chosen strategy and enabled/disabled preference must survive separately.
  Also reboot during a selection and verify recovery restores the state from
  before selection or reports an explicit recovery failure.
- [ ] **YouTube:** with the selected strategy, play a real video for 10 minutes,
  change quality and seek. Record stalls or playback failures.
- [ ] **Discord:** confirm sign-in/messages, then join a real voice channel and
  verify bidirectional audio for at least 5 minutes. A successful STUN probe alone
  does not confirm Discord voice.

## Next step

The Russian phase-0 report is in `docs/PHASE-0-REPORT.md`. Before committing the
report, local fmt, strict workspace clippy, workspace tests (15 passed), frontend
check (0 errors / 0 warnings) and frontend build passed again. This publication
changes documentation only; Windows CI evidence above refers to its tested revisions.

Run the manual Windows checklist and record the actual results. The required
Windows CI exit criterion has passed; physical-device and commercial release
acceptance remain pending. Phase 1 has not been started and is outside this
session's scope. No main push, merge or release was performed.

## 2026-11-11 — phase 1 start: Rust strategy compiler and zbuild (local only)

Requested by the user as the first phase-1 task. Not pushed; no CI run exists for
these commits. Host: Arch Linux x86_64, rustc/cargo 1.95.0, Node 22.22.2.

- `754c7d1` `zapret_core::compiler`: the single `.bat` parser. Reads the one
  `start ... winws.exe` line as text, never executes it. Rejects syntax the Python
  importer tolerated: several winws launches, unclosed quotes, empty `--new`
  profiles, duplicate ids, non-UTF-8 input.
- `a43f643` `crates/zbuild` (`prepare-windows`): ports the Windows part of
  `scripts/prepare_engines.py`. Extraction accepts only regular files and
  directories inside the destination; `work/flowseal` is emptied first.
- `48df320` `build_windows.ps1`, `npm run prepare:engines` and the Windows CI job use
  zbuild; Python is kept only for the frozen macOS build. The golden fixtures no
  longer pin the Python importer hash.

| Check (Linux) | Result |
| --- | --- |
| `cargo fmt --all --check` | Passed |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | Passed |
| `cargo test --workspace --locked` | 20 passed, 0 failed (3 compiler, 2 zbuild tests new) |
| Golden test: compile all 22 pinned `.bat`, compare with `strategies.json` | Byte-identical |
| `zbuild prepare-windows` on the real pinned 1.10.4 archive | 22 strategies; `strategies.json` byte-identical to golden |
| `diff -r` of `resources/runtime` from Python importer vs zbuild | Identical |
| `npm run prepare:engines` (now zbuild) | Passed, output matches golden |
| `npm run check` / `npm run build` | 0 errors / 0 warnings; build passed |
| Workflow YAML parse (PyYAML) | Passed; not a GitHub Actions run |

Not verified: the Windows CI job with these changes (needs a push), zbuild on
Windows (path handling, CRLF checkout of the golden JSON is pinned to LF by
`.gitattributes`), installer build and installed-service test after the switch.
Phase-0 unverified gates listed above are unchanged.
