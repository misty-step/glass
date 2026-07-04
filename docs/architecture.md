# Glass Architecture

Glass is a single Rust process with one SQLite database. The public interface is
intentionally smaller than the implementation:

- HTTP API for sessions, posts, comments, assets, sandbox rendering, setup docs,
  and MCP-compatible JSON-RPC.
- Viewer HTML served from `/`, polling `/api/posts/recent` and rendering trusted
  data directly while loading rich surfaces through `/s/:post_id?part=N`.
- Domain methods on `Glass` that tests exercise directly for exact feedback and
  asset semantics.

## Surface Grammar

The frozen kind list is owned by `SURFACE_KINDS` in `src/lib.rs`. Any new kind
must choose one of two rendering paths:

- Data rendered by the trusted viewer with escaped text/attributes.
- A string served as a real sandboxed URL with a CSP response header.

There is no third DOM sink for agent-authored content.

## Feedback Cursor

`sessions.agent_seq` is the highest comment sequence delivered to the producing
agent. Both `GET /api/comments?session_id=...` and publish/update piggyback
responses read comments after that cursor and advance it. Returned feedback is
filtered to `author = "user"`, while the cursor advances past every comment in
the window so multiple integration tiers do not redeliver old feedback.

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
