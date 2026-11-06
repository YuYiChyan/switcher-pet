# Project agents — codex-switcher

## Self-use fork safety overrides

These rules supersede the upstream deployment instructions below for this fork.

- The user's installed Switcher and current Codex session are live infrastructure.
  Never edit, stop, restart, replace or uninstall the original app, its account
  store, or its Codex auth/config. Do not deploy to the upstream author's Macs.
- Work only in this independent fork. Before running any app build, isolate all
  data/config/log/skill paths, app identifiers, deep links, ports, update channels
  and OAuth refresh-token ownership. The upstream debug HOME override is not
  sufficient to isolate every module.
- User-requested exception: default inference proxy port is 18080, matching
  the original. Keep it disabled while the original owns that listener; never
  stop the original to claim it. Tests use random loopback ports. Management
  and dev ports remain separate.
- Use synthetic fixtures. Never copy real credentials into this repository or
  launch two refreshers against the same live OAuth refresh token.
- Review docs/SELFUSE_AUDIT.zh-CN.md and docs/SELFUSE_PRODUCT_DESIGN.zh-CN.md.
- Only the paths explicitly documented in README.selfuse.md have been repaired
  and tested. Synthetic provider tests do not imply real-account compatibility.

Tauri 2 desktop app that proxies codex CLI / Claude Code traffic across
multiple ChatGPT / Relay accounts. Routing rules from
`~/.claude/CLAUDE.md` and `~/.codex/AGENTS.md` apply here too.

## Local conventions

- Rust + React. `npx tauri dev` for live reload, `npx tauri build
  --bundles app` for production .app.
- Source: `src-tauri/src/` (Rust) + `src/` (React/TS). Frontend: Vite,
  no UI lib (pure CSS).
- Don't introduce dependencies you can do without.

## Architecture

- `src-tauri/src/proxy.rs` — main HTTP+WS proxy. Get familiar with
  `handle_request` / `handle_websocket` / `get_upstream` (3-branch:
  ChatGPT / OpenAI key / Relay) before touching it.
- `src-tauri/src/account.rs` — `AccountKind` (Legacy / ChatgptOauth /
  OpenaiKey / Relay) + `AppSettings`.
- `src-tauri/src/output_compress.rs` — Phase-1 shell-output compressor
  hooked into both Codex WS and Claude SSE paths in proxy.rs.
- `src-tauri/src/usage.rs` — quota fetchers per account kind.

## When editing

- After ANY proxy.rs change, restart the running Codex Switcher.app to
  reload (the proxy is in-process).
- Settings UI lives in `src/components/Settings.tsx`. Mirror the existing
  toggle pattern — don't invent new layouts.
- Two macs run this: local + mini mac (192.168.2.6 LAN / mini-mac-zt
  ZeroTier). Big changes go through `tar -czf + scp` after replacing
  /Applications/Codex Switcher.app locally.

## When NOT to use glance

This project deals with proxying chat completion APIs — don't ask glance
to "research" the OpenAI / Claude wire format. Read the official docs
yourself or look at `proxy.rs` directly.
