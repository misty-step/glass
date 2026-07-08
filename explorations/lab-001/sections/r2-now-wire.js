// ROUND 2 — NOW-7..11 and WIRE-7..10.
// Honors r2 verdicts: single column, ACTIVE vs NOT (never a 4-state kanban),
// strong filter/sort affordances, redesigned rows/cards where state reads on
// the glyph (not as prose), aligned centered glyphs, and an icon/badge-forward
// answer to the plain stat band. The wire is ONE scope-parametric component;
// blocked + questions ALWAYS pin top-of-line. Both viewports clean.
window.SPECS = window.SPECS || {};
(function () {
  const { esc, icon, stateIcon, shell, wireRow } = parts;

  const AGENTS = DATA.agents;
  const publishing = AGENTS.filter((a) => a.state === "publishing");
  const blocked = AGENTS.filter((a) => a.state === "blocked");
  const quiet = AGENTS.filter((a) => a.state === "quiet");
  // ACTIVE vs NOT — the only split. Blocked rises to the top of ACTIVE.
  const active = [...blocked, ...publishing];

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

  const S = DATA.stats; // live:6 quiet:8 needYou:3 postsToday:23 sessionsToday:7 freshness:"12s"

  // ── shared row anatomies ─────────────────────────────────────────────
  // glyph-led: state glyph | name+badges / act | age
  const rowGlyph = (a) => `<a class="r2n-row" href="#0">
    <span class="r2n-glyph">${stateIcon(a.state)}</span>
    <span class="r2n-main">
      <span class="r2n-line"><span class="ae-item">${esc(a.name)}</span><span class="ae-tag">${esc(stateWord(a.state))}</span><span class="ae-tag">${esc(a.card)}</span></span>
      <span class="r2n-meta">${esc(a.act)}</span>
    </span>
    <span class="r2n-age ae-num">${esc(a.age)}</span>
  </a>`;

  // age-led: loud mono age | glyph | name+badges / act
  const rowAge = (a) => `<a class="r2n-row r2n-age-lead" href="#0">
    <span class="r2n-age ae-num r2n-loud">${esc(a.age)}</span>
    <span class="r2n-glyph">${stateIcon(a.state)}</span>
    <span class="r2n-main">
      <span class="r2n-line"><span class="ae-item">${esc(a.name)}</span><span class="ae-tag">${esc(a.card)}</span></span>
      <span class="r2n-meta">${esc(a.act)}</span>
    </span>
  </a>`;

  // name-led: name+badges / act·age | trailing glyph+state-word
  const rowName = (a) => `<a class="r2n-row r2n-name-lead${a.state === "quiet" ? " mk-quiet" : ""}" href="#0">
    <span class="r2n-main">
      <span class="r2n-line"><span class="ae-item">${esc(a.name)}</span><span class="ae-tag">${esc(a.card)}</span></span>
      <span class="r2n-meta">${esc(a.act)} · ${esc(a.age)}</span>
    </span>
    <span class="r2n-trail"><span class="r2n-glyph">${stateIcon(a.state)}</span><span class="r2n-state">${esc(stateWord(a.state))}</span></span>
  </a>`;

  // redesigned card: state on the glyph + word, labelled badges in a grid
  const cardAgent = (a) => `<a class="r2n-card${a.state === "quiet" ? " mk-quiet" : ""}" href="#0">
    <span class="r2n-card-head">
      <span class="r2n-glyph">${stateIcon(a.state)}</span>
      <span class="ae-item">${esc(a.name)}</span>
      <span class="r2n-card-state">${esc(stateWord(a.state))}</span>
    </span>
    <span class="r2n-meta">${esc(a.act)}</span>
    <span class="r2n-badges"><span class="ae-tag">${esc(a.card)}</span><span class="ae-tag">${esc(repoOf(a))}</span><span class="r2n-age ae-num">${esc(a.age)}</span></span>
  </a>`;

  // ── shared summary-number widgets (the "better than a stat band" answers) ──
  const digestFig = (v, l, warn) =>
    `<span class="r2n-fig">${warn ? icon("warn") : ""}<span class="ae-num ae-item">${esc(v)}</span><span class="r2n-fig-l">${esc(l)}</span></span>`;
  const heroFig = (v, l, warn) =>
    `<span class="r2n-hero-fig"><span class="r2n-hero-v">${warn ? icon("warn") : ""}${esc(v)}</span><span class="r2n-fig-l">${esc(l)}</span></span>`;
  const badge = (glyph, v, l) =>
    `<span class="ae-stat-badge">${glyph}<span class="ae-stat-value">${esc(v)}</span><span class="ae-stat-label">${esc(l)}</span></span>`;

  const chip = (label, n, on) =>
    `<a class="r2n-chip${on ? " is-on" : ""}" href="#0">${esc(label)}${n != null ? ` <span class="r2n-n ae-num">${esc(n)}</span>` : ""}</a>`;

  // ════════════════════════ NOW ════════════════════════════════════════

  // NOW-7 — two self-labelled groups, chip-filter bar, glyph-led rows.
  // Numbers live in the group headings ("WORKING NOW · 6").
  const now7 = () => {
    const desk = `
      <div class="r2n-chips r2n-bar" role="group" aria-label="Filter">
        ${chip("All", AGENTS.length, true)}${chip("Live", publishing.length)}${chip("Blocked", blocked.length)}${chip("Idle", quiet.length)}
      </div>
      <section class="now-sec" aria-label="Working now">
        <p class="ae-h">WORKING NOW · ${active.length}<span class="r2n-sub">${publishing.length} live · ${blocked.length} blocked</span></p>
        <div class="r2n-list">${active.map(rowGlyph).join("")}</div>
      </section>
      <section class="now-sec" aria-label="Idle — claimed, quiet">
        <p class="ae-h">IDLE — CLAIMED, QUIET · ${quiet.length}<span class="r2n-sub">no posts yet</span></p>
        <div class="r2n-list">${quiet.map(rowGlyph).join("")}</div>
      </section>`;
    return shell("now", desk);
  };

  // NOW-8 — one list, .ae-tabs sort, age-led rows, idle folded away.
  // Numbers ride a right-aligned digest line.
  const now8 = () => {
    const byAttn = [...blocked, ...publishing.slice().sort((a, b) => ageSeconds(a.age) - ageSeconds(b.age))];
    const desk = `
      <div class="r2n-digest">
        ${digestFig(S.live, "working")}${digestFig(S.needYou, "need you", true)}${digestFig(S.postsToday, "posts today")}${digestFig(S.sessionsToday, "sessions")}${digestFig(S.freshness, "fresh")}
      </div>
      <div class="ae-tabs r2n-bar" role="tablist" aria-label="Sort">
        <a href="#0" class="is-active" aria-selected="true">Attention</a>
        <a href="#0">Recent</a>
        <a href="#0">Name</a>
      </div>
      <section aria-label="Working now">
        <p class="ae-h">WORKING NOW · ${active.length}</p>
        <div class="r2n-list">${byAttn.map(rowAge).join("")}</div>
      </section>
      <details class="ae-fold now-sec"><summary><span>IDLE — CLAIMED, QUIET</span><span class="ae-dim ae-num">${quiet.length}</span></summary>
        <div class="r2n-list">${quiet.map(rowAge).join("")}</div>
      </details>`;
    return shell("now", desk);
  };

  // NOW-9 — INVERSION of NOW-7's hard grouping: one uninterrupted column, no
  // dividers; attention is an emphasis gradient (active in full ink up top,
  // claimed-quiet fading below). .ae-settings sort row, icon badge strip,
  // name-led rows with a trailing state glyph.
  const now9 = () => {
    const quietStale = quiet.slice().sort((a, b) => ageSeconds(b.age) - ageSeconds(a.age));
    const rows = [...blocked, ...publishing, ...quietStale];
    const desk = `
      <div class="ae-stat-badges r2n-bar">
        ${badge(icon("ok"), S.live, "working")}${badge(icon("warn"), S.needYou, "need you")}${badge(icon("dash"), S.quiet, "idle")}${badge(icon("dash"), S.postsToday, "posts today")}${badge(icon("dash"), S.freshness, "fresh")}
      </div>
      <div class="ae-settings r2n-bar">
        <button class="ae-setting" type="button">Sort<span class="ae-setting-val">Attention — blocked first, quiet last</span></button>
      </div>
      <section aria-label="The fleet, most-urgent first">
        <p class="ae-h">THE FLEET — MOST URGENT FIRST · ${AGENTS.length}</p>
        <div class="r2n-list">${rows.map(rowName).join("")}</div>
      </section>`;
    return shell("now", desk);
  };

  // NOW-10 — keep the card WALL but redesign the card so state reads at a
  // glance: glyph + state word in the head, labelled card/repo/age badges.
  // Two labelled walls; a hero-figure masthead replaces the stat band.
  const now10 = () => {
    const desk = `
      <div class="r2n-hero">
        ${heroFig(S.live, "working now")}${heroFig(S.needYou, "need you", true)}${heroFig(S.quiet, "idle")}${heroFig(S.postsToday, "posts today")}${heroFig(S.freshness, "fresh")}
      </div>
      <section class="now-sec" aria-label="Working now">
        <p class="ae-h">WORKING NOW · ${active.length}</p>
        <div class="r2n-wall">${active.map(cardAgent).join("")}</div>
      </section>
      <section class="now-sec" aria-label="Idle — claimed, quiet">
        <p class="ae-h">IDLE — CLAIMED, QUIET · ${quiet.length}</p>
        <div class="r2n-wall">${quiet.map(cardAgent).join("")}</div>
      </section>`;
    return shell("now", desk);
  };

  // NOW-11 — cohabitation: the ACTIVE roster column with a slim LATEST ON THE
  // WIRE strip docked beneath it (3 newest events + a link to the full wire).
  // Idle folds away; a needs-you-forward badge trio carries the numbers.
  const now11 = () => {
    const peekRow = (e) => `<a class="ae-icon-row" href="#0">
      <span class="ae-list-icon"><span class="ae-tag ae-tag-bare ae-num">${esc(e.t.slice(0, 5))}</span></span>
      <span class="ae-icon-row-main">
        <span>${esc(e.title)}</span>
        <span class="ae-icon-row-meta"><span class="ae-chip ae-cat-${e.cat}">${esc(e.kind)}</span> ${esc(e.agent)}</span>
      </span>
    </a>`;
    const desk = `
      <div class="ae-stat-badges r2n-bar">
        ${badge(icon("warn"), S.needYou, "need you")}${badge(icon("ok"), S.live, "working")}${badge(icon("dash"), S.quiet, "idle")}
      </div>
      <section aria-label="Working now">
        <p class="ae-h">WORKING NOW · ${active.length}</p>
        <div class="r2n-list">${active.map(rowGlyph).join("")}</div>
      </section>
      <details class="ae-fold now-sec"><summary><span>IDLE — CLAIMED, QUIET</span><span class="ae-dim ae-num">${quiet.length}</span></summary>
        <div class="r2n-list">${quiet.map(rowGlyph).join("")}</div>
      </details>
      <div class="r2n-peek">
        <div class="r2n-peek-head"><p class="ae-h" style="margin:0">LATEST ON THE WIRE</p><a class="ae-chrome" href="#0">Open the wire &rarr;</a></div>
        <div class="lab-rows">${DATA.wire.slice(0, 3).map(peekRow).join("")}</div>
      </div>`;
    return shell("now", desk);
  };

  // ════════════════════════ WIRE ════════════════════════════════════════
  // The wire is one component. Blocked + questions pin top-of-line, sourced
  // from DATA.asks (richer: they carry what they block). The chronological
  // body excludes those kinds so nothing double-reports.
  const askKind = (a) => (a.card.startsWith("canary") ? "blocked" : "question");
  const askCat = (a) => (askKind(a) === "blocked" ? 0 : 2);
  const tape = DATA.wire.filter((e) => e.kind !== "blocked" && e.kind !== "question");

  const pinRow = (a) => `<a class="ae-icon-row" href="#0">
    <span class="ae-list-icon">${icon("warn")}</span>
    <span class="ae-icon-row-main">
      <span class="ae-item">${esc(a.title)}</span>
      <span class="ae-icon-row-meta"><span class="ae-chip ae-cat-${askCat(a)}">${esc(askKind(a))}</span> ${esc(a.agent)} · ${esc(a.card)} · ${esc(a.age)} · blocks ${esc(a.blocks)}</span>
    </span>
  </a>`;

  const pinnedBand = () => `<div class="ae-findings">
    <p class="ae-findings-title">NEEDS A DECISION — PINNED · ${DATA.asks.length}</p>
    <div class="lab-rows">${DATA.asks.map(pinRow).join("")}</div>
  </div>`;

  const tapeTable = () => `<div class="ae-plate">
    <p class="ae-plate-cap">THE WIRE · TAPE · ${tape.length} EVENTS</p>
    <table class="ae-table">
      <thead><tr><th class="num">time</th><th>agent</th><th>kind</th><th>event</th></tr></thead>
      <tbody>${tape.map((e) => `<tr>
        <td class="num" data-label="time">${esc(e.t)}</td>
        <td data-label="agent" class="ae-item">${esc(e.agent)}</td>
        <td data-label="kind"><span class="ae-chip ae-cat-${e.cat}">${esc(e.kind)}</span></td>
        <td data-label="event">${esc(e.title)}</td>
      </tr>`).join("")}</tbody>
    </table>
  </div>`;

  // WIRE-7 — pinned NEEDS A DECISION band above the chronological tape.
  const wire7 = () => shell("now",
    `<section aria-label="The wire">
      <p class="ae-h">THE WIRE · FLEET · TODAY</p>
      ${pinnedBand()}
      ${tapeTable()}
    </section>`);

  // WIRE-8 — the operator's filtering instinct on the wire: scope chips
  // (Fleet / per-agent) and categorical kind-filter chips over the tape.
  const wire8 = () => {
    const kinds = [...new Set(tape.map((e) => e.kind))];
    const kindChip = (k, on) => {
      const cat = tape.find((e) => e.kind === k).cat;
      return `<a class="ae-chip ae-cat-${cat}${on ? "" : " r2w-chipoff"}" href="#0">${esc(k)}</a>`;
    };
    const desk = `<section aria-label="The wire">
      <p class="ae-h">THE WIRE</p>
      <div class="r2n-chips r2n-bar" role="group" aria-label="Scope">
        ${chip("Fleet", null, true)}${chip("glass-933-codex")}${chip("cerberus")}${chip("bastion-deploy")}${chip("canary-lane")}
      </div>
      <div class="r2w-legend r2n-bar" role="group" aria-label="Kinds">
        <span class="r2n-fig-l">kinds</span>
        <a class="r2n-chip is-on" href="#0">All</a>
        ${kinds.map((k) => kindChip(k, true)).join("")}
      </div>
      ${pinnedBand()}
      ${tapeTable()}
    </section>`;
    return shell("now", desk);
  };

  // WIRE-9 — HYBRID: keep the pinned band but trade tape density for the
  // round-1 decomposed row anatomy, so every event stays legible.
  const wire9 = () => shell("now",
    `<section aria-label="The wire">
      <p class="ae-h">THE WIRE · FLEET · TODAY</p>
      ${pinnedBand()}
      <p class="ae-h now-sec">CHRONICLE · ${tape.length}</p>
      <div class="ae-list-rows">${tape.map(wireRow).join("")}</div>
    </section>`);

  // WIRE-10 — maximum density, minimum ink: each kind is a Lucide glyph, not
  // a word-chip, decoded by a one-line legend. Blocked + questions still pin.
  const wire10 = () => {
    const KIND_SVG = {
      shipped: '<path d="M20 6 9 17l-5-5"></path>',
      report: '<path d="M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z"></path><path d="M14 2v5h5"></path><path d="M8 13h8"></path><path d="M8 17h5"></path>',
      receipt: '<circle cx="12" cy="12" r="10"></circle><path d="m9 12 2 2 4-4"></path>',
      release: '<path d="M12.586 2.586A2 2 0 0 0 11.172 2H4a2 2 0 0 0-2 2v7.172a2 2 0 0 0 .586 1.414l8.704 8.704a2.426 2.426 0 0 0 3.42 0l6.58-6.58a2.426 2.426 0 0 0 0-3.42Z"></path><path d="M7.5 7.5h.01"></path>',
      note: '<path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z"></path>',
    };
    const kindGlyph = (k) => `<svg class="ae-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${KIND_SVG[k] || KIND_SVG.note}</svg>`;
    const legendKinds = [...new Set(tape.map((e) => e.kind))];
    const legend = legendKinds.map((k) => `<span class="r2w-leg">${kindGlyph(k)}<span class="r2n-fig-l">${esc(k)}</span></span>`).join("");
    const desk = `<section aria-label="The wire">
      <p class="ae-h">THE WIRE · TAPE</p>
      <div class="r2w-legend r2n-bar" aria-label="Legend"><span class="r2n-fig-l">legend</span>${legend}</div>
      ${pinnedBand()}
      <div class="ae-plate">
        <p class="ae-plate-cap">${tape.length} EVENTS · GLYPH = KIND</p>
        <table class="ae-table">
          <thead><tr><th>kind</th><th class="num">time</th><th>agent</th><th>event</th></tr></thead>
          <tbody>${tape.map((e) => `<tr>
            <td data-label="kind">${kindGlyph(e.kind)}</td>
            <td class="num" data-label="time">${esc(e.t)}</td>
            <td data-label="agent" class="ae-item">${esc(e.agent)}</td>
            <td data-label="event">${esc(e.title)}</td>
          </tr>`).join("")}</tbody>
        </table>
      </div>
    </section>`;
    return shell("now", desk);
  };

  Object.assign(window.SPECS, {
    "NOW-7": { label: "Grouped roster + chip filters", thesis: "Two self-labelled groups (WORKING NOW / IDLE) with counts in the headings, a state chip-filter bar, glyph-led rows where the state hue rides the icon.", build: now7 },
    "NOW-8": { label: "Attention ledger, tab-sorted", thesis: "One list sorted by an .ae-tabs switch, each row led by a loud mono age, idle collapsed into a fold; numbers ride a right-aligned digest line.", build: now8 },
    "NOW-9": { label: "Emphasis-gradient column", thesis: "Inversion of hard grouping — one uninterrupted column with no dividers; attention is an ink gradient (active bright, quiet fading), .ae-settings sort, name-led rows with a trailing state glyph.", build: now9 },
    "NOW-10": { label: "Redesigned card wall", thesis: "Keep the card wall but rebuild the card so state reads at a glance (glyph + state word + labelled card/repo/age badges); two walls, a hero-figure masthead for the numbers.", build: now10 },
    "NOW-11": { label: "Roster + wire preview", thesis: "Cohabitation — the ACTIVE roster with a slim LATEST ON THE WIRE strip docked beneath (3 newest + link to full wire); idle folds, a needs-you-forward badge trio carries the numbers.", build: now11 },

    "WIRE-7": { label: "Pinned band + tape", thesis: "The needs-a-decision items (blocked + questions, from asks) pin to a warn-glyph band; the rest of the day runs below as the dense timestamp tape.", build: wire7 },
    "WIRE-8": { label: "Filterable tape", thesis: "The operator's filtering instinct on the wire — scope chips (Fleet / per-agent) and categorical kind-filter chips over the tape, attention still pinned.", build: wire8 },
    "WIRE-9": { label: "Pinned band + decomposed rows", thesis: "Hybrid — keep the pinned attention band but trade tape density for the round-1 decomposed row anatomy so every event stays legible.", build: wire9 },
    "WIRE-10": { label: "Icon tape + legend", thesis: "Maximum density, minimum ink — each kind becomes a Lucide glyph instead of a word-chip, decoded by a one-line legend; blocked + questions still pin.", build: wire10 },
  });
})();
