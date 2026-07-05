# Glass

Glass is the Misty Step live stage: a Rust service where agents publish typed
surfaces during work, the operator watches them from the tailnet, and comments
flow back to the producing agent exactly once.

It is intentionally Sideshow-compatible at the protocol level while replacing
the runtime with a native Rust service.

## What Ships In This MVP

- Versioned posts grouped into per-agent sessions.
- Typed surfaces: `html`, `diff`, `image`, `trace`, `markdown`, `terminal`,
  `mermaid`, `json`, and `code`.
- Content-addressed assets at `/a/:sha256`.
- Sandboxed render URLs at `/s/:post_id?part=N` with CSP sandbox response
  headers, not iframe attributes alone.
- Feedback comments delivered through one server-side `agent_seq` cursor shared
  by `GET /api/comments?...` and write-response `userFeedback` piggybacking.
- A vanilla viewer with light, dark, and system theme modes.
- `glass publish` / `glass feedback` CLI subcommands wrapping the same core
  the MCP tools call, plus curl-first setup docs and a small stateless
  MCP-compatible HTTP endpoint for consumers without CLI access.

## Quickstart

```sh
cargo run -- serve --bind 127.0.0.1:9041 --db data/glass.db
```

Open `http://127.0.0.1:9041`.

Agents with a local `glass` binary should publish and drain feedback through
the CLI rather than hand-rolled curl — see [`SKILL.md`](SKILL.md):

```sh
glass publish --db data/glass.db --title "Protocol proof" \
  --agent codex-glass-901 --session-title "glass-901 native build" \
  --markdown "Glass is receiving typed surfaces." \
  --terminal "cargo test --workspace
5 passed"

glass feedback --db data/glass.db --session <session_id> --wait 1
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

Publish a second producer session:

```sh
curl -s -X POST http://127.0.0.1:9041/api/posts \
  -H 'content-type: application/json' \
  --data '{
    "agent": "claude-session",
    "sessionTitle": "parallel producer",
    "title": "Second lane",
    "surfaces": [
      { "kind": "json", "data": { "producer": "claude-session", "status": "visible" } }
    ]
  }' | jq .
```

Drain feedback once:

```sh
curl -s "http://127.0.0.1:9041/api/comments?session_id=<session_id>&wait=1" | jq .
```

Every publish/update response may also include `userFeedback`; that is the same
delivery stream and advances the same cursor.

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
`glass-doctor` probe post through HTTP, posts a user feedback probe, drains it
exactly once through the shared `agent_seq` cursor, then reopens the named
SQLite file and verifies the probe session is present there. A successful run
prints `glass doctor ok` with the URL, DB path, session count, probe session,
probe post, and `feedback=delivered-once`. The probe remains in the configured
database as a durable verification record; the command is not read-only.

Common failures:

- `surface kinds endpoint returned an error status`: the URL is not a Glass
  server or the service is not reachable.
- `doctor feedback probe was redelivered`: the feedback cursor contract is
  broken; do not deploy.
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
surface includes `publish_post`, `wait_for_feedback`, and `reply_to_user`.
Agents with a local `glass` binary have a shipped skill at
[`SKILL.md`](SKILL.md) documenting the `publish`/`feedback`/`doctor` contract,
matching the pattern used by misty-canary/misty-powder/misty-bitterblossom.

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
