// NOW — the fleet wall composition, and WIRE — the ambient feed.
// Six structurally distinct options each. Baseline first (PREFIX-1),
// one explicit inversion per section, real DATA at truthful density.
window.SPECS = window.SPECS || {};
(function () {
  const { esc, icon, stateIcon, shell, statBand, wallCard, wireRow } = parts;

  const AGENTS = DATA.agents;
  const active = AGENTS.filter((a) => a.state !== "quiet");
  const publishing = AGENTS.filter((a) => a.state === "publishing");
  const blocked = AGENTS.filter((a) => a.state === "blocked");
  const quiet = AGENTS.filter((a) => a.state === "quiet");

  // ── shared derivations ──────────────────────────────────────────────
  const ageSeconds = (age) => {
    const m = String(age).match(/^(\d+)\s*([smhd])/);
    if (!m) return 0;
    return Number(m[1]) * { s: 1, m: 60, h: 3600, d: 86400 }[m[2]];
  };
  const stateWord = (s) => (s === "publishing" ? "live" : s === "blocked" ? "blocked" : "quiet");
  const repoOverride = { cerberus: "glass", "bastion-deploy": "sanctum", "team-lead": "glass" };
  const repoOf = (a) => {
    if (repoOverride[a.name]) return repoOverride[a.name];
    const m = String(a.card).match(/^([a-z][a-z-]*?)-\d+$/);
    return m ? m[1] : "glass";
  };

  const NOW_STATS = [
    [DATA.stats.live, "on stage"],
    [DATA.stats.quiet, "claimed · quiet"],
    [DATA.stats.needYou, "needs you", true],
    [DATA.stats.postsToday, "posts today"],
    [DATA.stats.sessionsToday, "sessions"],
    [DATA.stats.freshness, "fresh"],
  ];

  // a calm two-line row — glyph carries state, prose carries the rest
  const attnRow = (a) => `<a class="ae-icon-row" href="#0">
    <span class="ae-list-icon">${stateIcon(a.state)}</span>
    <span class="ae-icon-row-main">
      <span class="ae-item">${esc(a.name)} <span class="ae-tag">${esc(a.card)}</span></span>
      <span class="ae-icon-row-meta">${esc(a.act)} · ${esc(a.age)}</span>
    </span>
  </a>`;

  const boardCol = (title, meta, body) => `<div class="ae-column">
    <div class="ae-column-head"><span class="ae-column-title">${esc(title)}</span><span class="ae-column-meta">${esc(meta)}</span></div>
    <div class="ae-column-body">${body}</div>
  </div>`;

  const agentBoardCard = (a) => `<div class="ae-board-card"><a class="ae-icon-row" href="#0">
    <span class="ae-list-icon">${stateIcon(a.state)}</span>
    <span class="ae-icon-row-main">
      <span class="ae-item">${esc(a.name)}</span>
      <span class="ae-icon-row-meta">${esc(a.act)}</span>
      <span class="ae-icon-row-meta"><span class="ae-tag">${esc(a.card)}</span> · ${esc(a.age)}</span>
    </span>
  </a></div>`;

  const doneBoardCard = (c) => `<div class="ae-board-card"><span class="ae-icon-row">
    <span class="ae-list-icon">${icon("ok")}</span>
    <span class="ae-icon-row-main">
      <span class="ae-item">${esc(c.title)}</span>
      <span class="ae-icon-row-meta"><span class="ae-tag">${esc(c.card)}</span> · ${esc(c.repo)} · ${esc(c.at)}</span>
    </span>
  </span></div>`;

  // ════════════════════════ NOW ════════════════════════════════════════

  // NOW-1 — shipped: stat band, ON STAGE wall, idle fold, THE WIRE.
  const now1 = () => {
    const desk = `
      <div class="now-sec">${statBand(NOW_STATS)}</div>
      <section class="now-sec" aria-label="Fleet wall">
        <p class="ae-h">ON STAGE</p>
        <div class="ae-wall">${active.map(wallCard).join("")}</div>
      </section>
      <details class="ae-fold now-sec"><summary><span class="ae-dim">CLAIMED · QUIET</span><span class="ae-dim">${quiet.length} idle</span></summary><div class="ae-wall">${quiet.map(wallCard).join("")}</div></details>
      <section class="now-sec" aria-label="The wire">
        <p class="ae-h">THE WIRE</p>
        <div class="ae-list-rows">${DATA.wire.map(wireRow).join("")}</div>
      </section>`;
    return shell("now", desk);
  };

  // NOW-2 — kanban by state: the state IS the layout.
  const now2 = () => {
    const done = DATA.r001.completions;
    const doneBody = done.slice(0, 5).map(doneBoardCard).join("") +
      `<div class="ae-board-card"><span class="ae-icon-row-meta">…${DATA.r001.totals.completed} completions across ${DATA.finished24h.agents} agents · ${DATA.finished24h.sessions} sessions</span></div>`;
    const desk = `
      <div class="now-sec">${statBand(NOW_STATS)}</div>
      <div class="ae-board">
        ${boardCol("PUBLISHING", String(publishing.length), publishing.map(agentBoardCard).join(""))}
        ${boardCol("BLOCKED", String(blocked.length), blocked.map(agentBoardCard).join(""))}
        ${boardCol("CLAIMED · QUIET", String(quiet.length), quiet.map(agentBoardCard).join(""))}
        ${boardCol("FINISHED · 24H", DATA.r001.totals.completed + " done", doneBody)}
      </div>`;
    return shell("now", desk);
  };

  // NOW-3 — INVERSION: the comfortable card grid collapses into one dense
  // terminal ledger; every agent is a single row, like a departure board.
  const now3 = () => {
    const order = { blocked: 0, publishing: 1, quiet: 2 };
    const rows = AGENTS.slice().sort((a, b) =>
      (order[a.state] - order[b.state]) || (ageSeconds(a.age) - ageSeconds(b.age)));
    const body = rows.map((a) => `<tr>
      <td data-label="state">${stateIcon(a.state)} ${stateWord(a.state)}</td>
      <td data-label="agent" class="ae-item">${esc(a.name)}</td>
      <td data-label="card">${esc(a.card)}</td>
      <td data-label="act">${esc(a.act)}</td>
      <td data-label="age" class="num">${esc(a.age)}</td>
    </tr>`).join("");
    const desk = `<div class="ae-plate">
      <p class="ae-plate-cap">FLEET LEDGER · ${AGENTS.length} AGENTS · FRESH ${esc(DATA.stats.freshness)}</p>
      <table class="ae-table">
        <thead><tr><th>state</th><th>agent</th><th>card</th><th>act</th><th class="num">age</th></tr></thead>
        <tbody>${body}</tbody>
      </table>
      <p class="ae-plate-note">${DATA.stats.live} live · ${DATA.stats.quiet} claimed-quiet · ${blocked.length} blocked awaiting you.</p>
    </div>`;
    return shell("now", desk);
  };

  // NOW-4 — substitute the organizing principle: by-agent → by-repo.
  const now4 = () => {
    const groups = {};
    AGENTS.forEach((a) => { (groups[repoOf(a)] = groups[repoOf(a)] || []).push(a); });
    const ordered = Object.entries(groups).sort((a, b) =>
      (b[1].length - a[1].length) || a[0].localeCompare(b[0]));
    const secs = ordered.map(([repo, list]) => `<section class="now-sec">
      <p class="ae-h">${esc(repo.toUpperCase())} · ${list.length}</p>
      <div class="ae-wall">${list.map(wallCard).join("")}</div>
    </section>`).join("");
    const desk = `<div class="now-sec">${statBand(NOW_STATS)}</div>${secs}`;
    return shell("now", desk);
  };

  // NOW-5 — INVERSION: reverse the hierarchy from newest-flat to triage.
  // Blocked rises, quiet-stale next, humming publishing sinks. Stat band
  // becomes a single status sentence. Calm density.
  const now5 = () => {
    const staleQuiet = quiet.slice().sort((a, b) => ageSeconds(b.age) - ageSeconds(a.age));
    const rows = [...blocked, ...staleQuiet, ...publishing];
    const desk = `
      <div class="ae-status now-sec">${icon("warn")}<span class="ae-status-label"><span class="ae-strong">${blocked.length}</span> blocked · <span class="ae-strong">${quiet.length}</span> quiet · <span class="ae-strong">${publishing.length}</span> publishing · fleet fresh ${esc(DATA.stats.freshness)}</span></div>
      <p class="ae-h now-sec" style="margin-bottom:0.7em">BY WHAT NEEDS YOU</p>
      <div class="lab-rows now5-col">${rows.map(attnRow).join("")}</div>`;
    return shell("now", desk);
  };

  // NOW-6 — combine strata: wall and wire side by side, the feed a ticker rail.
  const now6 = () => {
    const tick = (e) => `<a class="ae-icon-row" href="#0">
      <span class="ae-list-icon"><span class="ae-tag ae-tag-bare">${esc(e.t.slice(0, 5))}</span></span>
      <span class="ae-icon-row-main">
        <span>${esc(e.title)}</span>
        <span class="ae-icon-row-meta"><span class="ae-chip ae-cat-${e.cat}">${esc(e.kind)}</span> ${esc(e.agent)}</span>
      </span>
    </a>`;
    const desk = `
      <div class="now-sec">${statBand(NOW_STATS)}</div>
      <div class="now6-split">
        <section aria-label="Fleet wall">
          <p class="ae-h">ON STAGE</p>
          <div class="ae-wall">${active.map(wallCard).join("")}</div>
          <p class="ae-dim" style="margin-top:1.4em">+ ${quiet.length} more claimed &amp; quiet</p>
        </section>
        <aside class="now6-rail" aria-label="The wire">
          <p class="ae-h">THE WIRE</p>
          <div class="lab-rows">${DATA.wire.map(tick).join("")}</div>
        </aside>
      </div>`;
    return shell("now", desk);
  };

  // ════════════════════════ WIRE ════════════════════════════════════════

  // WIRE-1 — shipped: decomposed list rows, chronological.
  const wire1 = () => shell("now",
    `<section aria-label="The wire"><p class="ae-h">THE WIRE</p><div class="ae-list-rows">${DATA.wire.map(wireRow).join("")}</div></section>`);

  // WIRE-2 — the trail spine: time + actor ticks, prose bodies; the
  // conversation the fleet is having with itself.
  const wire2 = () => {
    const item = (e, i) => `<li class="ae-trail-item${i === 0 ? " is-active" : ""}">
      <div class="ae-trail-head"><span class="ae-trail-time">${esc(e.t)}</span><span class="ae-trail-who">${esc(e.agent)}</span><span class="ae-chip ae-cat-${e.cat}">${esc(e.kind)}</span></div>
      <div class="ae-trail-body">${esc(e.title)}</div>
    </li>`;
    return shell("now", `<section aria-label="The wire"><p class="ae-h">THE WIRE · TODAY</p><ul class="ae-trail">${DATA.wire.map(item).join("")}</ul></section>`);
  };

  // WIRE-3 — day-grouped digest: hour headers, quiet icon-rows. Calm.
  const wire3 = () => {
    const kindIcon = (e) => (e.kind === "blocked" || e.kind === "question") ? icon("warn") : icon("dash");
    const digestRow = (e) => `<a class="ae-icon-row" href="#0">
      <span class="ae-list-icon">${kindIcon(e)}</span>
      <span class="ae-icon-row-main">
        <span>${esc(e.title)}</span>
        <span class="ae-icon-row-meta">${esc(e.t)} · ${esc(e.agent)} · <span class="ae-chip ae-cat-${e.cat}">${esc(e.kind)}</span></span>
      </span>
    </a>`;
    const groups = {};
    DATA.wire.forEach((e) => { const hh = e.t.slice(0, 2); (groups[hh] = groups[hh] || []).push(e); });
    const secs = Object.entries(groups)
      .sort((a, b) => b[0].localeCompare(a[0]))
      .map(([hh, list]) => `<p class="ae-h">${esc(hh)}:00 · ${list.length}</p><div class="wire3-group">${list.map(digestRow).join("")}</div>`)
      .join("");
    return shell("now", `<section aria-label="The wire"><p class="ae-h" style="letter-spacing:0.14em">THE WIRE · DIGEST</p>${secs}</section>`);
  };

  // WIRE-4 — substitute the axis: time → kind. One lane per feed kind.
  const wire4 = () => {
    const laneCard = (e) => `<div class="ae-board-card"><a class="ae-icon-row" href="#0">
      <span class="ae-list-icon"><span class="ae-tag ae-tag-bare">${esc(e.t.slice(0, 5))}</span></span>
      <span class="ae-icon-row-main"><span>${esc(e.title)}</span><span class="ae-icon-row-meta">${esc(e.agent)}</span></span>
    </a></div>`;
    const order = ["shipped", "report", "receipt", "blocked", "question", "release", "note"];
    const groups = {};
    DATA.wire.forEach((e) => { (groups[e.kind] = groups[e.kind] || []).push(e); });
    const kinds = Object.keys(groups).sort((a, b) => order.indexOf(a) - order.indexOf(b));
    const cols = kinds.map((k) => {
      const list = groups[k];
      const head = `<span class="ae-chip ae-cat-${list[0].cat}">${esc(k)}</span>`;
      return `<div class="ae-column"><div class="ae-column-head"><span class="ae-column-title">${head}</span><span class="ae-column-meta">${list.length}</span></div><div class="ae-column-body">${list.map(laneCard).join("")}</div></div>`;
    }).join("");
    return shell("now", `<section aria-label="The wire"><p class="ae-h">THE WIRE · BY KIND</p><div class="ae-board">${cols}</div></section>`);
  };

  // WIRE-5 — INVERSION: the wire prioritizes, not chronicles. Blocked and
  // questions pin to a findings frame up top; the rest flows below.
  const wire5 = () => {
    const isUrgent = (e) => e.kind === "blocked" || e.kind === "question";
    const urgent = DATA.wire.filter(isUrgent);
    const rest = DATA.wire.filter((e) => !isUrgent(e));
    const urgentRow = (e) => `<a class="ae-icon-row" href="#0">
      <span class="ae-list-icon">${icon("warn")}</span>
      <span class="ae-icon-row-main">
        <span class="ae-item">${esc(e.title)}</span>
        <span class="ae-icon-row-meta">${esc(e.t)} · ${esc(e.agent)} · <span class="ae-chip ae-cat-${e.cat}">${esc(e.kind)}</span></span>
      </span>
    </a>`;
    const desk = `
      <section aria-label="The wire">
        <div class="ae-findings">
          <p class="ae-findings-title">NEEDS A DECISION · ${urgent.length}</p>
          <div class="lab-rows">${urgent.map(urgentRow).join("")}</div>
        </div>
        <p class="ae-h">EVERYTHING ELSE</p>
        <div class="ae-list-rows">${rest.map(wireRow).join("")}</div>
      </section>`;
    return shell("now", desk);
  };

  // WIRE-6 — the tape: an ultra-dense mono table, timestamp-sorted.
  const wire6 = () => {
    const body = DATA.wire.map((e) => `<tr>
      <td class="num" data-label="time">${esc(e.t)}</td>
      <td data-label="agent" class="ae-item">${esc(e.agent)}</td>
      <td data-label="kind"><span class="ae-chip ae-cat-${e.cat}">${esc(e.kind)}</span></td>
      <td data-label="event">${esc(e.title)}</td>
    </tr>`).join("");
    const desk = `<div class="ae-plate">
      <p class="ae-plate-cap">THE WIRE · TAPE · ${DATA.wire.length} EVENTS</p>
      <table class="ae-table">
        <thead><tr><th class="num">time</th><th>agent</th><th>kind</th><th>event</th></tr></thead>
        <tbody>${body}</tbody>
      </table>
    </div>`;
    return shell("now", desk);
  };

  Object.assign(window.SPECS, {
    "NOW-1": { label: "Baseline — shipped wall", thesis: "Stat band, an ON STAGE card wall of active agents, a fold of claimed-quiet agents, the wire below.", build: now1 },
    "NOW-2": { label: "Kanban by state", thesis: "The state is the layout: publishing / blocked / claimed-quiet / finished as columns you scan across.", build: now2 },
    "NOW-3": { label: "Departure-board ledger", thesis: "Inversion — the comfortable card grid collapses into one dense terminal table, every agent a single row.", build: now3 },
    "NOW-4": { label: "Grouped by repo", thesis: "Substitute the organizing principle from by-agent to by-repo: a small wall under each repository header.", build: now4 },
    "NOW-5": { label: "Attention-sorted column", thesis: "Inversion — reverse newest-flat to a triage column: blocked rises, quiet-stale next, humming work sinks; stat band becomes one status sentence.", build: now5 },
    "NOW-6": { label: "Split desk + wire rail", thesis: "Combine strata: the wall holds two-thirds left, the wire runs as a ticker rail on the right.", build: now6 },

    "WIRE-1": { label: "Baseline — shipped feed", thesis: "Decomposed list rows, newest first, each with labelled time / agent / kind / event fields.", build: wire1 },
    "WIRE-2": { label: "The trail spine", thesis: "A hairline spine with time-and-actor ticks and prose bodies — the wire as the conversation the fleet is having.", build: wire2 },
    "WIRE-3": { label: "Day-grouped digest", thesis: "Group by the hour under quiet headers, each event a calm icon-row — the wire read back as a digest, not a stream.", build: wire3 },
    "WIRE-4": { label: "Kind-clustered lanes", thesis: "Substitute the axis from time to kind: one column per feed kind, shipped / report / blocked / question / release / note.", build: wire4 },
    "WIRE-5": { label: "Severity-first inbox", thesis: "Inversion — the wire prioritizes instead of chronicling: blocked and questions pin to a findings frame up top, everything else flows below.", build: wire5 },
    "WIRE-6": { label: "Ticker-plate tape", thesis: "The tape — an ultra-dense mono table at the 13px instrument register, timestamp-sorted.", build: wire6 },
  });
})();
