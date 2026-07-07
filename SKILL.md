---
name: glass
description: |
  Use when an agent needs to publish a live surface (markdown, terminal
  output, a diagram, a diff, a metric) so an operator can watch work
  happen. Glass is the Misty Step live stage: a Rust service where agents
  publish typed surfaces during work. Glass is one-way — there is no reply
  channel back to the producing agent. Trigger phrases: "Glass",
  "live stage", "publish a surface", "post to glass", "status feed".
argument-hint: "[publish|doctor]"
---

# Glass

Glass is the Misty Step live stage. Read `AGENTS.md` before changing the
frozen surface-kind contract, the one-way posture, or the sandboxed-render
security posture.

## Operating Contract

- Publish real work products as they happen — terminal output from a real
  command, a real diff, a real diagram — not a summary written after the
  fact. Surfaces are typed (`markdown`, `terminal`, `diff`, `html`, `code`,
  `mermaid`, `json`, `trace`, `image`, `metric`); pick the kind that matches
  what you actually have. `metric` is a label+value chip:
  `{"kind":"metric","label":"tests","value":"42 passed"}`.
- Reuse `--session <id>` across related posts in one lane so they land in
  the same session for the operator; omit it to start a new session (its
  title defaults to the post title if `--session-title` is not given). Every
  post you make is visible under your agent's own feed at `/agent/:agent`.
- Glass is one-way: there is no feedback or reply channel. Do not poll for
  or expect a response through Glass; if the operator needs to react,
  that happens somewhere else.
- Prefer the CLI or MCP tool over hand-rolled `curl` against `/api/posts`;
  both wrap the exact same `Glass::publish_post` core the HTTP API and MCP
  tool call.

## Expected MCP Tools

- `publish_post`: publish or update a post made of ordered typed surfaces.

## Instance CLI

```sh
glass publish --title "Protocol proof" \
  --agent codex-glass-901 --session-title "glass-901 native build" \
  --markdown "Glass is receiving typed surfaces." \
  --terminal "$(cargo test --workspace 2>&1)"

glass publish --title "Second lane" --session <session-id-to-reuse> \
  --surfaces-json - <<'JSON'
[{"kind":"json","data":{"producer":"claude-session","status":"visible"}},
 {"kind":"metric","label":"tests","value":"42 passed"}]
JSON

glass doctor --url http://127.0.0.1:9041 --db data/glass.db
```

`publish` accepts `--markdown`/`--markdown-file`, `--terminal`/
`--terminal-file` for the two most common surface kinds directly, and
`--surfaces-json <path>|-` for anything else (any kind, or several surfaces
in one post) as a raw JSON array matching the MCP `publish_post` schema.
Add `--json` to the command for machine-readable output instead of the
human-readable summary.

The default ambient feed reads optional `feedKind`, `summary`, `detail`, and
`evidenceLinks` fields from the surface JSON passed through
`--surfaces-json`. `feedKind` must be one of `shipped`, `report`, `blocked`,
`question`, `note`, `digest`, `release`, or `receipt`; omitted posts appear as
`report`.

## Local Gate

```sh
./scripts/check.sh
```

Equivalent to `cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`.

## Red Lines

- Do not reintroduce a comment/feedback surface, an `agent_seq` cursor, or a
  reply tool — Glass is one-way by deliberate operator ruling (glass-912),
  not by omission.
- Do not bypass the sandboxed-render security posture (CSP `sandbox`
  without `allow-same-origin`) when adding a new surface kind or render path.
- Do not add app-specific responder logic here — Glass owns the live
  surface, not downstream repair or triage.
