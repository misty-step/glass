// SHELL-* — the app chrome & places (how the operator moves between
//   Now / Needs you / Reports / Clips, and how much chrome exists).
// AGENT-* — the per-agent drill-down (/agent/:name).
// Every SHELL option renders the SAME Now desk body (nowDeskBody) so chrome
// differences are judged against identical content. All content is DATA.
(function () {
  const P = parts, esc = P.esc, icon = P.icon;
  const S = DATA.stats;

  const logo = `<span class="ae-logo ae-logo-compact"><span class="ae-app-mark">${icon("glass")}</span><span class="ae-name">Glass</span></span>`;
  const themeBtn = `<button class="ae-mode" type="button" aria-label="Theme">${icon("sun")}${icon("moon")}</button>`;

  // ── shared Now desk body (shipped composition, via parts) ───────────
  function nowDeskBody() {
    const band = P.statBand([
      [S.live, "agents live"],
      [S.needYou, "need you", true],
      [S.postsToday, "posts today"],
      [S.sessionsToday, "sessions"],
      [S.freshness, "since last event"],
    ]);
    const wall = DATA.agents.map(P.wallCard).join("");
    const wire = DATA.wire.map(P.wireRow).join("");
    const dead = `<details class="ae-fold shell-sec">
        <summary><span class="ae-dim">FINISHED IN THE LAST 24H</span><span class="ae-dim">${DATA.finished24h.agents} agents &middot; ${DATA.finished24h.sessions} sessions</span></summary>
      </details>`;
    return `${band}
      <section class="shell-sec" aria-label="Fleet wall">
        <p class="ae-h">ON STAGE</p>
        <div class="ae-wall">${wall}</div>
      </section>
      ${dead}
      <section class="shell-sec" aria-label="The wire">
        <p class="ae-h">THE WIRE</p>
        <div class="ae-list-rows">${wire}</div>
      </section>`;
  }

  // ── SHELL options ───────────────────────────────────────────────────
  Object.assign(window.SPECS, {
    "SHELL-1": {
      label: "Baseline — left rail",
      thesis: "Shipped chrome: a persistent 13px vertical rail of places beside the working desk.",
      build() { return P.shell("now", nowDeskBody()); },
    },

    "SHELL-2": {
      label: "Top-strip tabs",
      thesis: "The vertical rail rotates into a horizontal top strip; places become .ae-tabs and the desk claims full width.",
      build() {
        return `<div class="shell-screen">
          <header class="shell-strip">
            ${logo}
            <nav class="ae-tabs" aria-label="Places">
              <a href="#0" aria-current="page">Now</a>
              <a href="#0">Needs you&ensp;&middot; ${S.needYou}</a>
              <a href="#0">Reports</a>
              <a href="#0">Clips</a>
            </nav>
            <span class="shell-strip-foot"><a href="#0">Sanctum</a><a href="#0">Wire an agent</a>${themeBtn}</span>
          </header>
          <div class="shell-scroll">${nowDeskBody()}</div>
        </div>`;
      },
    },

    "SHELL-3": {
      label: "Instrument rail",
      thesis: "The rail is an instrument panel: every place carries its own live figures and a recent-state trace, so navigation doubles as a dashboard.",
      build() {
        const trace = (glyphs) => `<span class="ae-wall-trace">${glyphs}</span>`;
        const wallStates = DATA.agents.slice(0, 6).map((a) => a.state === "blocked" ? icon("warn") : a.state === "quiet" ? icon("dash") : icon("ok")).join("");
        const oldestAsk = DATA.asks[0];
        const rail = `<aside class="ae-rail">
          ${logo}
          <p class="ae-h">PLACES</p>
          <nav class="shell-inst">
            <a class="shell-inst-row" href="#0" aria-current="page">
              <span class="ae-item">Now</span>
              <span class="shell-inst-fig"><span class="ae-num ae-strong">${S.live}</span> live<span class="ae-num">${S.quiet}</span> quiet</span>
              ${trace(wallStates)}
            </a>
            <a class="shell-inst-row" href="#0">
              <span class="ae-item">Needs you</span>
              <span class="shell-inst-fig">${icon("warn")}<span class="ae-num ae-strong">${S.needYou}</span> open</span>
              <span class="shell-inst-fig">oldest ${esc(oldestAsk.age)}</span>
            </a>
            <a class="shell-inst-row" href="#0">
              <span class="ae-item">Reports</span>
              <span class="shell-inst-fig"><span class="ae-num ae-strong">${DATA.reports.length}</span> filed</span>
              <span class="shell-inst-fig">newest ${esc(DATA.reports[1].by)}</span>
            </a>
            <a class="shell-inst-row" href="#0">
              <span class="ae-item">Clips</span>
              <span class="shell-inst-fig"><span class="ae-num ae-strong">${DATA.clips.length}</span> today</span>
            </a>
          </nav>
          <div class="ae-rail-foot">
            <a href="#0">${icon("home")} Sanctum</a>
            <a href="#0">Wire an agent</a>
            ${themeBtn}
          </div>
        </aside>`;
        return `<div class="ae-shell">${rail}<main class="ae-desk">${nowDeskBody()}</main></div>`;
      },
    },

    "SHELL-4": {
      label: "Zero-chrome (inversion)",
      thesis: "Inversion: no rail, no tabs — `/` is the only page. The other places are inline .ae-fold disclosures under the wall, so the whole app is one scroll.",
      build() {
        const askRows = DATA.asks.map((a) =>
          `<div><span class="ae-status">${icon("warn")}<span class="ae-status-label ae-item">${esc(a.title)}</span></span><div class="ae-dim">${esc(a.agent)} &middot; powder ${esc(a.card)} &middot; ${esc(a.age)} &middot; blocks ${esc(a.blocks)}</div></div>`).join("");
        const repRows = DATA.reports.map((r) =>
          `<div><span class="ae-item">${esc(r.title)}</span><div class="ae-dim">${esc(r.id)} &middot; ${esc(r.scope)} &middot; ${esc(r.by)}</div></div>`).join("");
        const clipRows = DATA.clips.map((c) =>
          `<div><span class="ae-item">${esc(c.title)}</span><div class="ae-dim">${esc(c.agent)} &middot; ${esc(c.when)}</div></div>`).join("");
        return `<div class="shell-screen">
          <header class="shell-strip">${logo}<span class="shell-strip-foot">${themeBtn}</span></header>
          <div class="shell-scroll">
            ${nowDeskBody()}
            <div class="shell-sec">
              <details class="ae-fold"><summary>Needs you<span class="ae-dim">${S.needYou} open &rarr;</span></summary><div class="lab-rows">${askRows}</div></details>
              <details class="ae-fold"><summary>Reports<span class="ae-dim">${DATA.reports.length} filed &rarr;</span></summary><div class="lab-rows">${repRows}</div></details>
              <details class="ae-fold"><summary>Clips<span class="ae-dim">${DATA.clips.length} today &rarr;</span></summary><div class="lab-rows">${clipRows}</div></details>
            </div>
          </div>
        </div>`;
      },
    },

    "SHELL-5": {
      label: "Bottom command-bar (inversion)",
      thesis: "Inversion: the mobile bottom-chrome pattern is promoted to every viewport — the desk fills the screen above a persistent command-bar of places carrying counts.",
      build() {
        const cell = (label, fig, cur) =>
          `<a href="#0"${cur ? ' aria-current="page"' : ""}><span class="ae-item">${label}</span>${fig ? `<span class="ae-num ae-dim">${fig}</span>` : ""}</a>`;
        return `<div class="shell-screen">
          <div class="shell-scroll">${nowDeskBody()}</div>
          <nav class="shell-dock" aria-label="Places">
            ${cell("Now", S.live + " live", true)}
            ${cell("Needs you", S.needYou, false)}
            ${cell("Reports", DATA.reports.length, false)}
            ${cell("Clips", DATA.clips.length, false)}
          </nav>
        </div>`;
      },
    },

    "SHELL-6": {
      label: "Two-tier chrome",
      thesis: "Two strata: a thin global strip of places on top, plus a contextual sub-nav band scoped to the active place (Now's own sections).",
      build() {
        return `<div class="shell-screen">
          <header class="shell-strip">
            ${logo}
            <nav class="ae-tabs" aria-label="Places">
              <a href="#0" aria-current="page">Now</a>
              <a href="#0">Needs you&ensp;&middot; ${S.needYou}</a>
              <a href="#0">Reports</a>
              <a href="#0">Clips</a>
            </nav>
            <span class="shell-strip-foot">${themeBtn}</span>
          </header>
          <div class="shell-tier2">
            <nav class="ae-tabs" aria-label="Now sections">
              <a href="#0" aria-current="page">On stage&ensp;&middot; ${S.live}</a>
              <a href="#0">The wire&ensp;&middot; ${DATA.wire.length}</a>
              <a href="#0">Finished 24h&ensp;&middot; ${DATA.finished24h.agents}</a>
            </nav>
          </div>
          <div class="shell-scroll">${nowDeskBody()}</div>
        </div>`;
      },
    },
  });

  // ── AGENT drill-down ────────────────────────────────────────────────
  const D = DATA.agentDetail;
  const KIND_CAT = { shipped: 1, report: 4, note: 5, blocked: 0, milestone: 3, question: 2, receipt: 6, release: 3 };
  // derived, truthful metrics from the trail (16:45 start → 18:31 latest;
  // blocked 17:05 → recovered 17:26)
  const AGENT_METRICS = { cards: "1", beats: String(D.trail.length), blocked: "21m", onStage: "1h46m" };

  const crumbs = `<nav class="ae-crumbs" aria-label="Breadcrumb"><ol><li><a href="#0">Now</a></li><li><a href="#0">Agents</a></li><li>${esc(D.name)}</li></ol></nav>`;
  const stateGlyph = D.state === "blocked" ? icon("warn") : D.state === "quiet" ? icon("dash") : icon("ok");
  const agentHead = `${crumbs}
    <p class="ae-status shell-sec">${stateGlyph}<span class="ae-status-label ae-item">${esc(D.name)}</span><span class="ae-tag">powder ${esc(D.card)}</span></p>
    <p class="ae-dim">${esc(D.stateLine)}</p>`;

  const trailGlyph = (kind) => kind === "blocked" ? icon("warn") : kind === "shipped" ? icon("ok") : "";
  function trailItem(e, active) {
    const g = trailGlyph(e.kind);
    return `<li class="ae-trail-item${active ? " is-active" : ""}">
      <div class="ae-trail-head"><span class="ae-trail-time">${esc(e.t)}</span><span class="ae-trail-who">${esc(e.kind)}</span></div>
      <div class="ae-trail-body">${g ? `<span class="ae-status">${g}</span> ` : ""}${esc(e.body)}</div>
    </li>`;
  }
  const trailList = (items) => `<ol class="ae-trail">${items.map((e, i) => trailItem(e, i === 0)).join("")}</ol>`;

  // the latest beat as a "stage" surface, with the gate readout as a meter
  const gatePlate = `<div class="ae-plate">
      <p class="ae-plate-cap">GATE READOUT &middot; 18:04</p>
      <p><span class="ae-status">${icon("ok")}<span class="ae-status-label">coverage 74.22%</span></span> <span class="ae-dim">floor 74.0</span></p>
      <div class="ae-meter"><span class="ae-meter-fill ae-ok" style="width:74.22%"></span><span class="ae-meter-mark" style="left:74%"></span></div>
      <p class="ae-plate-note">check.sh green &middot; e2e 7/7 &middot; rebased over 934 + 932</p>
    </div>`;
  const latestBeat = D.trail[0];

  Object.assign(window.SPECS, {
    "AGENT-1": {
      label: "Baseline — trail",
      thesis: "Shipped drill-down: breadcrumb, a name + status header, then the whole session as one .ae-trail spine.",
      build() {
        return P.shell("now", `${agentHead}<div class="shell-sec">${trailList(D.trail)}</div>`);
      },
    },

    "AGENT-2": {
      label: "Split — spine + stage",
      thesis: "Two columns: the trail spine holds history on the left while the latest surface (shipped PR + gate readout) stands open on the right.",
      build() {
        const body = `${agentHead}
          <div class="shell-sec agent-split">
            <div>${trailList(D.trail)}</div>
            <div>
              <p class="ae-h">LATEST SURFACE</p>
              <p><span class="ae-trail-who">${esc(latestBeat.kind)}</span> <span class="ae-trail-time">${esc(latestBeat.t)}</span></p>
              <p class="ae-item">${esc(latestBeat.body)}</p>
              <div class="shell-sec">${gatePlate}</div>
            </div>
          </div>`;
        return P.shell("now", body);
      },
    },

    "AGENT-3": {
      label: "Dossier",
      thesis: "Combine strata: a quantified stat band for the agent + a sessions plate on top, with the narrative trail beneath — the personnel-file read.",
      build() {
        const band = P.statBand([
          [AGENT_METRICS.cards, "cards claimed"],
          [AGENT_METRICS.beats, "beats today"],
          [AGENT_METRICS.blocked, "blocked", true],
          [AGENT_METRICS.onStage, "on stage"],
        ]);
        const sessions = `<div class="ae-plate shell-sec">
            <p class="ae-plate-cap">FIG. 1 &middot; SESSIONS TODAY</p>
            <table class="ae-table">
              <thead><tr><th>Session</th><th>Started</th><th class="num">Beats</th><th>State</th></tr></thead>
              <tbody>
                <tr>
                  <td data-label="Session" class="ae-item">${esc(D.session)}</td>
                  <td data-label="Started">16:45</td>
                  <td data-label="Beats" class="num">${AGENT_METRICS.beats}</td>
                  <td data-label="State">${esc(D.state)}</td>
                </tr>
              </tbody>
            </table>
          </div>`;
        return P.shell("now", `${agentHead}<div class="shell-sec">${band}</div>${sessions}<div class="shell-sec">${trailList(D.trail)}</div>`);
      },
    },

    "AGENT-4": {
      label: "Ledger (blotter)",
      thesis: "Import the trader's blotter: the timeline collapses from a prose spine into a dense mono .ae-table ledger — one ruled row per beat.",
      build() {
        const rows = D.trail.map((e) => {
          const g = trailGlyph(e.kind);
          return `<tr>
            <td data-label="Time">${esc(e.t)}</td>
            <td data-label="Kind">${esc(e.kind)}</td>
            <td data-label="Detail">${g ? `<span class="ae-status">${g}</span> ` : ""}${esc(e.body)}</td>
          </tr>`;
        }).join("");
        const plate = `<div class="ae-plate shell-sec">
            <p class="ae-plate-cap">LEDGER &middot; ${esc(D.name)} &middot; ${AGENT_METRICS.beats} BEATS</p>
            <table class="ae-table">
              <thead><tr><th>Time</th><th>Kind</th><th>Detail</th></tr></thead>
              <tbody>${rows}</tbody>
            </table>
            <p class="ae-plate-note">Session 16:45 &rarr; 18:31 &middot; one crash recovered (os error 28)</p>
          </div>`;
        return P.shell("now", `${agentHead}${plate}`);
      },
    },

    "AGENT-5": {
      label: "Stage (detail-first)",
      thesis: "Reverse the hierarchy: the latest surface dominates the view and the earlier history folds into a single thin .ae-fold — conversation-free.",
      build() {
        const earlier = D.trail.slice(1).map((e) => {
          const g = trailGlyph(e.kind);
          return `<div><span class="ae-trail-time">${esc(e.t)}</span> <span class="ae-trail-who">${esc(e.kind)}</span> <span>${g ? `<span class="ae-status">${g}</span> ` : ""}${esc(e.body)}</span></div>`;
        }).join("");
        const body = `${agentHead}
          <div class="shell-sec">
            <p class="ae-h">${esc(latestBeat.t)} &middot; ${esc(latestBeat.kind).toUpperCase()}</p>
            <p class="ae-item">${esc(latestBeat.body)}</p>
            <div class="shell-sec">${gatePlate}</div>
          </div>
          <div class="shell-sec">
            <details class="ae-fold"><summary>Earlier<span class="ae-dim">${D.trail.length - 1} beats &rarr;</span></summary><div class="lab-rows">${earlier}</div></details>
          </div>`;
        return P.shell("now", body);
      },
    },

    "AGENT-6": {
      label: "Filtered wire (inversion)",
      thesis: "Inversion: the agent page needs no bespoke layout — it is the ambient wire (same .ae-list-row component) scoped to one agent.",
      build() {
        const rows = D.trail.map((e) => P.wireRow({
          t: e.t, agent: D.name, kind: e.kind, cat: KIND_CAT[e.kind] ?? 5, title: e.body,
        })).join("");
        const body = `${crumbs}
          <p class="ae-status shell-sec">${stateGlyph}<span class="ae-status-label ae-item">${esc(D.name)}</span><span class="ae-dim">the wire, scoped &middot; ${D.trail.length} events</span></p>
          <div class="shell-sec ae-list-rows">${rows}</div>`;
        return P.shell("now", body);
      },
    },
  });
})();
