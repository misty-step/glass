# Glass Vision

Glass is the operator live stage: watch the factory think.

## The live stage

Glass answers three operator questions, and those questions govern the
information architecture:

- Now: what is working right now, which agents are live, and which claimed
  lanes are quiet.
- Needs you: what decision, action, or answer is waiting on the operator.
- What happened: what durable reports, clips, and review artifacts explain a
  window of work.

The root screen is Now. It joins Powder claims with Glass sessions so no
working agent is invisible. A claimed card with no posts is still a visible
state, not a missing row: claimed-quiet means the board says work exists and
Glass has not seen evidence yet.

Reports are persisted artifacts over any window. Daily and weekly activity
digests are standing reports in the same library as operator-generated
reports; review, backlog, fleet, and activity reports are all durable URLs,
not transient pages.

Surfaces are typed and sandboxed. The frozen kinds are `html`, `diff`, `image`,
`trace`, `markdown`, `terminal`, `mermaid`, `json`, `code`, and `metric`.
Agent-authored rich content is served from real URLs with a CSP sandbox that
does not allow same-origin.

Glass is one-way. Operator ruling glass-912, ratified 2026-07-07: no reply
channel, ever. Glass shows what agents are doing; communication and approval
happen in Powder or another work surface.

Glass runs tailnet-only, bastion-supervised, and Rust-first. Deterministic Rust
owns storage, policy, routing, sandboxing, and gates.

## Non-goals

- Glass is not the work ledger. Powder owns cards, claims, answers, and
  durable operator communication.
- Glass is not standing docs. Glance and repository docs own long-lived
  reference material.
- Glass has no accounts. Tailnet and bastion supervision provide the boundary.
