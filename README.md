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
- Curl-first setup docs plus a small stateless MCP-compatible HTTP endpoint.

## Quickstart

```sh
cargo run -- serve --bind 127.0.0.1:9041 --db data/glass.db
```

Open `http://127.0.0.1:9041`.

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

## Agent Setup

Agents can fetch live instructions from the running service:

```sh
curl -s http://127.0.0.1:9041/setup
curl -s http://127.0.0.1:9041/agent-howto
```

MCP-capable clients can use the stateless HTTP endpoint at `/mcp`. The MVP tool
surface includes `publish_post`, `wait_for_feedback`, and `reply_to_user`.

## Tailnet Posture

Glass is designed to run on the workstation or Sanctum behind Tailscale. This
repo does not yet declare an autonomous deploy path, so campaign lanes should
merge code and leave deployment or `tailscale serve` cutover to an operator
decision unless a future repo doc says otherwise.

## Gate

```sh
./scripts/check.sh
```

The same command runs in GitHub Actions.
