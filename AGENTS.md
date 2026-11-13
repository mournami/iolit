# Zapret Desktop project

## Source of truth
- Target architecture and phase plan: `docs/ARCHITECTURE.md`. Follow it. If a task conflicts with it, stop and ask the owner before writing code.
- Current phase: **1 (Windows MVP)**. Do not start work that belongs to a later phase unless the owner asks for it.
- Bypass checks in Russia are pending external testers; technical acceptance is done by the owner.

## Architecture rules
- All backend logic is Rust: `crates/zapret-core` (no OS calls), `crates/zapretd` (privileged daemon), `crates/zbuild` (CI bundle builder). Do not add new Go or Python to the product. `engines/macos/` is frozen until phase 5: keep it, do not extend it.
- The GUI (`src-tauri`, `ui`) is a thin unprivileged client. Strategy selection, probes, Doctor and updates run in `zapretd`, never in the Tauri process.
- The daemon API is defined by Rust types. `contracts/*.schema.json` and `ui/src/generated/*` are generated from them and never edited by hand.
- `.bat` files are parsed in exactly one place, the Rust strategy compiler. Never execute imported `.bat` scripts. Reject unknown importer syntax.
- Services accept fixed commands, never arbitrary executable paths or shell commands. The daemon never writes to user-chosen paths. Preserve other applications' firewall state.
- UI: the design may be changed only with the agreement of both developers; keep Russian UI strings. Talk to the daemon only through `ui/src/api.ts` and the transport layer.

## Platforms
- Release order: Windows, then routers (OpenWrt, Keenetic) and Linux, then Android, then macOS. iOS and tvOS are out of scope.
- Windows uses unmodified pinned Flowseal/bol-van engine binaries.
- macOS (phase 5): the packet engine is the attributed MIT fork under `engines/macos/`; keep the upstream pin and local changes documented. A TCP-only Mac proxy is not an accepted substitute; never silently fall back to a proxy.

## Product rules
- Start fresh installations disabled. Keep enable/disable and strategy persistent.
- No telemetry. Diagnostic reports exclude IP addresses, user domains and raw logs.

## Working rules
- Move existing code instead of rewriting it; keep behaviour unless the task says otherwise.
- Small commits, one logical change each. English commit messages and code comments.
- Before each commit run what applies: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `npm run check`, `npm run build`. If a check cannot run in your environment, write that down instead of claiming it passed.
- Update `docs/RELEASE.md` with actual test evidence and failures before handoff. Do not claim production readiness based on compilation, unit tests or upstream assertions.
- Never generate a publisher's production signing key or configure a paid service without the owner's decision. Signatures and network hardware checks are release gates.
- Do not push unless the owner asks. Keep the project in this directory; source is open for now.
