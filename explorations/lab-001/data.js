// Real fleet content, 2026-07-08. Everything here actually happened; the
// "posts" simulate the beat contract (roster-961) applied to today's real
// work so rich-card options render truthfully.
const DATA = {
  stats: { live: 6, quiet: 8, needYou: 3, postsToday: 23, sessionsToday: 7, freshness: "12s" },

  // wall agents: state = publishing | blocked | quiet | done
  agents: [
    { name: "glass-933-codex", state: "publishing", card: "glass-933", act: "shipped: persisted reports library — PR #18 merged", age: "12s", trace: [1,1,1,1] },
    { name: "cerberus", state: "publishing", card: "review PR #19", act: "verifying: IA e2e sweep on merged main", age: "48s", trace: [1,1,1] },
    { name: "canary-lane", state: "blocked", card: "canary-100", act: "blocked 42m: awaiting scoped ingest key — ask in Needs you", age: "42m", trace: [1,1,0] },
    { name: "glass-937-codex", state: "publishing", card: "glass-937", act: "milestone: standing digest scheduler green", age: "3m", trace: [1,1,1,1] },
    { name: "roster-961-codex", state: "publishing", card: "roster-961", act: "shipped: Glass beat contract in lane cards — PR #12", age: "9m", trace: [1,1] },
    { name: "bastion-deploy", state: "publishing", card: "sanctum PR #63", act: "fly deploy rolling — machine healthy, glass on 6f00287", age: "6m", trace: [1,1,1] },
    { name: "lead-daybook", state: "quiet", card: "glass-930", act: "claimed 4m ago · no posts yet", age: "4m" },
    { name: "bridge-2026-07-07c", state: "quiet", card: "bridge-016", act: "claimed 1d ago · no posts yet", age: "1d" },
    { name: "builder-landmark-lane", state: "quiet", card: "landmark-207", act: "claimed 1d ago · no posts yet", age: "1d" },
    { name: "builder-sonnet-crucible-chew", state: "quiet", card: "crucible-118", act: "claimed 1d ago · no posts yet", age: "1d" },
    { name: "team-lead", state: "quiet", card: "session-15", act: "claimed 1d ago · no posts yet", age: "1d" },
    { name: "claude-fable-lead-daybook", state: "quiet", card: "harness-kit-130", act: "claimed 1d ago · no posts yet", age: "1d" },
    { name: "lane-sites-b", state: "quiet", card: "misty-step-906", act: "claimed 3d ago · no posts yet", age: "3d" },
    { name: "lane-landmark-907", state: "quiet", card: "landmark-907", act: "claimed 3d ago · no posts yet", age: "3d" },
  ],

  finished24h: { agents: 7, sessions: 31 },

  wire: [
    { t: "18:31:02", agent: "glass-933-codex", kind: "shipped",  cat: 1, title: "PR #18 merged — reports persist with stable URLs" },
    { t: "18:28:40", agent: "phaedrus",        kind: "report",   cat: 4, title: "R-001 generated: Activity digest — fleet, yesterday" },
    { t: "18:24:11", agent: "bastion-deploy",  kind: "receipt",  cat: 6, title: "fly deploy healthy — glass 6f00287 live on :10003" },
    { t: "17:58:03", agent: "glass-937-codex", kind: "shipped",  cat: 1, title: "PR #19 merged — VISION.md + standing digests" },
    { t: "17:44:36", agent: "canary-lane",     kind: "blocked",  cat: 0, title: "canary-100 waiting on a scoped ingest key" },
    { t: "17:31:19", agent: "cerberus",        kind: "report",   cat: 4, title: "e2e suite 9/9 green on merged main" },
    { t: "17:10:05", agent: "sweep",           kind: "question", cat: 2, title: "OK to scrub overmind fixture hostnames?" },
    { t: "16:42:27", agent: "glass-931-codex", kind: "shipped",  cat: 1, title: "PR #15 merged — one shared rail on every route" },
    { t: "16:12:44", agent: "landmark",        kind: "release",  cat: 3, title: "glance v0.9.2 tagged and published" },
    { t: "15:54:43", agent: "lead-daybook",    kind: "note",     cat: 5, title: "redesign fan-out dispatched: 931/932/933/934" },
  ],

  asks: [
    { title: "Provision a scoped Canary ingest key for Glass?", agent: "canary-lane", card: "canary-100", age: "42m", blocks: "glass-100 live-fire", body: "Canary self-report is deployed but dormant. Need CANARY_API_KEY scoped to the glass service. Options: mint a new ingest key on canary-obs, or reuse the fleet key (not recommended)." },
    { title: "OK to scrub tailnet hostnames from the overmind test fixture?", agent: "sweep", card: "bastion-945", age: "1h", blocks: "master publicability gate is red", body: "vendor/overmind/tests/fixtures/registry-snapshot-20260707.json embeds bastion.tail5f5eb4.ts.net. Scrubbing changes Overmind's test fixtures; their lane should confirm." },
    { title: "Adopt the Mint proxy for Glass→Powder credentials?", agent: "mint-lane", card: "mint-906", age: "3h", blocks: "standing key retirement", body: "End state: glass holds no Powder key; awaiting-input routes through mint's proxy with a glass actor policy. Needs a needs_you.rs change on the glass side." },
  ],
  answered: [
    { title: "Ship the marketing hero without the wash panel?", agent: "glass-923-codex", answer: "Yes — flat hero, kit typography contract.", when: "yesterday" },
    { title: "Axe Bridge outright or hold for parity?", agent: "glass-bridge", answer: "Fully collapse it. Rebuild what we need in Glass.", when: "yesterday" },
  ],

  reports: [
    { id: "R-004", title: "Weekly digest — 2026-W28", window: "Jul 6 – Jul 12", scope: "fleet", by: "auto · Mon 06:00", kind: "activity-digest" },
    { id: "R-003", title: "Daily digest — 2026-07-08", window: "Jul 8", scope: "fleet", by: "auto · 06:00", kind: "activity-digest" },
    { id: "R-002", title: "Backlog — glass", window: "—", scope: "repo glass", by: "phaedrus · 18:40", kind: "backlog" },
    { id: "R-001", title: "Activity digest — fleet", window: "Jul 7 – Jul 8", scope: "fleet", by: "phaedrus · 18:28", kind: "activity-digest" },
  ],

  // R-001 content (real): 58 completions on 2026-07-07→08
  r001: {
    title: "Activity digest — fleet",
    window: "2026-07-07 → 2026-07-08",
    generated: "2026-07-08 18:28 UTC · phaedrus",
    totals: { completed: 58, posts: 0, clips: 0, blocked: 0 },
    completions: [
      { card: "glass-926", title: "The ambient feed is Glass's default view — rebuild Bridge's FEED natively", repo: "glass", pri: "p1", at: "23:22" },
      { card: "glass-902", title: "The review surface: three-context narrated diff as a stage artifact", repo: "glass", pri: "p1", at: "23:21" },
      { card: "linejam-942", title: "Own the identity seam: themed auth + a guest path back to their poems", repo: "linejam", pri: "p1", at: "21:46" },
      { card: "canary-101", title: "Fleet Canary coverage → comprehensive: all error paths + panics", repo: "canary", pri: "p1", at: "21:36" },
      { card: "cairn-101", title: "Comprehensive Canary coverage for cairn", repo: "cairn", pri: "p2", at: "21:30" },
      { card: "glass-905", title: "Application floor: coverage ratchet + rendered e2e gates", repo: "glass", pri: "p2", at: "20:58" },
      { card: "glass-921", title: "Rotate the leaked Powder key", repo: "glass", pri: "p1", at: "20:12" },
      { card: "powder-917", title: "Key lifecycle: revoke leaves no orphan claims", repo: "powder", pri: "p2", at: "19:44" },
    ],
    byRepo: [ ["glass", 14], ["canary", 9], ["linejam", 7], ["powder", 6], ["cairn", 5], ["bastion", 5], ["roster", 4], ["other", 8] ],
  },

  clips: [
    { title: "Claimed-quiet card renders on live Powder join", agent: "glass-932-codex", session: "glass-932 build", caption: "First live render of the wall against production Powder — 14 agents, all quiet.", when: "18:26" },
    { title: "R-001 first persisted report", agent: "phaedrus", session: "operator", caption: "Yesterday's fleet digest: 58 completions, stable URL.", when: "18:28" },
    { title: "e2e: fresh operator reaches last week's digest in 2 clicks", agent: "glass-937-codex", session: "glass-937 build", caption: "The oracle passing in CI.", when: "17:52" },
  ],

  // one agent's day, for drill-down options (real: the glass-933 lane incl. crash)
  agentDetail: {
    name: "glass-933-codex", card: "glass-933", state: "publishing",
    stateLine: "shipped 12s ago — PR #18 merged",
    session: "glass-933: persisted reports library",
    trail: [
      { t: "18:31", kind: "shipped", body: "PR #18 merged — reports persist with stable URLs; /rep1 and /backlog fold in" },
      { t: "18:04", kind: "report", body: "gates green on rebased branch: check.sh, coverage 74.22% vs 74.0 floor, e2e 7/7" },
      { t: "17:26", kind: "note", body: "recovery lane: inherited 350 uncommitted lines, rebased over 934+932" },
      { t: "17:05", kind: "blocked", body: "machine out of disk mid-gate (os error 28) — lane died, work preserved" },
      { t: "16:47", kind: "report", body: "reports table migration + generator + library routes drafted" },
      { t: "16:45", kind: "note", body: "session start: glass-933 off main 23f5a3b" },
    ],
  },
};

// Round 2: the REAL synthesis payload for report-doc options — the shape a
// generative-UI report renderer consumes. All facts are true fleet history,
// window 2026-07-07 18:00 → 2026-07-08 18:30 UTC.
DATA.synthesis = {
  query: { scope: "fleet", window: "past 24h", resolved: "2026-07-07 18:00 → 2026-07-08 18:30 UTC", cached: false, tookMs: 14200 },
  headline: "The fleet redesigned its own operator surface and shipped it live",
  tldr: "58 Powder cards completed across 8 repos. The dominant thread: the Glass operator-UI redesign went from spec to deployed in one day — six lanes, five glass PRs plus roster and sanctum, one disk-full crash recovered without losing work. One ask has been blocked 42 minutes on a Canary ingest key.",
  numbers: { completed: 58, prs: 8, deploys: 1, incidents: 1, blocked: 1, questions: 2, releases: 1 },
  themes: [
    { title: "The Glass redesign shipped end to end", kind: "velocity",
      body: "A full operator-UI redesign (epic glass-930) went spec → build → deploy inside the window: shared shell on every route (PR #15), the NOW wall joined from Powder claims and live sessions (PR #17), a reports engine (PR #18), needs-you/clips polish (PR #16), VISION.md + standing digests (PR #19). Roster lanes gained a publishing contract (roster PR #12) and the box now runs the result (sanctum PR #63, fly deploy healthy).",
      evidence: [["glass PR #15","#0"],["glass PR #17","#0"],["glass PR #18","#0"],["sanctum PR #63","#0"]] },
    { title: "One incident, recovered clean", kind: "reliability",
      body: "The glass-933 build lane died mid-gate when the machine ran out of disk (os error 28, 217MB free). ~350 uncommitted lines were preserved; a recovery lane inherited the worktree, rebased over two merged siblings, and shipped. 41GB reclaimed by clearing stale cargo targets across six repos.",
      evidence: [["receipt: glass-933 recovery","#0"]] },
    { title: "Attention debt is accruing", kind: "risk",
      body: "canary-100 has been blocked 42 minutes on a scoped ingest key — it gates glass-100 live-fire observability. Two questions wait: scrubbing tailnet hostnames from an overmind fixture (master publicability gate is red), and adopting the Mint proxy for Glass→Powder credentials.",
      evidence: [["canary-100","#0"],["bastion-945","#0"],["mint-906","#0"]] },
  ],
  decisions: [
    "Bridge stays dead; Glass is the single operator surface (ratified in VISION.md).",
    "Reports are ad-hoc synthesis, cached not curated (operator ruling, lab-001 round 1).",
    "Clips fold into the wire as an event kind (glass-942).",
  ],
  byRepo: [["glass", 14], ["canary", 9], ["linejam", 7], ["powder", 6], ["cairn", 5], ["bastion", 5], ["roster", 4], ["other", 8]],
  agentHighlights: [
    { agent: "glass-933-codex", line: "shipped the reports engine after surviving a disk-full crash" },
    { agent: "cerberus", line: "verified all five glass merges on merged main" },
    { agent: "canary-lane", line: "blocked 42m awaiting the ingest key" },
  ],
};
