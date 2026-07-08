# Glass Architecture

Glass is a single Rust process with one SQLite database. The public interface is
intentionally smaller than the implementation:

- HTTP API for sessions, posts, assets, sandbox rendering, setup docs, and
  MCP-compatible JSON-RPC.
- Shared-shell HTML for the operator IA: `/` (Now), `/needs-you`, `/reports`,
  `/reports/:id`, `/agent/:agent`, and `/session/:id`. `/clips` is retired as
  a human place and redirects to Now; clip capture remains in `/api/clips`.
- Now polls `/api/now`, which joins active Powder claims with Glass sessions
  for the attention-ordered fleet column and includes the Wire, a reverse-chron
  evidence feed from native Glass posts plus configured Landmark release events.
- The drill-down routes keep polling `/api/posts/recent` while loading rich
  surfaces through `/s/:post_id?part=N`.
- Reports persist generated documents in SQLite. `POST /api/reports` writes
  activity, fleet, backlog, and review-index reports; `GET /reports` lists the
  library; `GET /reports/:id` reopens the stored document. The serve process
  also schedules daily and weekly standing activity digests at local 06:00.
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

## Now And The Wire

`GET /api/now` is the root read model. It returns stats, fleet rows, Wire rows,
dead-session data, and upstream notices. Fleet rows come from active Powder
claims joined to live Glass sessions; claimed-quiet is a first-class state when
Powder says an agent is working and Glass has not received posts from it.

The Wire rows are also available through `GET /api/feed/recent`. That endpoint
is a projection over box-native sources only: the local SQLite post store and
the configured `GLASS_LANDMARK_RELEASE_EVENTS_URL`. Producers can declare
Bridge-compatible row semantics by putting `feedKind`, `summary`, `detail`, and
`evidenceLinks` on any posted surface. If no `feedKind` is declared, the post
appears as a `report` row. Evidence links are declared links plus native Glass
detail links (`/session/...`, `/s/...`, and `/a/...`); the viewer opens a
read-only detail dialog and exposes no reply or approval channel.

## Needs You

`/needs-you` and `GET /api/needs-you` read Powder awaiting-input runs. Answers
posted to `POST /api/needs-you/answer` are relayed to Powder's answer endpoint.
This is still one-way for Glass: the answer belongs to Powder's work ledger and
does not create a reply channel to the producing agent inside Glass.

## Reports

The `reports` table stores report kind, scope, window, rendered HTML, metadata,
generation time, and requester. Manual generation always creates a new report
row. The standing digest scheduler is narrower: before inserting a daily or
weekly activity digest, it skips when any fleet activity digest already exists
for that exact window.

Legacy human routes `/rep1` and `/backlog/:repo` redirect to `/reports` with
the matching generator kind/scope selected. The API routes
`/api/rep1/:window`, `/api/backlog/:repo`, and `/api/window-report/:window`
remain available for consumers that already poll them.

## Dead Session Demotion

`LIVE_WINDOW_SECONDS` (600s) marks a session dead once it goes that long
without a new post. `GET /api/posts/recent` reports `isLive` per session;
the Now column on `/` renders only active claimed/live peers, keeping dead
sessions out of the primary operator scan. Dead sessions keep
their full post history on their own agent/session feed — nothing is
dropped, only demoted from the primary column.

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
