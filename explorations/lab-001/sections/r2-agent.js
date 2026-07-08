// R2 AGENT-7..11 — the per-agent page is NOT a bespoke surface. It is the
// SAME three consolidated primitives, scoped to one agent:
//   (a) a badge STATE HEADER (who · state glyph · card · age),
//   (b) THE WIRE scoped to this agent (same .ae-list-row component),
//   (c) the REPORT primitive scoped to this agent (ask & render for its window).
// These five options diverge on how the three scoped primitives COMPOSE.
// Round-1 survivor AGENT-6 (agent page = filtered wire) presaged this; it is
// not re-implemented here. All content is DATA (glass-933-codex — a real day,
// including the disk-full crash and its recovery).
(function () {
  const P = parts, esc = P.esc;
  const D = DATA.agentDetail;
  const SY = DATA.synthesis;

  // this agent's live age comes from its wall card
  const AGE = (DATA.agents.find((a) => a.name === D.name) || {}).age || "12s";
  const stateGlyph = P.stateIcon(D.state);

  // derived, truthful metrics from the trail (16:45 start → 18:31 latest;
  // blocked 17:05 → recovered 17:26)
  const M = { beats: String(D.trail.length), cards: "1", blocked: "21m", onStage: "1h46m" };

  const KIND_CAT = { shipped: 1, report: 4, note: 5, blocked: 0, question: 2, receipt: 6, release: 3, milestone: 3 };

  const crumbs = `<nav class="ae-crumbs" aria-label="Breadcrumb"><ol><li><a href="#0">Now</a></li><li><a href="#0">Agents</a></li><li>${esc(D.name)}</li></ol></nav>`;

  // ── (a) the scoped STATE HEADER — icons/badges only, no prose run ─────
  function stateHead() {
    return `<div class="r2a-head">
      <span class="ae-status">${stateGlyph}<span class="ae-status-label ae-item">${esc(D.name)}</span></span>
      <span class="ae-tag">powder ${esc(D.card)}</span>
      <span class="ae-tag">${esc(D.state)}</span>
      <span class="ae-tag">${esc(AGE)} ago</span>
    </div>`;
  }

  // ── (b) THE WIRE, scoped — the shipped .ae-list-row, this agent only ──
  function scopedWire(limit) {
    const src = limit ? D.trail.slice(0, limit) : D.trail;
    const rows = src.map((e) => P.wireRow({
      t: e.t, agent: D.name, kind: e.kind, cat: KIND_CAT[e.kind] ?? 5, title: e.body,
    })).join("");
    return `<div class="ae-list-rows">${rows}</div>`;
  }

  // ── (c) the REPORT primitive, scoped to this agent's window ──────────
  // A synthesis, not a table: the glass-933-codex slice of the fleet report
  // (its agentHighlight + the crash-recovery theme — truthful for this lane).
  const HL = SY.agentHighlights.find((h) => h.agent === D.name) || SY.agentHighlights[0];
  const THEME = SY.themes[1]; // "One incident, recovered clean"
  function scopedReport(opts) {
    opts = opts || {};
    const band = P.statBand([
      [M.beats, "beats today"],
      [M.cards, "cards"],
      [M.blocked, "blocked", true],
      [M.onStage, "on stage"],
    ]);
    const ev = THEME.evidence.map(([label]) => `<span class="ae-tag">${esc(label)}</span>`).join("");
    return `<article class="ae-doc">
      <p class="ae-plate-cap">SCOPED REPORT &middot; ${esc(D.name)} &middot; PAST 24H</p>
      <p class="ae-lede">${esc(D.name)} ${esc(HL.line)}.</p>
      ${opts.compact ? "" : `<div class="r2a-sec">${band}</div>`}
      <section class="ae-findings r2a-sec">
        <p class="ae-findings-title">${esc(THEME.title).toUpperCase()}</p>
        <p>${esc(THEME.body)}</p>
        <p class="ae-rec">Work preserved through the crash; the recovery lane rebased over two merged siblings and shipped PR #18 &mdash; no lost commits.</p>
        <div class="rep-chips r2a-sec">${ev}</div>
      </section>
    </article>`;
  }

  // the report ask-bar (sentence form, scoped to this agent)
  function reportBar() {
    return `<section class="rep-gen r2a-sec">
      <p class="ae-plate-cap">REPORT</p>
      <p>Report on <span class="ae-item">${esc(D.name)}</span> over <button type="button" class="rep-slot">the past day</button> <span class="ae-tag">${esc(SY.query.resolved)}</span></p>
      <span><button type="button" class="ae-button ae-button-compact">Run</button></span>
    </section>`;
  }

  // Wire | Report tab strip
  const tabs = (active) => `<div class="ae-tabs r2a-sec" role="tablist" aria-label="Agent views">
      <button type="button" role="tab" aria-selected="${active === "wire"}">Wire &middot; ${D.trail.length}</button>
      <button type="button" role="tab" aria-selected="${active === "report"}">Report</button>
    </div>`;

  Object.assign(window.SPECS, {

    "AGENT-7": {
      label: "Stacked · header→wire→ask",
      thesis: "The three scoped primitives stack in reading order: state header, then the full scoped wire, then a one-line report ask-bar pinned at the bottom.",
      build() {
        const body = `${crumbs}${stateHead()}
          <section class="r2a-sec" aria-label="The wire, scoped">
            <p class="ae-h">THE WIRE &middot; SCOPED</p>
            ${scopedWire()}
          </section>
          ${reportBar()}`;
        return P.shell("now", body);
      },
    },

    "AGENT-8": {
      label: "Header + tabs (Wire)",
      thesis: "One header, two tabs: Wire and Report share the same shell as .ae-tabs; the Wire tab is active and the scoped wire fills the pane.",
      build() {
        const body = `${crumbs}${stateHead()}
          ${tabs("wire")}
          <section class="r2a-sec" role="tabpanel" aria-label="Wire">
            ${scopedWire()}
          </section>`;
        return P.shell("now", body);
      },
    },

    "AGENT-9": {
      label: "Header + tabs (Report)",
      thesis: "The identical tabbed shell as AGENT-8 with the Report tab active — the scoped synthesis renders in place (this agent's highlight + the crash-recovery story), no separate report page.",
      build() {
        const body = `${crumbs}${stateHead()}
          ${tabs("report")}
          <section class="r2a-sec" role="tabpanel" aria-label="Report">
            ${scopedReport()}
          </section>`;
        return P.shell("now", body);
      },
    },

    "AGENT-10": {
      label: "Side-by-side",
      thesis: "The state header spans the top while the two scoped primitives sit abreast: the wire on the left, the compact synthesis report on the right — both visible at once.",
      build() {
        const body = `${crumbs}${stateHead()}
          <div class="r2a-sec agent-split">
            <section aria-label="The wire, scoped">
              <p class="ae-h">THE WIRE &middot; SCOPED</p>
              ${scopedWire()}
            </section>
            <section aria-label="Report, scoped">
              <p class="ae-h">REPORT &middot; PAST DAY</p>
              ${scopedReport({ compact: true })}
            </section>
          </div>`;
        return P.shell("now", body);
      },
    },

    "AGENT-11": {
      label: "Overlay (inversion)",
      thesis: "Inversion: there is no agent page — an .ae-panel drawer opens over a veiled NOW column with the state header, the last 5 scoped wire events, and a link to the full report; you drill down without leaving Now.",
      build() {
        const band = P.statBand([
          [DATA.stats.live, "agents live"],
          [DATA.stats.needYou, "need you", true],
          [DATA.stats.postsToday, "posts today"],
          [DATA.stats.sessionsToday, "sessions"],
          [DATA.stats.freshness, "since last event"],
        ]);
        const wall = DATA.agents.map(P.wallCard).join("");
        const overlay = `<aside class="r2a-overlay ae-panel" aria-label="${esc(D.name)}">
            <div class="r2a-panel-head">
              <span class="ae-status">${stateGlyph}<span class="ae-status-label ae-item">${esc(D.name)}</span></span>
              <button type="button" class="ae-button-quiet ae-button-compact">Close</button>
            </div>
            <div class="r2a-head">
              <span class="ae-tag">powder ${esc(D.card)}</span>
              <span class="ae-tag">${esc(D.state)}</span>
              <span class="ae-tag">${esc(AGE)} ago</span>
            </div>
            <p class="ae-h r2a-sec">LAST 5 EVENTS</p>
            ${scopedWire(5)}
            <p class="r2a-sec"><a href="#0" class="ae-item">Full report on ${esc(D.name)} &rarr;</a></p>
          </aside>`;
        const stage = `<div class="r2a-stage">
            ${band}
            <section class="r2a-sec" aria-label="Fleet wall">
              <p class="ae-h">ON STAGE</p>
              <div class="ae-wall">${wall}</div>
            </section>
            <div class="r2a-scrim"></div>
            ${overlay}
          </div>`;
        return P.shell("now", stage);
      },
    },

  });
})();
