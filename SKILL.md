---
name: glass
description: |
  Use when an agent needs to publish a live surface (markdown, terminal
  output, a diagram, a diff, HTML) so an operator can watch work happen, or
  needs to drain the operator's feedback comments. Glass is the Misty Step
  live stage: a Rust service where agents publish typed surfaces during
  work and comments flow back exactly once. Trigger phrases: "Glass",
  "live stage", "publish a surface", "post to glass", "drain feedback",
  "wait for feedback".
argument-hint: "[publish|feedback|doctor]"
---

# Glass

Glass is the Misty Step live stage. Read `AGENTS.md` before changing the
frozen surface-kind contract, the feedback cursor semantics, or the
sandboxed-render security posture.

## Operating Contract

- Publish real work products as they happen — terminal output from a real
  command, a real diff, a real diagram — not a summary written after the
  fact. Surfaces are typed (`markdown`, `terminal`, `diff`, `html`, `code`,
  `mermaid`, `json`, `trace`, `image`); pick the kind that matches what you
  actually have.
- Reuse `--session <id>` across related posts in one lane so they land in
  the same session for the operator; omit it to start a new session (its
  title defaults to the post title if `--session-title` is not given).
- Drain feedback after publishing, not instead of it. `feedback` returns
  only `user`-authored comments through a server-side `agent_seq` cursor —
  each comment is delivered exactly once, so a second drain call for the
  same session returns nothing new even if nothing was read.
- Reply to a specific comment thread with `reply_to_user` (MCP only, no CLI
  wrapper yet — see Residual gap below).
- Prefer the CLI or MCP tools over hand-rolled `curl` against `/api/posts`
  or `/api/comments`; both wrap the exact same `Glass::publish_post` /
  `Glass::wait_for_feedback` core the HTTP API and MCP tools call.

## Expected MCP Tools

- `publish_post`: publish or update a post made of ordered typed surfaces.
- `wait_for_feedback`: drain user feedback once for a session via the
  server-side cursor.
- `reply_to_user`: attach an agent reply to a post's comment thread.

## Instance CLI

```sh
glass publish --title "Protocol proof" \
  --agent codex-glass-901 --session-title "glass-901 native build" \
  --markdown "Glass is receiving typed surfaces." \
  --terminal "$(cargo test --workspace 2>&1)"

glass publish --title "Second lane" --session <session-id-to-reuse> \
  --surfaces-json - <<'JSON'
[{"kind":"json","data":{"producer":"claude-session","status":"visible"}}]
JSON

glass feedback --session <session-id> --wait 5

glass doctor --url http://127.0.0.1:9041 --db data/glass.db
```

`publish` accepts `--markdown`/`--markdown-file`, `--terminal`/
`--terminal-file` for the two most common surface kinds directly, and
`--surfaces-json <path>|-` for anything else (any kind, or several surfaces
in one post) as a raw JSON array matching the MCP `publish_post` schema.
Add `--json` to either command for machine-readable output instead of the
human-readable summary.

## Residual gap

No CLI wrapper for `reply_to_user` yet (attaching an agent reply to a
specific comment thread) — use the MCP tool or `POST /api/comments` with
`author: "agent"` directly. Filed as follow-up scope, not blocking:
`publish`/`feedback` cover the two verbs the fleet audit (`misty-step-915`)
found agents actually reaching for hand-rolled curl to do.

## Local Gate

```sh
./scripts/check.sh
```

Equivalent to `cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`.

## Red Lines

- Do not fabricate `userFeedback` — only real comments authored `"user"`
  through the real cursor count; do not synthesize approval.
- Do not bypass the sandboxed-render security posture (CSP `sandbox`
  without `allow-same-origin`) when adding a new surface kind or render path.
- Do not add app-specific responder logic here — Glass owns the live
  surface, not downstream repair or triage.
