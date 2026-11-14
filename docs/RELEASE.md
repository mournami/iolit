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

## Manual Windows acceptance (not executed)

Acceptance is split in two. The owner has no Russian ISP connection and runs the
technical part on their own Windows machine; ISP bypass can only be checked on a home or mobile
connection inside Russia (datacenter VPN/VPS addresses do not pass through TSPU the
same way). Russian checks stay open until external testers report; they are never
marked passed on the basis of technical or CI results.

Use the installer artifact from the latest successful Windows CI run. Record Windows
version, build/commit, tested strategy and pass/fail for each item.

### Technical (no Russian ISP needed; done by the owner)

- [ ] **Install:** on a clean Windows 10/11 x64 machine, install the development
  NSIS package with UAC, then run the GUI as a standard user. The service is
  available, strategies are listed, and interception starts disabled.
- [ ] **Enable/disable:** enable and verify the GUI reports running and winws.exe
  appears in Task Manager; disable and verify it stops. Repeat the cycle and
  confirm ordinary internet access remains available.
- [ ] **Selection after GUI exit:** outside Russia the unbypassed baseline usually
  passes, so selection ends immediately. To keep it busy, temporarily add
  `0.0.0.0 www.youtube.com` to `C:\Windows\System32\drivers\etc\hosts`: every
  candidate then fails and selection walks the whole list. Click «Подобрать
  стратегию», exit through the tray's «Выйти из приложения» while it is busy and
  confirm the GUI process closes. Reopen it: progress must continue without
  restarting and end with a clear failure and the original state restored.
- [ ] **Cancel:** with the same hosts entry, start selection and cancel it; the
  previous strategy and enabled/disabled state must be restored.
- [ ] **Reboot:** reboot with interception enabled, then with it disabled. The
  chosen strategy and enabled/disabled preference must survive separately. Also
  reboot during a selection (hosts entry still present) and verify recovery restores
  the state from before selection or reports an explicit recovery failure.
- [ ] Remove the hosts entry afterwards and confirm YouTube opens normally.

### Russian ISP only (pending external testers)

- [ ] **YouTube:** with the selected strategy, play a real video for 10 minutes,
  change quality and seek. Record stalls or playback failures.
- [ ] **Discord:** confirm sign-in/messages, then join a real voice channel and
  verify bidirectional audio for at least 5 minutes. A successful STUN probe alone
  does not confirm Discord voice.
- [ ] **QUIC:** YouTube over QUIC/HTTP3 works with the selected strategy.
- [ ] **Providers:** the above on 3+ different Russian providers, including at
  least one mobile network. This is the phase-1 exit criterion.

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

Requested by the user as the first phase-1 task. Pushed to `codex/desktop-app` by
the collaborator on 2026-11-11 (head `7500872`). The Windows CI result for `7500872`
is **not confirmed**: the follow-up session had no GitHub Actions access (no `gh`,
no signed-in browser). Check the Actions tab for that commit before relying on it. Host: Arch Linux x86_64, rustc/cargo 1.95.0, Node 22.22.2.

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

## 2026-11-11 — phase 1: signed strategy bundle in zbuild (branch `phase1/zbuild-release`)

Local commits only; not pushed, no CI run. Host: Apple Silicon macOS, rustc/cargo
1.96.0, Node 26.13.0. Cargo C builds use `CC=/usr/bin/clang CXX=/usr/bin/clang++`
because of the Cyrillic project path (see phase 0).

- `zapret_core::bundle::Manifest`: schema 1, `version` (`YYYY.MM.DD.HHMM`, numeric
  parts), Flowseal tag, archive name and SHA-256, every file with its SHA-256.
  Unknown fields, bad paths, duplicates and malformed hashes are rejected.
  `contracts/bundle-manifest.schema.json` is generated by `gen-contracts`; the
  Windows CI diff step now covers it.
- No separate bol-van version field: on Windows the engine binaries ship inside the
  Flowseal release and are not pinned separately; their hashes are in `files`.
- `zbuild bundle` packs the prepared `resources/runtime/windows` and `licenses`
  into a deterministic `bundle-<version>.tar.gz` (sorted, fixed metadata) and an
  unsigned manifest. The empty `*-user.txt` placeholders are included because
  strategies reference them with `=@`; real user domains stay in service data.
- `zbuild sign` signs the exact manifest bytes with ed25519; the key comes only
  from `ZAPRET_BUNDLE_SIGNING_KEY` (base64 of a 32-byte seed). Without it the
  command fails with «нужен ключ подписи». `zbuild public-key` prints the public key.
- `zbuild verify` is the client-side check: signature, archive hash, safe
  extraction, every file hash, no unlisted files. `zbuild changes` writes release
  notes (strategies and files added/removed/changed against the pinned import).
- `zbuild prepare-windows` accepts `--flowseal-url/--flowseal-sha256` for a newer
  release, caches archives per digest and now rebuilds `resources/runtime/windows`
  from scratch, so files from an earlier import cannot leak into a bundle.
- `.github/workflows/bundle.yml`: hourly + manual. Takes the latest Flowseal release
  and the asset SHA-256 reported by GitHub; builds only if it is newer than the pin
  and has no bundle yet (or `force`). Runs golden tests, compares the pinned import,
  prepares the new release (any parse error fails the job), builds, signs, verifies
  and creates a **draft** release `bundle-<version>`. Without the
  `BUNDLE_SIGNING_KEY` secret it fails before building and never uses a test key.
  Upstream values reach shell scripts only through environment variables.

| Check (macOS) | Result |
| --- | --- |
| `cargo fmt --all --check` | Passed |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | Passed |
| `cargo test --workspace --locked` | 28 passed, 0 failed (2 bundle, 7 release tests new) |
| `npm run check` / `npm run build` | 0 errors / 0 warnings; build passed |
| `gen-contracts`, then `git status` | Only the new schema file appeared; daemon contracts unchanged |
| Real pinned 1.10.4: `prepare-windows` → `bundle` → `sign` (throwaway local key) → `verify` | 39 files, 1.5 MB archive, verified (42 after keeping the list placeholders); `strategies.json` byte-identical to golden |
| `sign` without the key | Exit 1, «нужен ключ подписи» |
| Release-check step run locally with `gh api` replaced by `curl` | Latest 1.10.4 = pin: skipped; with force: build; GitHub digest equals the pinned SHA-256 |
| Workflow YAML parse (Ruby) | Passed; not a GitHub Actions run |

Tests use a seed marked test-only; its public key exists only in the tests. No
production key was generated.

Not verified: any GitHub Actions run of `bundle.yml`, draft creation with
`GITHUB_TOKEN`, the Windows CI job on this branch, and `zbuild` on Windows. GitHub
runs scheduled workflows and offers the manual button only for workflows on the
default branch, so `bundle.yml` stays inactive until it reaches `main`. The owner
must add the `BUNDLE_SIGNING_KEY` secret. Bundle download and switching in
`zapretd` is the next task and is not started.

## 2026-11-11 — phase 1: signed bundle updates in zapretd (branch `phase1/bundle-update`)

Local commits on top of `phase1/zbuild-release`; not pushed, no CI run.
Host: Apple Silicon macOS, rustc/cargo 1.96.0, Node 26.13.0.

- Fix on `phase1/zbuild-release` (`85dec4a`): bundles keep the empty
  `*-user.txt` placeholders. Strategies reference them with `=@`, so excluding them
  would have made every updated strategy fail to start.
- Archive packing, safe extraction, the unlisted-file check and numeric version
  comparison moved from zbuild into `zapret_core::bundle`; zbuild and the service
  run the same verification code.
- `zapretd::updater`: first check 2 minutes after start, then every 6 hours or on
  `trigger` (for Doctor, which does not exist yet). Steps: manifest from each
  source in order → signature with the built-in key → newer than current and not
  rejected → archive (size-limited, https only) → archive hash → extraction into a
  staging folder → every file hash, no unlisted files → rename to
  `bundles/<version>` → under the operation gate: probes, switch, restart, probes
  again → if fewer probes pass, switch back and never retry that version. A bundle
  whose engine fails to start is restored and rejected the same way.
- Bundles live in `<install>/bundles`, not `%ProgramData%`: standard users may add
  files to ProgramData subfolders, and winws runs as SYSTEM and loads DLLs from its
  own folder. User domains stay in `%ProgramData%\ZapretDesktop\user-domains.txt`
  and are never touched. The current and previous bundles are kept, older removed.
- At startup the current bundle is verified again against its stored signed
  manifest; on failure the installer's bundle is used and the reason is kept in the
  bundle status.
- The public key comes from `ZAPRET_BUNDLE_PUBLIC_KEY` at build time; Windows CI
  passes the `BUNDLE_PUBLIC_KEY` repository variable. Without it updates are off and
  downloaded bundles are ignored. Sources: `<install>/bundle-sources.json`
  (https base URLs) or GitHub Releases `latest/download/`.
- `Snapshot.bundle` (version, Flowseal tag, last check/update, error); the overview
  shows one line under «Стратегия», e.g. «Стратегии обновлены 2 ч назад».

| Check (macOS) | Result |
| --- | --- |
| `cargo fmt --all --check` | Passed |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | Passed (one `large_enum_variant` allow on `DaemonResponse`, with a reason) |
| `cargo test --workspace --locked` | 39 passed, 0 failed; 10 new updater tests |
| Updater tests (fake engine, fetcher and probes; real signatures and archives) | Update and restart, bad signature, bad archive hash, missing and truncated download, rollback on worse probes, engine start failure, tampered bundle at startup, build without key, pruning, busy gate with download reuse |
| `npm run check` / `npm run build` | 0 errors / 0 warnings; build passed |
| Contract regeneration | Daemon schema and `api.ts` updated with `BundleStatus` |

Not verified: Windows compilation of the new `platform/windows.rs` wiring (no
Windows SDK here; CI must check it), any real download from GitHub or a mirror,
behaviour of Windows file locks and Defender during rename and pruning, the UI line
in a real window, and an end-to-end update on a Windows machine. The repository is
private, so clients cannot download from GitHub Releases until it is public or a
mirror is configured. When the app itself is updated with a newer Flowseal, a
previously downloaded bundle still counts as newer (the installer's bundle has no
version yet). Network-change triggers are not implemented.

## 2026-11-13 — first Windows install by a tester (branch `phase1/tester-fixes`)

The installer was cross-built on macOS (`cargo-xwin`, Homebrew `makensis`/LLVM,
`tauri build --runner cargo-xwin --target x86_64-pc-windows-msvc --bundles nsis`)
from `phase1/bundle-update` and installed on a tester's Windows 11 x64 PC. This
was the first Windows compile of the updater wiring in `platform/windows.rs`; it
compiled. `makensis` crashes with `std::bad_alloc` when the locale is empty; set
`LANG=en_US.UTF-8`. Cross-compilation is marked experimental by Tauri and the
installer is unsigned.

Reported by the tester (photos and a saved report, no logs):

1. The installer showed «Создание папки: C:\ProgramData\ZapretDesktop» for a long
   time, then finished. Cause found: the service hooks run last and `nsExec` prints
   nothing, so that line stayed while `icacls` and `sc create/start` ran. Which
   step was slow is not known (the first start of an unsigned service binary is a
   likely candidate, not confirmed).
2. Strategy search stopped at 16 of 22 with «winws завершился при запуске: exit
   code: 1 … could not read ^!». Cause: `general (FAKE TLS AUTO)` passes
   `--dpi-desync-fake-tls=^!`; for cmd, a caret outside quotes makes the next
   character literal, so winws must get `!` (built-in fake). The Rust compiler and
   the golden fixtures (taken from the phase-0 Python importer) kept `^!`. The
   frozen macOS converter already handled it. Second cause: any engine start
   failure aborted the whole search.
3. After the aborted search, `general` was restored: Cloudflare and UDP passed;
   YouTube, Discord API and the gstatic check failed. HTTPS failures showed only
   «error sending request for url», so the reason (reset, timeout, TLS) is unknown.

Changes:

- Compiler: caret escapes follow cmd rules outside quotes (`^!` → `!`,
  `^^` → `^`); unquoted `& | < >`, escaped quotes, `^^` before a line break and
  `!` under `EnableDelayedExpansion` are rejected. Golden `strategies.json` and
  `winws-argv.json`: only the 4 `--dpi-desync-fake-tls` lines of
  `general-fake-tls-auto` changed.
- Strategy search: a candidate whose engine does not start is skipped; the winner
  notice («Не запустились и пропущены: …») or the final error lists skipped
  strategies. Service and IPC errors still abort with rollback.
- Checks: HTTPS failures name the cause (reset, aborted, refused, timeout, closed
  before answer, DNS, certificate) and the error chain. A local run against closed,
  unresolvable, expired-certificate and unroutable targets showed the expected
  texts without IP addresses.
- Installer: each service step prints its own line; the 1.5 s stop delay runs only
  when a running service was stopped.

| Check (macOS) | Result |
| --- | --- |
| `cargo fmt --all --check` | Passed |
| `cargo clippy --workspace --all-targets -- -D warnings` | Passed |
| `cargo xwin clippy -p zapretd -p zapret-core --target x86_64-pc-windows-msvc --all-targets -- -D warnings` | Passed |
| `cargo test --workspace` | 42 passed, 0 failed (new: caret escapes, skipped candidates, failure texts) |
| Golden import | Passed with the corrected fixtures |
| `npm run check` / `npm run build` | 0 errors / 0 warnings; build passed |
| Cross-built NSIS installer with new hooks | Built (4.66 MiB), not installed yet |

Not verified: the fixed installer on Windows, `general (FAKE TLS AUTO)` actually
starting under winws v72.9, a full 22-strategy search on the tester's ISP, why
YouTube/Discord/gstatic failed with `general`, and which installer step was slow.
Windows CI has not run on this branch.

## 2026-11-13 — Rust installer replaces NSIS (branch `phase1/tester-fixes`)

Owner decision: ship our own Windows installer in Rust (`crates/zsetup`,
recorded in `docs/ARCHITECTURE.md`). Trigger: on the tester's PC the NSIS
installer, run over the earlier install, stopped with «Невозможно открыть файл
для записи: …\runtime\windows\bin\WinDivert64.sys». Cause: winws loads the
WinDivert driver, and WinDivert.dll leaves it loaded (marked for deletion) after
winws exits, so the `.sys` stays busy even with our service stopped. The
workaround given to the tester: `sc stop WinDivert`, then «Повтор», or reboot.

`zapret-setup.exe` (published as `Zapret-Desktop-Setup.exe`):

- Console window, UAC prompt from the embedded manifest
  (`requireAdministrator`, checked in the built exe), fixed folder
  `Program Files\Zapret Desktop`; paths from known folders, not environment
  variables. Each step prints its duration.
- Stops the service, ends running copies of the app, stops the `WinDivert`
  driver service only when its image is inside our folder. Busy files are
  renamed and deleted at reboot instead of failing. Files an update dropped
  are removed using `install.json`.
- Creates `%ProgramData%\ZapretDesktop` with owner Administrators and a
  protected SYSTEM/Administrators ACL; an existing folder gets the same
  owner and ACL; a link or file in its place, or a folder whose ACL cannot be
  reset, is moved aside. Files planted inside a folder created earlier by a
  standard user are not re-checked (same as the NSIS hooks).
- Registers or updates `ZapretDesktop` (same name, description and restart
  policy as the NSIS hooks), writes the shortcuts and the Apps entry under
  the NSIS names, removes NSIS registry leftovers, starts the service and
  waits for Running, opens the app through explorer as the signed-in user.
- `uninstall.exe --uninstall [--quiet]` removes the service, our driver
  service, shortcuts, the Apps entry and the install folder (busy files at
  reboot); settings stay in ProgramData. No rollback if an install fails
  halfway; running the installer again repairs it.
- The NSIS config and hooks stay in the repository until the Rust installer
  passes tester checks; CI no longer builds NSIS.

| Check (macOS) | Result |
| --- | --- |
| `cargo fmt --all --check` | Passed |
| `cargo clippy --workspace --all-targets -- -D warnings` | Passed |
| `cargo xwin clippy -p zsetup -p zapretd -p zapret-core -p zbuild --target x86_64-pc-windows-msvc --all-targets -- -D warnings` | Passed |
| `cargo test --workspace` | 47 passed, 0 failed (new: payload checks, install record, driver path check, zbuild payload) |
| `npm run check` / `npm run build` | 0 errors / 0 warnings; build passed |
| `scripts/build_windows_cross.sh` | Built `outputs/Zapret-Desktop-Setup.exe` (7.0 MB, payload 44 files) in 2 min |

Not verified: the installer has not run on Windows yet (no Windows here). The
new CI step (install, IPC test, reinstall over a loaded driver, IPC test,
uninstall, service and driver gone) has not run. Not checked: upgrade over the
tester's interrupted NSIS install, renaming a loaded driver file, the explorer
launch as the user, shortcuts, the Apps entry, SmartScreen and antivirus
reaction to an unsigned console installer.

## 2026-11-13 — user domain list and one-button WinDivert stop (branch `phase1/tester-fixes`)

Owner requests after the first tester run: show which domains were added,
make added domains take effect faster, and stop WinDivert with one button.

- Daemon: fixed commands `domains` (readable form, Unicode for IDN) and
  `remove_domain`. Adding a domain no longer restarts winws: the engine
  re-reads a hostlist when its size or modification time changes
  (`nfq/hostlist.c`, «automatically reload lists» in the engine readme), and
  the restart cut bypass for 1–2 s, so connections opened meanwhile failed.
  The list file is replaced in one rename. Reports still exclude user domains.
- Settings: under «Свои домены» a toggle «Ваш список: N доменов» expands the
  list; the domain just added is highlighted and marked «добавлен»; each
  entry can be removed. The notice says to reopen the site's tab, because
  the browser keeps an existing connection for a while.
- What stays an engine or network limit: sites that load from other domains
  (CDNs, APIs) need those added too; IP-based blocking is not fixed by DPI
  tricks; user domains get the same desync as the general list.
- Installer: the window asks once (Enter installs, «2» only stops WinDivert);
  `--stop-windivert` ends every winws and stops and unregisters the
  `WinDivert` and `WinDivert14` driver services whoever loaded them; a Start
  menu shortcut «Zapret Desktop — остановить WinDivert» runs it. CI runs it
  after the IPC test and checks the driver service is gone.

| Check (macOS) | Result |
| --- | --- |
| `cargo fmt --all --check` | Passed |
| `cargo clippy --workspace --all-targets -- -D warnings` | Passed |
| `cargo xwin clippy -p zapretd -p zapret-core -p zbuild -p zsetup --target x86_64-pc-windows-msvc --all-targets -- -D warnings` | Passed |
| `cargo test --workspace` | 48 passed, 0 failed (new: list, remove, IDN display, no engine restart on add) |
| `npm run check` / `npm run build` | 0 errors / 0 warnings; build passed |
| Settings page in Vite with sample domains | List expands and collapses, new entry highlighted (screenshot, no service) |
| `scripts/build_windows_cross.sh` | Built `outputs/Zapret-Desktop-Setup.exe` (7.0 MB) |

Not verified: winws picking up an added domain without restart on Windows,
the list against a real service, the stop mode and its shortcut on Windows,
and CI on this branch.

## 2026-11-14 — interface redesign (local, `codex/desktop-app`)

Owner-approved redesign after the reference mockup: black/white chrome with
one green accent, dark/light/system theme, a large on/off card instead of the
route diagram, results with a system side column, two-column settings, and on
Windows an in-app title row. The daemon API and Rust backend are unchanged;
the only non-UI changes are Tauri window config (size, black background,
frame off on Windows) and window permissions (minimise, maximise, close, drag).

- Fonts (Inter, Geist Mono, OFL) are bundled from @fontsource; Lucide icon paths
  (ISC) are inlined. Licenses added under `licenses/`. CSP unchanged.
- Not implemented from the mockup because the service has no such setting:
  restart behaviour choice and strategy-update toggle (rows state current
  behaviour), "slow" status, prettified log lines, "Open log" button.
- The time of the last check is remembered by the window only.
- Layout adapts: overview card becomes a band below 940 px, side columns move
  under the main column below 1000 px, the page scrolls down to the 720x560
  minimum window.

| Check (Arch Linux) | Result |
| --- | --- |
| `cargo fmt --all --check` | Passed |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | Passed |
| `cargo test --workspace --locked` | 48 passed, 0 failed |
| `npm run check` / `npm run build` | 0 errors / 0 warnings; build passed |
| Vite + Playwright with a mocked Tauri IPC | Screenshots of on, off, no strategy, service missing, selection running, selection failed, Doctor, light theme, 820x700 and 720x560 windows, Windows title row (mocked user agent); no page errors |

Not verified: the real Tauri window on any OS (frameless window, drag,
snap, maximise, close-to-exit on Windows), fonts and theme inside WebView2,
clipboard copy of the log, and the redesign against a running service.

## 2026-11-14: first Windows run of the redesign, pipe busy fix

Installer `Zapret-Desktop-Setup.exe` cross-built on Arch (cargo-xwin) and run
by three external testers on Windows (unsigned build). Wine with WebView2 157
crashes on start, so Wine is not a test environment.

| Tester | Result |
| --- | --- |
| A (screen recording) | Install, service, frameless window, title buttons, overview, Checks, Settings work. Bypass on with `general (ALT)`: YouTube, Discord, UDP, Cloudflare checks pass; YouTube, SoundCloud, Twitch open in the browser. |
| B | Works (no details reported). |
| C | Checks: YouTube fails on DNS (`os error 11001`, host unknown) for all 22 strategies, other checks pass; selection restores the original setting. The system resolver cannot resolve YouTube, which the packet engine cannot fix. |

Bug found by tester A: opening Settings showed "Служба Zapret не установлена
или не запущена" while the service was running. Settings loads domains and the
log in parallel with the 2 s status refresh; the daemon created the next pipe
instance only after accepting a client, so a parallel client got
`ERROR_PIPE_BUSY`, reported as a missing service. Fixed on both sides: the
daemon opens the next instance before serving a client, and the client retries
`ERROR_PIPE_BUSY` for up to 2.5 s.

| Check (Arch Linux) | Result |
| --- | --- |
| `cargo fmt --all --check` | Passed |
| `cargo xwin clippy -p zapretd -p zapret-core --target x86_64-pc-windows-msvc -- -D warnings` | Passed |
| `cargo test --workspace` | Passed |
| Windows build (`build.sh`) | Passed |

Not verified: the pipe fix on Windows (needs a retest of Settings).
Open: strategy selection keeps trying all strategies when the failure is DNS;
it could stop early and suggest changing DNS.

## 2026-11-14: installer window instead of console, NSIS removed

Decision of both developers: one own installer, no NSIS, MSI, Inno or
console. `zsetup` keeps every install step as it was; only the output moved
from `println`/Enter prompts to events shown in a window.

- Window: tao + wry (WebView2), frameless 420x500, rounded corners on
  Windows 11, our palette and fonts; the page `crates/zsetup/ui/index.html` and
  fonts are embedded and served from a custom protocol, WebView2 profile in
  `%TEMP%` (removed on exit), devtools, context menu and browser keys off.
- Install: one UAC prompt (manifest), installing starts at once, progress
  "N из 10" with the current step, then the app opens and the window closes by
  itself. Step notes (for example a file replaced after reboot) keep the window
  open with a Close button. Failure: step and error, Retry, Copy, Close. Close
  and Alt+F4 are ignored while steps run.
- Uninstall from Apps: same window, one confirmation, then progress.
  "Stop WinDivert" shortcut: same window, starts at once.
- No WebView2: a system message offers the Microsoft download page.
- `--quiet`: no window, attaches to the parent console and prints steps
  (CI install/reinstall/uninstall test unchanged).
- Exe: GUI subsystem, icon and version info (tauri-winres), manifest with
  requireAdministrator, Common Controls 6, PerMonitorV2. Size 8.1 MB (was 7.3).
- Tauri bundler: NSIS config and `packaging/windows/hooks.nsh` removed;
  bundle targets only `app` (macOS).
- Tester guide: SmartScreen steps and a DNS section (1.1.1.1 / DoH for
  Windows 11, Windows 10 and browsers).

| Check (Arch Linux) | Result |
| --- | --- |
| `cargo fmt --all --check` | Passed |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | Passed |
| `cargo xwin clippy -p zsetup --target x86_64-pc-windows-msvc --all-targets -- -D warnings` | Passed |
| `cargo test --workspace --locked` | 48 passed, 0 failed |
| Windows cross-build (`build.sh`) | Passed; PE subsystem GUI, manifest present |
| Page states in Chromium (Playwright, mocked events) | Progress, done, done with note, failure, uninstall question, stop done; no page errors |

Not verified: the installer window on real Windows (WebView2 crashes under
Wine), UAC and SmartScreen flow, auto-launch of the app, uninstall from Apps,
the no-WebView2 message, `--quiet` console output in CI.

## 2026-11-14: DNS failures no longer fail strategy selection

Second tester's recording: our YouTube check failed on every strategy with
`dns error … (os error 11001)` while Flowseal's `test zapret.ps1` (curl in the
user session) reached `https://www.youtube.com` with TLS 1.2/1.3 OK on the
same PC, and `general (ALT11)` worked. The check mixed a DNS problem with the
DPI result, so selection rejected all 22 strategies.

- HTTPS probe: on a DNS error the host (fixed check host only) is resolved
  through DNS-over-HTTPS by IP (`1.1.1.1`, then `8.8.8.8`) and checked at that
  address. Passing there counts as passed, with a new optional
  `ProbeResult.warning` (contracts regenerated). Failing there reports both.
- UI: Checks shows an amber mark, the warning, and how to turn on secure DNS
  (Windows 11 and Chrome/Edge); Overview shows "Мешает DNS" for that service.
- Not known: why the service (LocalSystem) gets NXDOMAIN when the user's curl
  does not. To ask the tester: `nslookup www.youtube.com`, VPN or DNS apps,
  hosts file.

| Check (Arch Linux) | Result |
| --- | --- |
| `cargo fmt --all --check` | Passed |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | Passed |
| `cargo xwin clippy -p zapret-core -p zapretd --target x86_64-pc-windows-msvc -- -D warnings` | Passed |
| `cargo test --workspace --locked` | 49 passed, 0 failed (new: DoH answer parsing, DNS error detection) |
| `npm run check` / `npm run build` | 0 errors / 0 warnings; build passed |
| Vite + Playwright, mocked IPC with a DNS warning | Checks and Overview screenshots, no page errors |
| Windows cross-build (`build.sh`) | Passed |

Not verified: the DoH fallback on a Windows PC with broken DNS.

## 2026-11-14: second tester, new installer and DNS fallback

Recording from the tester with the DNS problem, new `Zapret-Desktop-Setup.exe`
(installer window, DoH fallback):

- Installer window on Windows 11 (build 26200): installed over the previous
  version, showed "Готово" and opened the app. First real run of the window.
- Overview with `general-alt11`: all 4 checks pass, YouTube "Мешает DNS"
  (about 450 ms) instead of failing; also with the bypass off.
- Cause on this PC: `nslookup www.youtube.com` against Cloudflare
  `2606:4700:4700::1111` without encryption gives "No response from server";
  with automatic DNS the router (MikroTik) answers nothing either. Plain DNS
  queries for YouTube are dropped on the way; DoH works. The browser opens
  YouTube (it uses secure DNS).
- "Изменено пакетов" always showed 0: winws reports no counters and the
  service sends zero. The overview now shows "—, движок не сообщает" while
  running without counters.

## 2026-11-14: one-click secure DNS (Windows 11)

The tester fixed YouTube by hand: Settings → Wi-Fi → DNS server assignment →
manual, `1.1.1.1` / `1.0.0.1` and `2606:4700:4700::1111` with "DNS over HTTPS:
on (automatic template)", fallback to plain text off. After that all checks
pass with no DNS warning. The app now does the same on request.

- `DaemonCommand::SetSecureDns { enabled }` (fixed command, no arguments from
  the user besides on/off); `ServiceStatus.secure_dns { supported, enabled }`.
- `zapretd` (`secure_dns.rs`): connected Ethernet/Wi-Fi adapters with a
  gateway, VPN and virtual adapters skipped. Before changing anything it saves
  each adapter's IPv4/IPv6 servers and DoH settings to
  `%ProgramData%\ZapretDesktop\secure-dns.json`; enabling again keeps the
  first backup. Sets Cloudflare IPv4/IPv6 with
  `DNS_DOH_SERVER_SETTINGS_ENABLE_AUTO`, no UDP fallback, via
  `SetInterfaceDnsSettings` (DNS_INTERFACE_SETTINGS3). Off restores the saved
  settings (empty list = DHCP) and deletes the backup.
- Windows 10 (build < 22000) reports unsupported; the UI shows browser steps.
- UI: "Включить" in the Checks DNS hint (then checks run again), row
  "Защищённый DNS" in Settings with Включить/Выключить.
- Uninstall: new step "Возврат настроек DNS" runs
  `runtime\zapretd.exe --restore-dns` (no console window) before removing the
  service; a failure leaves a note, not an error.
- Never automatic: the app does not change DNS without the user's click, as
  corporate networks, VPNs and routers with local names rely on their DNS.

| Check (Arch Linux) | Result |
| --- | --- |
| `cargo fmt --all --check` | Passed |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | Passed |
| `cargo xwin clippy --workspace --target x86_64-pc-windows-msvc --all-targets -- -D warnings` | Passed |
| `cargo test --workspace --locked` | 50 passed, 0 failed |
| `npm run check` / `npm run build` | 0 errors / 0 warnings; build passed |
| Vite + Playwright, mocked IPC | Checks hint with button, Windows 10 hint, Settings on/off rows |
| Windows cross-build (`build.sh`) | Passed, installer 8.2 MB |

Not verified on Windows: enabling and restoring DNS on a real adapter, the
Settings app showing the result, uninstall restore, Windows 10 message.

## 2026-11-14: secure DNS on Windows, appearance settings

Tester recording with the one-click secure DNS build: installed over the old
version, "Включить" in Settings → "Защищённый DNS", checks pass, and Windows
Settings shows the adapter's DNS changed by the app. Restore and uninstall
were not shown.

Appearance (Settings → "Оформление"), stored only in the window's local
storage like the theme:

- Background and accent: popover with a large saturation/brightness field,
  hue strip, hex input (3 or 6 digits), screen eyedropper (EyeDropper API in
  WebView2), 16 presets, back-to-theme button; changes apply live. Panels,
  lines and text are derived from the background; a custom background picks
  dark or light by its brightness, and choosing a theme drops it. Accent text
  is adjusted to WCAG AA on the background; text on the accent picks the
  higher-contrast of near-black and white; a warning shows when the accent
  nearly matches the background.
- Font: Inter (default), system, Geist Mono and Windows fonts (Bahnschrift,
  Cascadia Code, Trebuchet MS, Verdana, Georgia, Consolas), each previewed in
  its own face; logs and codes stay monospace.
- Title-bar icon: drop or pick PNG/JPG/WebP/GIF/SVG/BMP up to 25 MB, any
  size; drag to pan, wheel/slider/keys to zoom (100-800 %), live preview in a
  title-bar sample; saved as a 256x256 PNG, corners rounded on display. Used
  in the title bar and the about line.
- "Сбросить всё" returns the defaults.

| Check (Arch Linux) | Result |
| --- | --- |
| `npm run check` / `npm run build` | 0 errors / 0 warnings; build passed |
| `cargo fmt --all --check`, `cargo test --workspace --locked` | Passed |
| Playwright on Vite with mocked IPC | Picker open, preset, hue and field drags, hex typing, font list, icon drop zone, crop with zoom and pan, saved icon in title bar, dark custom and light custom backgrounds, 720x560 popover fits, settings survive reload; no page errors |
| Windows cross-build (`build.sh`) | Passed |

Not verified: the eyedropper and fonts on real Windows/WebView2.

## 2026-11-14: app icon, window icon

- The lightning icon (`resources/icon.svg`, old placeholder) is replaced by
  the green z tile from the title bar; all `src-tauri/icons` regenerated with
  `tauri icon`. Exe, installer (`zsetup` embeds `icon.ico`), shortcuts, window
  and tray use it.
- The icon chosen in Settings → "Оформление" → "Значок" now also goes to the
  window (taskbar, Alt+Tab) and the tray, with rounded corners; removing it
  restores the z mark. Start menu and desktop shortcuts keep the exe icon.
  Tauri `image-png` feature, `core:window:allow-set-icon`,
  `core:tray:allow-get-by-id`, `core:tray:allow-set-icon`; tray id `main`.
- Checks: fmt, clippy (workspace, locked), 50 tests, `npm run check`/`build`,
  Windows cross-build (installer 8.4 MB) passed. Not verified on Windows.

## State at the end of 2026-11-14 (handoff)

Branch `codex/desktop-app`, everything pushed. Latest test installer was
built locally at `~/E/zapret-build/out/Zapret-Desktop-Setup.exe` (cross-build
on Arch, `~/E/zapret-build/build.sh`); CI builds the same from the branch.

Done today, in order:

1. Redesign of the UI after the mockup, own title bar on Windows
   (earlier today, see above).
2. Named pipe fix: parallel requests no longer report "service not installed".
3. Own installer window (tao + wry), no NSIS/MSI/Inno/console; `--quiet`
   stays windowless for CI.
4. DNS: checks fall back to DoH so selection measures DPI only; amber
   "Мешает DNS" with instructions.
5. Packet counter shows "—" because winws reports none.
6. One-click secure DNS (Windows 11): service command, backup and restore,
   uninstall restores DNS.
7. Appearance: background/accent pickers, fonts, cropped title-bar icon.
8. New app icon; custom icon on taskbar and tray.

Confirmed on testers' Windows 11 PCs (screen recordings):

- Installer window installs over the previous version and opens the app.
- No more red banner in Settings after the pipe fix.
- DoH fallback: YouTube shows "Мешает DNS" instead of failing; selection
  works.
- Secure DNS button sets Cloudflare DoH; Windows Settings shows it; all
  checks green afterwards.

Waiting for a Windows test:

- Uninstall from Apps in the window (with DNS restore step), "Выключить"
  for secure DNS restoring DHCP, Windows 10 message.
- Appearance on WebView2: eyedropper, Windows fonts, taskbar/tray icon swap.
- New z icon in Start menu, taskbar and installer.

Known gaps and ideas for next time:

- Why the service (LocalSystem) gets no DNS answer for YouTube while the
  user's browser does: the browser uses its own DoH; plain DNS for YouTube is
  dropped by the provider on testers' networks. Secure DNS covers it.
- Settings rows without a service setting yet: restart behaviour, update
  toggle; "slow" status; Doctor could detect DNS blocking up front.
- Appearance is per window (local storage), not per machine; exe and
  shortcut icons are fixed.
- Signing (SmartScreen warning) is a release gate and needs the owner.
