# Acceptance criteria

No item is considered verified from an upstream README or an unrelated test.

## Windows x64

- Clean-machine signed NSIS install, UAC only for installation.
- Standard-user GUI connects to service; remote pipe clients rejected.
- Enable/disable confirms actual winws process state. Competing WinDivert handles
  and missing/blocked drivers produce actionable errors.
- Windows reboot restores enabled and disabled states separately.
- YouTube plays a video for 10 minutes; Discord joins a real voice channel and
  transmits/receives audio; QUIC and an ordinary UDP application are exercised.
- Network disconnect/reconnect, Wi-Fi change, sleep/wake and engine crash recovery.
- Strategy selection cancellation/no-winner/error restores prior state.
- Upgrade and uninstall stop the engine and leave unrelated firewall/software intact.

## macOS

- Test Apple Silicon and Intel separately on the minimum supported and current OS.
- Clean signed/notarized app + helper installation; root-owned data/socket paths.
- Packet transport explicitly `divert`; `caps.udp` and injection available.
- PF-log capture and BPF write validated on actual uplink (Wi-Fi and Ethernet).
- IPv4 and IPv6, QUIC, Discord voice and video playback verified independently.
- Repeat startup, reboot, sleep/wake, network changes, VPN coexistence refusal.
- Kill engine with an active connection: guard must restore networking within its
  interval and app must report stopped/failure, never a false connected state.
- Uninstall removes only this app's PF anchor, daemon and guard; user data retained.

## Product/release

- Published support matrix and known limitations; no universal unblock guarantee.
- App and bundle update signatures, staged application, health check, rollback.
- Real publisher certificates: Apple Developer ID/notarization, Windows Authenticode.
- Corresponding WinDivert source and required notices included with distribution.
- Reproducible build, security review of local IPC and root operations, logs bounded.
- No fabricated success indicators, unconfigured payment or licensing controls.
