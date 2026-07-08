# Glass Design Contract

Operator-ratified via design lab rounds 1–3 (`explorations/lab-001`,
2026-07-08). This file names the locked decisions the code must render.
The Misty Step Aesthetic kit (`assets/aesthetic.css`, DESIGN.md in the
`aesthetic` repo) remains the underlying law; this contract binds how
Glass composes it. When a lab round re-opens a decision, update this
file in the same change.

## The three primitives

Glass has exactly three content primitives. Every screen is one of them
or a scoped composition of them:

1. **Now** — which agents are active, which are claimed-quiet, their
   current state. One column, attention-ordered.
2. **The Wire** — the single raw activity feed (releases, merges,
   QA/evidence receipts, blocked, questions, notes). One component,
   scope-parametric: identical fleet-wide and per-agent. Blocked and
   question events always pin above the chronological flow.
3. **Reports** — ad-hoc synthesis over scope × window, rendered
   immediately in place. Cached (same query serves the cached render,
   marked quietly), never curated into a front-and-center library.

Retired: Clips as a place (fold captures into the wire as an event
kind). Needs You ships as-is (locked at its current baseline).

## Locked compositions (lab option IDs are the reference renders)

- **Shell — `SHELL-7`.** One left rail, shadcn-grade in kit vocabulary:
  per-place Lucide icons, labeled groups (PLACES, SCOPES), a pinned
  account/foot block, active state carried by ink weight plus a hairline
  indicator bar. Wordmark is **GLASS** — always caps (fleet law,
  aesthetic-030). Phone: a thin top bar (burger + GLASS + needs-you
  count) opening a slide-over sheet — NOT a thick bottom bar; this app
  overrules the kit's bottom-chrome default by operator ruling.
- **Now — `NOW-9`.** A single uninterrupted column, no group dividers:
  attention is an ink gradient — blocked brightest, then live, then
  quiet fading. Name-led rows with trailing state glyph; sort control as
  a quiet settings row. State copy is terse ("quiet · 1d", not
  "idle-claimed, quiet · no posts yet"). Summary numbers ride compact
  icon badges, never prose runs; glyphs vertically centered.
- **Wire — `WIRE-10`.** The dense timestamp tape at the 13px instrument
  register; each kind is a Lucide glyph (not a word-chip) decoded by a
  one-line legend; blocked + questions pinned in a warn band on top.
- **Report query — `REP-8`.** A plain-English sentence with editable
  slot tokens: "Show me [the whole fleet] over [the past 24h]". Report
  renders beneath, immediately. Cache note reads quietly
  ("cached · generated 18:28").
- **Report document — round 3 open.** Whatever composition wins
  (DOC-13…18), the invariant is ratified: the synthesis is COMPOSED
  GENERATIVE UI assembled from a component library — stat bands,
  sparks/bars/meters, pipeline flows, trails, diff/terminal/code
  exhibits, badges, callouts, icon rows — with deliberate rhythm.
  Never a prose wall. The synthesizing model picks and fills
  components; deterministic code owns the component renderers.
- **Agent page — round 2 open.** It is a scoped composition of the same
  primitives (state header + scoped wire + scoped report entry), never
  a bespoke surface. Arrangement (stacked / tabs / side-by-side /
  overlay) pending verdict.

## Standing rules from the rounds

- Prefer icons, badges, and labels over runs of plain text for state.
- Glyphs center against adjacent text (`align-self: center`).
- Headings must be self-explanatory to a fresh operator.
- Desktop and phone are co-equal deliverables; no dead chrome at 390px.
- One-way law (glass-912) unmoved: no reply channels anywhere.
