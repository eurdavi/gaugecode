<div align="center">

# GaugeCode

**How much of your AI limit you have spent — without opening anyone's dashboard.**

A tray app for Windows, macOS and Linux that shows your AI tool usage in real time
and when each limit resets.

[English](README.en.md) · [Português](../README.md)

</div>

---

## The problem

You use Claude Code, Cursor and one or two other AI tools at once. None of them shows
your usage outside its own product, so you are blind to the limit until you hit it in
the middle of a task.

GaugeCode fixes that with two surfaces that are always in reach.

- **Tray.** The icon is a ring that fills with your usage and changes colour: green
  under 50%, amber to 80%, red above. Hover for the exact figure, click for every limit
  window the provider reported.
- **Notch.** A thin sliver pinned to a screen edge, always on top. Folded it is
  click-through, so it never gets in the way. Point at it to expand; click to pin it.

## Supported tools

| Tool | What you do | Where GaugeCode reads it |
|---|---|---|
| **Claude Code** | run `claude`, then `/login` | `~/.claude/.credentials.json` |
| **Cursor** | just be signed in to Cursor | the editor's local database, read-only |
| **Codex** | run `codex` and sign in | `~/.codex/auth.json` |
| **GLM** (Z.ai) | set a Coding Plan key up in Claude Code, ZCode or OpenCode | the key that tool already holds |
| **Grok** | run `grok login` | `~/.grok/auth.json` |
| **OpenCode** | run `opencode auth login` and connect Go | the `opencode-go` key |

### There is no token to paste

That is the design, not a gap. Each tool already writes a credential on your machine
when **you** sign in to it. GaugeCode reads that file and asks the vendor's own usage
endpoint. Sign in to the tool and the number shows up within a minute — Settings has
the steps for each one, and shows them automatically when a tool needs a sign-in.

## Install

Grab an installer from [Releases](https://github.com/eurdavi/gaugecode/releases).

- **Windows** — `.exe` (NSIS) or `.msi`. Unsigned, so SmartScreen warns once:
  "More info" then "Run anyway".
- **macOS** — universal `.dmg` (Apple Silicon and Intel). Not notarised yet, so
  Gatekeeper warns on first open.
- **Linux** — `.AppImage` or `.deb`. The notch needs an **X11** session: on Wayland an
  application cannot place its own window, so it is turned off there and the tray takes
  over. Everything else works the same.

Available in English, Português and Español; it follows your system language.

## The honest caveat

The usage endpoints this app reads are **not documented** by the vendors and can change
without notice. When one breaks, GaugeCode says *stale* (with the age of the reading),
*needs sign-in*, or *error*.

It will **never** show a made-up percentage. That is the one commitment here that does
not bend — a wrong number is worse than no number.

## What the app reads, and what it never does

- **Read-only.** It reads the credential each tool already left on your machine and
  calls that vendor's own endpoint. It never writes to those files, and Cursor's SQLite
  is opened read-only.
- **No sign-in on your behalf**, no account switching, no keep-alive pings.
- **No telemetry.** The only network traffic is the app talking to the provider. CI
  fails the build if a network call or a credential read appears outside the adapters.
- **Tokens never reach the logs.** Logs carry HTTP status, provider name and duration.
- **Updates never install themselves.** The check only reports that a version exists.

## Contributing

Issues and pull requests welcome. If you have an account on a tool this app does not
cover well yet — especially **Codex, GLM, Grok or OpenCode**, which could not be tested
against a real account — a report of what appeared on your screen is genuinely useful.

```sh
pnpm install
pnpm tauri dev
```

`GAUGECODE_DEMO=1` runs on fixtures, with no network and no credential reads.

## License

MIT — see [LICENSE](../LICENSE).
