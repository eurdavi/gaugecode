# GaugeCode

A tray app (Windows first, macOS next) that shows, in real time, **how much of your
session limit you've used** in Claude Code, Cursor and Codex — and when each window resets.

Two surfaces, both in v1:

- **Notch** — a small pill pinned to a screen edge, always on top. Hover to expand, click to pin.
- **Tray** — the current session percentage drawn on the tray icon itself. Click for details.

Inspired by [vinzdg/codenotch](https://github.com/vinzdg/codenotch) (macOS, Swift). This is an
independent, cross-platform reimplementation in Tauri 2 (Rust + React).

Available in English, Português (Brasil) and Español — it follows your system language.

> **Status: M5.** All three adapters, the tray, the notch, autostart and single-instance are in.
> The in-app updater is wired but inert until a signing key exists — see "Releases" below and
> `SPEC.md` §12 for the roadmap.

## What works today

- Tray icon with the session percentage drawn into it — green under 50%, amber to 80%, red above,
  dimmed when the reading is stale. If the provider you picked has no reading, the icon shows one
  that does, and the tooltip names it.
- Click the icon for a popup with every limit window each provider reported and when each resets.
- A notch overlay on any screen edge. Folded it is a click-through sliver; point at it to peek,
  click to pin. Three animation styles, or none.
- Polls every 60s while the tool is running, every 5 min otherwise. A 429 backs off from 1 min up
  to 15 min, and the penalty survives a restart.
- `GAUGECODE_DEMO=1` shows all three providers from fixtures, without touching a credential.

## There is no token to paste

That is the whole design. Each tool already writes a credential on your machine when you sign in to
it; GaugeCode reads that file and asks the vendor's own usage endpoint. So:

| Provider | What you do | Where GaugeCode reads it |
|---|---|---|
| Claude Code | run `claude`, then `/login` | `~/.claude/.credentials.json` (`claudeAiOauth`) |
| Cursor | just be signed in to Cursor | `state.vscdb`, opened read-only |
| Codex | install the CLI, run `codex` and sign in | `~/.codex/auth.json` |

Sign in to the tool and the number shows up within a minute — the adapter re-reads the credential
every cycle. Settings has these same steps per provider, and shows them automatically for any
provider that reports `needs sign-in`.

## The honest caveat

The three usage endpoints this app reads are **not documented** by the vendors and can change
without notice. When one breaks, GaugeCode shows `stale` (with its age), `needsAuth` or `error`.
It will **never** show a made-up percentage.

## What the app reads, and what it never does

- **Read-only.** It reads the credential each tool already left on your machine
  (`~/.claude/.credentials.json`, Cursor's `state.vscdb`, `~/.codex/auth.json`) and calls that
  vendor's own usage endpoint with it. It never writes to those files. SQLite is opened read-only.
- **No login, no account switching, no keep-alive pings.**
- **No telemetry.** The only network traffic is the app talking to the provider. CI fails the
  build if a network call appears outside `src-tauri/src/providers/`.
- **Tokens never reach the logs.** Logs contain HTTP status, provider name and duration. Nothing else.
- **Updates never install themselves.** The check only reports that a version exists; installing is
  a click in Settings.

## Releases

Pushing a `v*` tag runs `.github/workflows/release.yml`, which builds a Windows NSIS installer and a
universal macOS `.dmg` and opens a draft GitHub Release.

Windows builds are **not** code-signed, so SmartScreen warns once ("More info" → "Run anyway").
macOS builds are **not** notarised yet, so Gatekeeper warns too. Both are documented rather than
hidden; see `SPEC.md` §14.

The in-app updater needs a minisign key pair, which this repository does not have yet. Until it
does, `bundle.createUpdaterArtifacts` stays `false` and the update check simply fails at debug
level. To enable it:

```sh
pnpm tauri signer generate -w ~/.tauri/gaugecode.key
# put the public key in src-tauri/tauri.conf.json under plugins.updater.pubkey
# set createUpdaterArtifacts to true
gh secret set TAURI_SIGNING_PRIVATE_KEY < ~/.tauri/gaugecode.key
gh secret set TAURI_SIGNING_PRIVATE_KEY_PASSWORD
```

Keep the private key safe: losing it means no existing install can ever be updated again.

## Development

Prerequisites: [Tauri 2 prerequisites](https://v2.tauri.app/start/prerequisites/) for your OS,
Node LTS, `pnpm`.

```sh
pnpm install
pnpm tauri dev
```

Useful env vars:

- `GAUGECODE_DEMO=1` — fixtures only, no network, no credential reads (for recording demos).
- `RUST_LOG=gaugecode=debug` — verbose logs (still no tokens).

Before marking a milestone done:

```sh
pnpm tsc --noEmit
cargo test --manifest-path src-tauri/Cargo.toml
pnpm tauri build
rg "reqwest|fetch\(" src src-tauri/src --glob '!src-tauri/src/providers/**'   # must be empty
```

## License

MIT
