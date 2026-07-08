# On-Demand Synthesis Endpoint Contract

Glass is only the consumer for glass-919. The synthesis engine and HTTP surface
belong in `weave/apps/fleet-retro`; until weave schedules that work,
`GLASS_SYNTHESIS_ENDPOINT` may be unset and Glass will fall back to the nearest
shelf window.

## Consumer Wiring

Glass calls this endpoint for two consumer seams:

1. Custom/arbitrary streaming window reports on:

```text
GET /api/window-report/{window}?since=<rfc3339>&until=<rfc3339>&scope=<scope>
```

2. `/reports` ask-and-render activity digests when a query misses the local
   cache and `GLASS_SYNTHESIS_ENDPOINT` is set.

`daily` and `weekly` window-report routes never call this endpoint; they keep
the existing fleet-retro shelf fetch path.

Set:

```text
GLASS_SYNTHESIS_ENDPOINT=https://weave.example/internal/fleet-retro/synthesize
```

## Request

Glass sends `POST` with JSON:

```json
{
  "window": "custom",
  "since": "2026-07-07T00:00:00Z",
  "until": "2026-07-07T01:00:00Z",
  "scope": "fleet",
  "contract": "glass.report_components.v1"
}
```

Fields:

- `window`: caller-facing window slug from the Glass route, such as `custom`,
  `30m`, `1h`, or another consumer-defined label.
- `since`: inclusive RFC3339 lower bound.
- `until`: exclusive RFC3339 upper bound.
- `scope`: synthesis scope. `fleet` is the default; narrower scopes should use
  stable strings like `repo:glass`.
- `contract`: optional for the older window-report seam. When present and set
  to `glass.report_components.v1`, the endpoint should return the component
  list schema below.
- `context`: present for `/reports` activity digests. It carries bounded raw
  material (`posts`, `clips`, `powder.completed`) for the selected
  scope/window.

## Report Component Schema

The `/reports` generative-UI contract is a serde-deserializable component list:

```json
{
  "components": [
    {
      "kind": "stat_band",
      "figures": [
        { "value": "58", "label": "completed cards" },
        { "value": "1", "label": "blocked", "warn": true }
      ]
    },
    {
      "kind": "bars",
      "series": [
        { "label": "17:00", "value": 14 },
        { "label": "18:00", "value": 11 }
      ]
    },
    { "kind": "prose", "text": "The Glass redesign shipped end to end." },
    {
      "kind": "evidence_chips",
      "links": [{ "label": "glass-941", "href": "/reports/R-001" }]
    }
  ]
}
```

Accepted component kinds and fields:

- `stat_band`: `{ "figures": [{ "value": string, "label": string, "warn"?: bool }] }`
- `spark`: `{ "series": [{ "label": string, "value": number }] }`
- `bars`: `{ "series": [{ "label": string, "value": number }] }`
- `meters`: `{ "pairs": [{ "label": string, "value": number }] }`
- `pipeline`: `{ "stages": [{ "label": string, "state": "done"|"active"|"blocked"|"pending", "note"?: string }] }`
- `trail`: `{ "events": [{ "time": string, "title": string, "kind"?: string, "agent"?: string, "href"?: string }] }`
- `callouts`: `{ "lines": [{ "text": string, "status"?: string, "href"?: string }] }`
- `evidence_chips`: `{ "links": [{ "label": string, "href": string }] }`
- `diff_exhibit`: `{ "file": string, "lines": [{ "state": "add"|"del"|"ctx", "text": string }] }`
- `terminal_exhibit`: `{ "lines": [string] }`
- `pull_quote`: `{ "text": string, "by"?: string }`
- `badge_row`: `{ "badges": [{ "label": string, "value"?: string, "status"?: string }] }`
- `icon_row`: `{ "rows": [{ "text": string, "icon"?: string, "meta"?: string }] }`
- `prose`: `{ "text": string }`
- `fig_caption`: `{ "text": string }`

Every string is escaped by Glass before rendering. The endpoint chooses
components and fills plain data; deterministic Rust owns the renderer.

## Streamed Response

Preferred response:

```text
Content-Type: text/event-stream
```

Glass emits its own initial `skeleton` event, then relays upstream SSE events
unchanged. The weave endpoint should send JSON `data:` payloads:

```text
event: token
data: {"stage":"token","text":"The"}

event: partial
data: {"stage":"partial","narrative":[[{"type":"text","text":"The fleet..."}]]}

event: full
data: {"stage":"full","components":[{"kind":"prose","text":"..."}]}
```

The terminal `full` event is required. For the new `/reports` path it should
carry `components` matching `glass.report_components.v1`.

Backward compatibility: the terminal `full` event may still carry
`{"spec": {... RetroSpec ...}}`. Glass accepts that old shape and renders it
through the existing `glance_catalog` compatibility path. That shape should
remain citation-gated and valid for `glance_catalog::LayoutProfile::REPORT`.

## Citation Preservation

All rich prose uses `glance_catalog::InlineNode`. Citation nodes must survive
unchanged:

```json
{"type":"cite","text":"evidence sentence","ref_id":"powder:glass-919"}
```

Glass treats `ref_id` as opaque and only relays it. The weave endpoint owns the
citation gate and must not emit an uncited final narrative.

## Non-Streaming Compatibility

Glass also accepts a non-streaming `application/json` response as a
compatibility path for early weave implementations. For component reports, the
body may be either:

```json
{"components":[{"kind":"prose","text":"..."}]}
```

or the legacy:

```json
{"stage":"full","spec":{...}}
```

Streaming remains preferred for custom window-report misses. The `/reports`
page renders only after the terminal component list or legacy spec arrives.

## Failure Semantics

On unset endpoint, connection failure, non-2xx status, or invalid JSON, Glass
does not crash and does not render blank. It emits:

1. `skeleton`
2. `fallback` naming the custom window and selected shelf window (`daily` or
   `weekly`)
3. `full` with the shelf spec if available, otherwise `error` with the missing
   configuration or upstream failure
