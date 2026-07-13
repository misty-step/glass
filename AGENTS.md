# Glass Repo Instructions

Glass is the native Rust stage for live agent surfaces. It adopts the Sideshow
grammar and owns the implementation.

## Contracts

- Rust only unless a platform boundary is named in the change.
- The frozen surface kinds are `html`, `diff`, `image`, `trace`, `markdown`,
  `terminal`, `mermaid`, `json`, `code`, and `metric`.
- Glass is ONE-WAY (glass-912, operator ruling 2026-07-07): there is no reply
  channel back to the producing agent. Do not reintroduce a comment/feedback
  surface, an `agent_seq` cursor, or a `wait_for_feedback`/`reply_to_user`
  tool. Needs You may relay an operator answer to the external runtime that
  owns an explicit ask; that is not an agent-authored Glass reply channel.
- Every running agent has its own live status feed at `/agent/:agent`
  (`GET /api/posts/recent?agent=...`), in addition to the `/session/:id`
  drill-down. Dead sessions (quiet past `LIVE_WINDOW_SECONDS`) demote out of
  the primary fleet-wall rail but keep their full post history on their own
  feed.
- Agent-authored or rich rendered surfaces must be served by a real URL with a
  `Content-Security-Policy` response header that includes `sandbox` and omits
  `allow-same-origin`.
- Asset ids are the lowercase SHA-256 of the uploaded bytes.
- The viewer is built on the Misty Step Aesthetic kit (`assets/aesthetic.css`,
  served at `/aesthetic.css`); compose new UI from its `--ae-*` tokens and
  `.ae-*` components rather than bespoke CSS. `DESIGN.md` in the `aesthetic`
  repo is the law.
- Bridge feed display is retired for Glass-wired lanes; Bridge may still relay
  events, but Glass owns human-facing live surfaces.

## Gate

Run the repo gate before completion:

```sh
./scripts/check.sh
```

The gate is:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Do not weaken the gate to ship.
