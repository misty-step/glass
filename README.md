# Glass

Glass is the Misty Step live stage: a Rust service where agents publish typed
surfaces during work and the operator watches them from the tailnet. Glass is
ONE-WAY — there is no reply channel back to the producing agent; operator
communication happens somewhere else.

It is intentionally Sideshow-compatible at the protocol level while replacing
the runtime with a native Rust service.

## What Ships In This MVP

- Versioned posts grouped into per-agent sessions. Every agent gets its own
  live status feed at `/agent/:agent`, in addition to the `/session/:id`
  drill-down.
- Typed surfaces: `html`, `diff`, `image`, `trace`, `markdown`, `terminal`,
  `mermaid`, `json`, `code`, and `metric` (a label+value chip).
- Content-addressed assets at `/a/:sha256`.
- Sandboxed render URLs at `/s/:post_id?part=N` with CSP sandbox response
  headers, not iframe attributes alone.
- Dead sessions (quiet for longer than `LIVE_WINDOW_SECONDS`) are demoted out
  of the primary fleet-wall rail; the API still serves their full post
  history under their own agent/session feed.
- A viewer built on the Misty Step Aesthetic kit (`/aesthetic.css`), with a
  keyed post-diff renderer so polling never re-mounts a live surface's
  iframe unless that post actually changed.
- `glass publish` CLI subcommand wrapping the same core the MCP tool calls,
  plus curl-first setup docs and a small stateless MCP-compatible HTTP
  endpoint for consumers without CLI access.

## Quickstart

```sh
cargo run -- serve --bind 127.0.0.1:9041 --db data/glass.db
```

Open `http://127.0.0.1:9041`.

Agents with a local `glass` binary should publish through the CLI rather than
hand-rolled curl — see [`SKILL.md`](SKILL.md):

```sh
glass publish --db data/glass.db --title "Protocol proof" \
  --agent codex-glass-901 --session-title "glass-901 native build" \
  --markdown "Glass is receiving typed surfaces." \
  --terminal "cargo test --workspace
5 passed"
```

The curl examples below remain the documented protocol-level contract for
remote or MCP-only consumers without CLI access.

Publish a codex lane surface:

```sh
curl -s -X POST http://127.0.0.1:9041/api/posts \
  -H 'content-type: application/json' \
  --data '{
    "agent": "codex-glass-901",
    "sessionTitle": "glass-901 native build",
    "title": "Protocol proof",
    "surfaces": [
      { "kind": "markdown", "markdown": "Glass is receiving typed surfaces." },
      { "kind": "terminal", "text": "cargo test --workspace\\n5 passed" }
    ]
  }' | jq .
```

Publish a second producer session, including a metric surface:

```sh
curl -s -X POST http://127.0.0.1:9041/api/posts \
  -H 'content-type: application/json' \
  --data '{
    "agent": "claude-session",
    "sessionTitle": "parallel producer",
    "title": "Second lane",
    "surfaces": [
      { "kind": "json", "data": { "producer": "claude-session", "status": "visible" } },
      { "kind": "metric", "label": "tests", "value": "42 passed" }
    ]
  }' | jq .
```

Every running agent has its own feed:

```sh
curl -s "http://127.0.0.1:9041/api/posts/recent?agent=claude-session" | jq .
```

## Verified-Live Walkthrough

From a fresh checkout, the operator path should end with a running service that
has proved its HTTP API and SQLite backing store:

```sh
git pull --ff-only
cargo build --release
mkdir -p .glass-live
target/release/glass serve --bind 127.0.0.1:9041 --db .glass-live/glass.db
```

In another shell, run the live doctor against that process:

```sh
target/release/glass doctor \
  --url http://127.0.0.1:9041 \
  --db .glass-live/glass.db
```

`glass doctor` fetches the surface-kind contract, publishes a disposable
`glass-doctor` probe post through HTTP, reads it back through the same HTTP
API, then reopens the named SQLite file and verifies the probe session is
present there. A successful run prints `glass doctor ok` with the URL, DB
path, session count, probe session, and probe post. The probe self-cleans
after the round trip is proven, so it does not accumulate on the stage.

Common failures:

- `surface kinds endpoint returned an error status`: the URL is not a Glass
  server or the service is not reachable.
- `doctor probe read-back returned a different post than was published`: the
  publish/read path is broken; do not deploy.
- `probe session ... was not present`: the service is not using the DB path you
  passed to `--db`.

For local supervision and the tailnet `:9040` service slot, see
[`docs/deployment.md`](docs/deployment.md).

## Agent Setup

Agents can fetch live instructions from the running service:

```sh
curl -s http://127.0.0.1:9041/setup
curl -s http://127.0.0.1:9041/agent-howto
```

MCP-capable clients can use the stateless HTTP endpoint at `/mcp`. The MVP tool
surface is `publish_post` only — Glass is one-way, so there is no feedback or
reply tool. Agents with a local `glass` binary have a shipped skill at
[`SKILL.md`](SKILL.md) documenting the `publish`/`doctor` contract, matching
the pattern used by misty-canary/misty-powder/misty-bitterblossom.

## Tailnet Posture

Glass is designed to run on the workstation or Sanctum behind Tailscale. The
standing deployment contract and rollback path live in
[`docs/deployment.md`](docs/deployment.md). Campaign lanes may cut over the
tailnet service only when the claimed Powder card explicitly asks for a
verified-live Glass deployment; otherwise merge code and leave deployment or
`tailscale serve` changes to an operator decision.

## Gate

```sh
./scripts/check.sh
```

The same command runs in GitHub Actions.
