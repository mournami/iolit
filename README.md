<div align="center">

<img src="resources/icon-dark.svg" width="112" height="112" alt="Iolit icon">

# Iolit

**Gets YouTube, Discord and Telegram working again on Windows.**

![license](https://img.shields.io/badge/license-GPL--3.0-blue)
![platform](https://img.shields.io/badge/platform-Windows%2010%20%7C%2011%20x64-informational)
![engine](https://img.shields.io/badge/engine-winws%20%2B%20WinDivert-2e8b57)

English · [Русский](README.ru.md)

</div>

<!-- Screenshot of the main window on Windows goes here: docs/assets/main-window.png -->

Iolit brings back YouTube, Discord and Telegram on Windows where they are
throttled or blocked. Install it, press a button, and it finds a working
strategy, keeps an eye on it and updates itself. There is no VPN and no server
of ours in the middle: your traffic goes straight from your computer, as it
always did.

Under the hood are the [zapret](https://github.com/bol-van/zapret) engine by
bol-van, the
[zapret-discord-youtube](https://github.com/Flowseal/zapret-discord-youtube)
strategies by Flowseal, and our Rust port of his
[tg-ws-proxy](https://github.com/Flowseal/tg-ws-proxy) for Telegram. No more trying `.bat` files one by one as
administrator; if your provider changes something, the «Починить» (repair)
button is enough.

## Quick start

1. Download **`Iolit-Setup.exe`** from the
   [latest release](https://github.com/mournami/iolit/releases/latest).
2. Run it and allow the administrator prompt. The installer does the rest and
   opens the app.
3. Press **«Включить обход»** (turn on), then **«Подобрать стратегию»** (find a
   strategy). Finding one takes up to three minutes.

After installing, the bypass stays off until you turn it on. The interface is
in Russian.

> [!NOTE]
> The installer is not signed yet, so Windows SmartScreen will complain about
> it. Click "More info" → "Run anyway". Some antivirus programs also flag
> WinDivert, the packet driver that every zapret build uses. Signing will come
> later, see the [code signing policy](#code-signing-policy).

If Iolit brought YouTube, Discord or Telegram back for you, please star the
repository with the **Star** button at the top of the page. It helps people
find the project, and free code signing, which would stop the Windows warning,
is only given to projects that are visibly in use.

## What it does

- **Finds a strategy for you.** It goes through the Flowseal strategies, checks
  YouTube, Discord, Cloudflare and UDP with each one, and stops at the first
  one that makes everything work.
- **Builds its own when no single one is enough.** Sometimes one strategy fixes
  YouTube and another fixes Discord. Iolit then takes the part that worked from
  each.
- **Keeps an eye on things.** Every 10 minutes and after a network change it
  quietly checks the services. If one stops working, a «Починить» (repair)
  button appears and re-selects a strategy for that service only.
- **Doctor** tells you when something else is in the way: GoodbyeDPI or an old
  zapret is running, another program holds WinDivert, or your provider's DNS
  hides the real address.
- **Secure DNS** with one switch (Windows 11). If no server answers, your
  previous settings come back.
- **Your own sites**, for anything the built-in lists miss.
- **A Telegram proxy**, if you want one. More on that
  [below](#telegram-proxy).
- **Updates.** Strategies update on their own; a new version of the app
  installs when you press «Обновить» (update). If things get worse after an
  update or the service does not come back up, it rolls back. Every update is
  signed.
- **A report for us.** If something breaks, press «Сохранить отчёт» (save
  report) and send us the file. It has what we need to find the cause, with
  IP addresses and your own sites masked.

## How it works

```
 Iolit window  ──named pipe──▶  zapretd service (SYSTEM)  ──▶  winws + WinDivert
 (no admin rights)              selection, checks, Doctor,     unmodified engine
                                updates, secure DNS            binaries
```

The window runs without administrator rights. Everything that needs them is
done by the `zapretd` service, which only accepts a fixed set of commands and
never runs paths or scripts it is handed. The engine is the unmodified
`winws.exe` and WinDivert driver, pinned to a known version. We never run
Flowseal's `.bat` files: in CI our strategy compiler, written in Rust, reads
them and turns them into a signed bundle. The app checks every update against a
key built into it.

## What it changes on your computer

- A folder in `C:\Program Files\Iolit`, shortcuts in the Start menu and on the desktop, and an entry in
  "Apps".
- The **Iolit** service, which starts with Windows. Its settings live in
  `C:\ProgramData\Iolit`.
- The **WinDivert** driver, loaded while the bypass is on. If you need to stop
  everything at once, use the «Iolit — остановить WinDivert» shortcut.
- The system DNS changes only if you turn on secure DNS yourself. Turn it off
  and your previous settings come back.
- `127.0.0.1:1443` opens only while the Telegram proxy is on.

## Troubleshooting

| What you see | What to do |
| --- | --- |
| «Служба не найдена» (service not found) | Restart the computer. If that does not help, install the latest version over it and send us the report. |
| Doctor says another program holds WinDivert | Usually an old zapret or GoodbyeDPI. Close it, press the «остановить WinDivert» shortcut and turn the bypass on again. If that does not help, restart the computer. It has to be "Restart": after "Shut down", fast startup keeps the driver in memory. |
| A check fails with a DNS error | Your provider hides the real address, and the bypass cannot fix that. Turn on secure DNS from the hint or in «Настройки» (settings). |
| Discord voice does not work | The current strategy does not cover UDP. Press «Подобрать стратегию». |
| Updates do not download | Nothing to do: if GitHub does not open, the app gets the files from mirrors. |
| «Порт 1443 занят» (port 1443 is busy) | Most likely the original TG WS Proxy is running. Close it; the same proxy is built in. |

Nothing here helped? Save the report on the «Проверка» page and attach it to an
[issue](https://github.com/mournami/iolit/issues).

## Telegram proxy

Off by default. It has its own switch in «Настройки» and does not depend on
the bypass. It is our Rust port of the core of
[Flowseal/tg-ws-proxy](https://github.com/Flowseal/tg-ws-proxy). Telegram
Desktop connects to `127.0.0.1:1443`, and the service carries its traffic over
a WebSocket to Telegram's own gateways (`kws*.web.telegram.org`), or over plain
TCP when a server has no gateway. The traffic stays encrypted by Telegram; the
proxy only changes the wrapping.

We left out the original's Cloudflare fallback so that nobody else is in the
chain. The downside is that where direct TCP to Telegram is blocked, some media
may not load. The secret for the `tg://proxy` link is created on your computer
and never leaves it.

## Privacy policy

There is no telemetry, no account, and we have no server of our own. The app
goes online only for these reasons:

- **Checks.** When you press «Проверить» or «Подобрать стратегию», and every 10
  minutes while the bypass is on. These are ordinary requests to
  `www.gstatic.com`, `www.youtube.com`, `discord.com`, `www.cloudflare.com` and
  the STUN server `stun.l.google.com`, with nothing about you in them.
- **Updates.** Every 6 hours (strategies also a few minutes after start), from
  this repository's GitHub releases. If GitHub does not open, the same signed
  files come from `raw.githubusercontent.com`, and strategies also from
  `cdn.jsdelivr.net`.
- **Secure DNS**, if you turn it on. Your DNS queries then go to Cloudflare,
  Google or Quad9 instead of your provider.
- **Telegram proxy**, if you turn it on. Telegram Desktop's traffic then goes
  to Telegram's servers.

The report is saved only to the file you choose and is never sent anywhere on
its own. IP addresses, your domains and the proxy secret are replaced with
asterisks. Logs stay on your computer.

## Uninstall

«Параметры» → «Приложения» → **Iolit** → «Удалить» (Settings → Apps →
Uninstall). The service, the driver and the files are removed. Your settings
stay in `C:\ProgramData\Iolit` in case you come back; you can delete
them by hand.

## Code signing policy

Free code signing provided by [SignPath.io](https://about.signpath.io), certificate by [SignPath Foundation](https://signpath.org)

> [!IMPORTANT]
> We are still preparing the application to SignPath Foundation. Until it is
> approved, the installers in Releases are not signed.

We sign only the installer that GitHub Actions builds from this repository, and
every release is approved by hand.

- Committers and reviewers: [mournami](https://github.com/mournami),
  [pushmoveax](https://github.com/pushmoveax)
- Approvers: [mournami](https://github.com/mournami)

What the app sends over the network is described in the
[privacy policy](#privacy-policy).

## Credits

We did not invent the bypass and did not build it. We put it into one app and
added selection, checks and updates. Thanks to:

- [bol-van/zapret](https://github.com/bol-van/zapret): the `winws` engine.
- [Flowseal/zapret-discord-youtube](https://github.com/Flowseal/zapret-discord-youtube):
  the strategies and lists (MIT).
- [Flowseal/tg-ws-proxy](https://github.com/Flowseal/tg-ws-proxy): the Telegram
  proxy ours is ported from (MIT).
- [WinDivert](https://github.com/basil00/WinDivert) by basil00: the packet
  driver (LGPLv3 / GPLv2).

Full notices: [`licenses/THIRD-PARTY.md`](licenses/THIRD-PARTY.md). How to
build and check the code: [`docs/development.md`](docs/development.md).

## License

GNU GPL version 3 only (`GPL-3.0-only`), see [`LICENSE`](LICENSE). Forks and
builds that are passed on must publish their complete source under the same
license. The license was MIT until 2026-11-15; copies taken before that date
keep their MIT terms.

## Maintainers

Iolit is made by [mournami](https://github.com/mournami) and
[pushmoveax](https://github.com/pushmoveax).

Also, special thanks to [Codex](https://openai.com/codex/) and
[Claude Code](https://claude.com/claude-code).
