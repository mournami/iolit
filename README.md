<div align="center">

# Iolit

**YouTube, Discord and Telegram on Windows again, with one installer and one button.**

![license](https://img.shields.io/badge/license-GPL--3.0-blue)
![platform](https://img.shields.io/badge/platform-Windows%2010%20%7C%2011%20x64-informational)
![engine](https://img.shields.io/badge/engine-winws%20%2B%20WinDivert-2e8b57)

English · [Русский](README.ru.md)

<sub>Formerly Zapret Desktop.</sub>

</div>

<!-- Screenshot of the main window on Windows goes here: docs/assets/main-window.png -->

In Russia YouTube is slowed to a crawl and Discord does not connect, but the
blocking equipment only looks at the first packets of each connection. Change
those packets a little and it lets the connection through. The tools that do
this already exist: [zapret](https://github.com/bol-van/zapret) by bol-van and
the ready-made strategies of
[zapret-discord-youtube](https://github.com/Flowseal/zapret-discord-youtube) by
Flowseal. Using them means picking one of 22 `.bat` files by hand, running it
as administrator, and starting over when the provider changes something.

Iolit does that part. It installs the same engine as a Windows
service, tries the strategies itself, keeps a separate one for each service if
no single strategy fixes everything, checks in the background that it still
works, and updates the strategies on its own. Nothing goes through a server of
ours: there is no VPN, no account and no subscription. The traffic leaves your
computer exactly as before, only its first packets are shaped differently.

## Quick start

1. Download **`Iolit-Setup.exe`** from the
   [latest release](https://github.com/mournami/zapret13231/releases/latest).
2. Run it and confirm the administrator prompt. The installer shows its
   progress and opens the app; there is nothing to choose.
3. Press **«Включить обход»**, then **«Подобрать стратегию»**. Selection takes
   under three minutes.

> [!NOTE]
> Until code signing is set up (see [Code signing policy](#code-signing-policy)),
> Windows SmartScreen warns about the installer: press «Подробнее» →
> «Выполнить в любом случае». Antivirus programs sometimes flag WinDivert, the
> packet driver every zapret build uses.

A fresh installation starts with the bypass **off**. The interface is in
Russian.

## What it does

| | |
| --- | --- |
| **Strategy selection** | Tries the Flowseal strategies one by one against YouTube, Discord, Cloudflare and a UDP check, and stops at the first that fixes everything, within three minutes. |
| **Per-service composite** | If no single strategy fixes everything, YouTube gets the rules of the strategy that worked for YouTube, Discord those that worked for Discord, and so on. |
| **Background check** | While the bypass is on, a light check runs every 10 minutes and after a network change. If a service stops working, «Починить» re-selects for that service only. |
| **Doctor** | Finds the usual causes: the engine is not running, another program holds WinDivert, GoodbyeDPI or an old zapret is running, the provider's DNS hides the address. |
| **Secure DNS** | On Windows 11, one switch sets Cloudflare (then Google, then Quad9) as the system's encrypted DNS and checks that it answers. If none does, your previous settings come back. |
| **Your own sites** | Add domains the built-in lists do not cover. |
| **Telegram proxy** | An optional local proxy for Telegram Desktop. It carries Telegram's traffic to Telegram's own servers, see [below](#telegram-proxy). |
| **Updates** | New strategy bundles install silently and roll back if the checks get worse. A new app version waits behind the «Обновить» button and rolls back if the new service does not start. Both are signed. |
| **Full report** | «Сохранить отчёт» writes a text file with everything needed to find a fault, even when the service does not answer. IP addresses and your domains are masked. |

## How it works

```
 Iolit window  ──named pipe──▶  zapretd service (SYSTEM)  ──▶  winws + WinDivert
 (no admin rights)                        selection, checks, Doctor,      unmodified engine
                                          updates, secure DNS             binaries, pinned
```

The window is an ordinary unprivileged program. Everything that needs
administrator rights runs in the `zapretd` service, which accepts a fixed set
of commands and never runs paths or scripts it is given. The engine is the
unmodified, pinned `winws.exe` and WinDivert driver. Flowseal's `.bat` files are
never executed: a strategy compiler written in Rust reads them in CI and turns
them into a signed bundle. The client checks each bundle and app update with
an Ed25519 key built into it.

## What it changes on your computer

- Installs to `C:\Program Files\Zapret Desktop` (the folder keeps the old name
  for now), with Start menu and desktop shortcuts and an entry in «Приложения».
- Registers the **Iolit** service (`ZapretDesktop`), started
  automatically. It keeps its settings in `C:\ProgramData\ZapretDesktop`.
- Loads the **WinDivert** kernel driver while the bypass is on. The shortcut
  «Iolit — остановить WinDivert» stops it and the engine at once.
- Changes the system DNS **only** when you turn on secure DNS, and restores the
  previous settings when you turn it off.
- Opens `127.0.0.1:1443` **only** when you turn on the Telegram proxy.

## Troubleshooting

| What you see | Why | What to do |
| --- | --- | --- |
| «Служба не найдена» | The window cannot reach the service | Restart the computer. If it persists, install the latest version over it and send the report. |
| Doctor: «Драйвер WinDivert загружен другой программой» | An old zapret, GoodbyeDPI or similar loaded its own copy of the driver | Close that program, press the «остановить WinDivert» shortcut, then turn the bypass on again. Use «Перезагрузка», not «Завершение работы»: fast startup keeps the driver loaded. |
| A check fails with a DNS error | The provider hides the address, the bypass cannot fix that | Turn on secure DNS in the hint or in «Настройки». |
| Discord voice or the UDP check fails | The current strategy does not cover UDP | Run «Подобрать стратегию». |
| Updates fail to download | github.com does not answer on this network | Nothing: the app falls back to mirrors (from 0.2.6). |
| «Порт 1443 занят» | The original TG WS Proxy is running | Close it; this app has the same proxy built in. |

If none of this helps, press **«Сохранить отчёт»** on the «Проверка» page and
attach the file to an issue.

## Telegram proxy

Off by default, with its own switch in «Настройки», independent of the bypass.
The service listens on `127.0.0.1:1443` and carries Telegram Desktop's MTProto
traffic over a WebSocket (TLS) to Telegram's own gateway
(`kws*.web.telegram.org`), or over plain TCP to the datacenter when a datacenter
has no gateway. The traffic stays encrypted by Telegram end to end; the proxy
only re-wraps the transport. Upstream's Cloudflare fallback is deliberately left
out, so no third-party server takes part; the price is that media from some
datacenters may not load where direct TCP to Telegram is blocked. The secret in
the `tg://proxy` link is created on your computer and stays in the service data.
It is a Rust port of the core of
[Flowseal/tg-ws-proxy](https://github.com/Flowseal/tg-ws-proxy).

## Privacy policy

Iolit has no telemetry, no accounts and no server of its own. It
connects to other systems only for these purposes:

- **Checks**, when you press «Проверить» or «Подобрать стратегию», and every 10
  minutes while the bypass is on: `www.gstatic.com`, `www.youtube.com`,
  `discord.com`, `www.cloudflare.com` and the STUN server `stun.l.google.com`.
  Only a normal request is sent; nothing about you is included.
- **Updates**: the strategy bundle a few minutes after start and then every
  6 hours, the app manifest every 6 hours, from this repository's GitHub
  releases. Where github.com does not answer, the same signed files come from
  `raw.githubusercontent.com` (and, for strategy bundles, `cdn.jsdelivr.net`).
- **Secure DNS**, only if you turn it on: your DNS queries go to Cloudflare,
  Google or Quad9 instead of your provider.
- **Telegram proxy**, only if you turn it on: Telegram Desktop's traffic goes to
  Telegram's own servers.

The report from «Сохранить отчёт» is written only to the file you choose and is
never sent anywhere. It masks IP addresses, the domains you added and the
Telegram proxy secret. Logs stay on your computer.

## Uninstall

«Параметры» → «Приложения» → **Iolit** → «Удалить». The service, the
driver and the files go; your settings stay in `C:\ProgramData\ZapretDesktop`
in case you install again, and can be deleted by hand.

## Code signing policy

Free code signing provided by [SignPath.io](https://about.signpath.io), certificate by [SignPath Foundation](https://signpath.org)

> [!IMPORTANT]
> The application to SignPath Foundation is in preparation. Until it is
> approved, the installers in Releases are not signed.

Only the installer built by GitHub Actions from this repository is signed, and
every release is approved by hand.

- Committers and reviewers: [mournami](https://github.com/mournami),
  [pushmoveax](https://github.com/pushmoveax)
- Approvers: [mournami](https://github.com/mournami)

See also the [privacy policy](#privacy-policy).

## Credits

The bypass itself is other people's work. This project packages it and adds
selection, diagnosis and updates.

- [bol-van/zapret](https://github.com/bol-van/zapret): the `winws` engine.
- [Flowseal/zapret-discord-youtube](https://github.com/Flowseal/zapret-discord-youtube):
  the strategies and lists (MIT).
- [Flowseal/tg-ws-proxy](https://github.com/Flowseal/tg-ws-proxy): the Telegram
  proxy this one is ported from (MIT).
- [WinDivert](https://github.com/basil00/WinDivert) by basil00: the packet
  driver (LGPLv3 / GPLv2).

Full notices: [`licenses/THIRD-PARTY.md`](licenses/THIRD-PARTY.md). Building and
checking the code: [`docs/development.md`](docs/development.md).

## License

GNU GPL version 3 only (`GPL-3.0-only`), see [`LICENSE`](LICENSE). Forks and
redistributed builds must publish their complete source under the same license.
The license was MIT until 2026-11-15; copies taken before that date keep their
MIT terms.
