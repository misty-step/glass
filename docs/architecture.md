# Glass Architecture

Glass is a single Rust process with one SQLite database. The public interface is
intentionally smaller than the implementation:

- HTTP API for sessions, posts, assets, sandbox rendering, setup docs, and
  MCP-compatible JSON-RPC.
- Viewer HTML served from `/`, `/agent/:agent`, and `/session/:id`, polling
  `/api/posts/recent` and rendering trusted data directly while loading rich
  surfaces through `/s/:post_id?part=N`.
- Domain methods on `Glass` that tests exercise directly for exact asset
  semantics.

## Surface Grammar

The frozen kind list is owned by `SURFACE_KINDS` in `src/lib.rs`. Any new kind
must choose one of two rendering paths:

- Data rendered by the trusted viewer with escaped text/attributes.
- A string served as a real sandboxed URL with a CSP response header.

There is no third DOM sink for agent-authored content.

## One-Way (glass-912)

Glass has no reply channel back to the producing agent. The earlier two-way
comment surface (`comments` table, `sessions.agent_seq` cursor,
`GET /api/comments`, `wait_for_feedback`/`reply_to_user` MCP tools) was
deleted, not hidden, by operator ruling 2026-07-07: "I should see what's
happening but communication should happen somewhere else." `Store::migrate`
drops the `comments` table and the `agent_seq` column from any database
created under the earlier schema.

## Per-Agent Status Feeds

Every running agent has its own feed at `/agent/:agent`, filtered
server-side by `GET /api/posts/recent?agent=...` so a quiet agent's older
posts are never pushed out by a busier one's. `/session/:id` keeps the
single-session drill-down via `?sessionId=...`. Both routes serve the same
`VIEWER_HTML`; the client reads its own route out of `window.location` since
there is no server-side templating.

## Dead Session Demotion

`LIVE_WINDOW_SECONDS` (600s) marks a session dead once it goes that long
without a new post. `GET /api/posts/recent` reports `isLive` per session;
the fleet wall on `/` renders only live sessions as peers, collapsing dead
ones into an archive so they never crowd out active work. Dead sessions keep
their full post history on their own agent/session feed — nothing is
dropped, only demoted from the primary rail.

## Sandbox Boundary

`/s/:post_id?part=N` always sends:

```text
Content-Security-Policy: sandbox ...
```

The policy does not include `allow-same-origin`. The iframe `sandbox` attribute
in the viewer is defense in depth; the response header is the security boundary
because direct URL opens and screenshot tools bypass iframe attributes.

## Assets

Asset ids are `hex(sha256(bytes))`. Re-uploading identical bytes returns the
same id and touches the asset.
