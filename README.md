# GaugeCode

A tray app (Windows first, macOS next) that shows, in real time, **how much of your
session limit you've used** in Claude Code, Cursor and Codex — and when each window resets.

Two surfaces, both in v1:

- **Notch** — a small pill pinned to a screen edge, always on top. Hover to expand, click to pin.
- **Tray** — the current session percentage drawn on the tray icon itself. Click for details.

Inspired by [vinzdg/codenotch](https://github.com/vinzdg/codenotch) (macOS, Swift). This is an
independent, cross-platform reimplementation in Tauri 2 (Rust + React).

> **Status: M2.** The Claude Code adapter, the polling scheduler, the tray icon and the notch
> overlay are in. The Cursor and Codex adapters (M3) are not. See `SPEC.md` §12 for the roadmap.

## What works today

- Tray icon with the session percentage drawn into it — green under 50%, amber to 80%, red above,
  dimmed when the reading is stale.
- Click the icon for a popup with every limit window the provider reported and when each resets.
- Polls every 60s while the tool is running, every 5 min otherwise. A 429 backs off from 1 min up
  to 15 min, and the penalty survives a restart.
- `GAUGECODE_DEMO=1` shows all three providers from fixtures, without touching a credential.

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
