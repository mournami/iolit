# Zapret Desktop project

- Keep the project in this directory. User targets Windows and macOS, including
  TCP, UDP, Discord voice and QUIC. A TCP-only Mac proxy is not an accepted substitute.
- Source is open for now. Commercial quality is the release goal; do not claim
  production readiness based on compilation, unit tests or upstream assertions.
- Windows uses unmodified pinned Flowseal/bol-van engine binaries. Never execute
  imported `.bat` scripts. Reject unknown importer syntax.
- macOS packet engine is an attributed MIT fork under `engines/macos/`. Keep
  upstream pin and local changes documented. Do not silently fall back to proxy.
- GUI is unprivileged. Services accept fixed commands, never arbitrary executable
  paths or shell commands. Preserve other applications' firewall state.
- Start fresh installations disabled. Keep enable/disable and strategy persistent.
- Update `docs/RELEASE.md` with actual test evidence and failures before handoff.
- Never generate a publisher's production signing key or configure a paid service
  without the owner's decision. Signatures and network hardware checks are release gates.
