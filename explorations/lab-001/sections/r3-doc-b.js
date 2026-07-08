// Round 3 — REPORT DOCUMENTS, lane B (DOC-16, DOC-17, DOC-18).
// The operator killed the round-2 report docs: a report must be a component
// library of generative-UI instruments — assembled, composed, and delightful
// to read — not a wall of prose. Everything below builds ONE shared instrument
// set (mono code/diff/terminal, a CSS pipeline, a bar chart, a banner spark,
// meters, a trail/tape, badges + callouts) and then composes three structurally
// distinct documents from it. All content is DATA.synthesis (true fleet
// history, window 2026-07-07 18:00 → 2026-07-08 18:30 UTC). Kit primitives +
// r3b- helpers only; static mockups stay static (entrance reveals noted in
// figcaptions). Written as a SEPARATE Object.assign block so lane A (DOC-13..15)
// and this lane never touch each other's code.
(() => {
  const esc = parts.esc;
  const S = DATA.synthesis;
  const GEN = "18:28";

  // ── categorical + evidence primitives ───────────────────────────────
  const KIND = { velocity: 1, reliability: 6, risk: 0 };
  const kindTag = (k) => `<span class="ae-chip ae-cat-${KIND[k] ?? 5}">${esc(k)}</span>`;
  const evLine = (ev) => `<p class="r2r-ev"><span class="r2r-ev-k">evidence</span> ${
    ev.map(([l]) => `<a href="#0" class="ae-tag">${esc(l)}</a>`).join(" ")}</p>`;

  // ── the headline number band (stat badges: value + label + optional warn) ─
  const numBand = () => parts.statBand([
    [S.numbers.completed, "completed"],
    [S.numbers.prs, "PRs merged"],
    [S.numbers.deploys, "deploy"],
    [S.numbers.releases, "release"],
    [S.numbers.blocked + S.numbers.questions, "need you", true],
  ]);

  // ── the velocity trend: a banner sparkline (posts + completions / hour) ──
  const hourlySpark = (cls) => {
    const H = S.hourly, lo = 2, hi = 14;
    const pts = H.map((d, i) => {
      const x = (i / (H.length - 1)) * 100;
      const y = 21 - ((d.n - lo) / (hi - lo)) * 18;
      return `${x.toFixed(2)},${y.toFixed(2)}`;
    }).join(" ");
    return `<svg class="ae-spark ${cls || ""}" viewBox="0 0 100 24" preserveAspectRatio="none" aria-hidden="true"><polyline points="${pts}"></polyline></svg>`;
  };

  // ── the same series as a real axed bar chart, dressed on .ae-plot chrome ──
  const hourlyPlot = () => {
    const H = S.hourly, hi = 14, L = 40, R = 470, T = 16, B = 150;
    const slot = (R - L) / H.length, bw = slot * 0.56;
    const bars = H.map((d, i) => {
      const bh = (d.n / hi) * (B - T - 4);
      const x = L + i * slot + (slot - bw) / 2, y = B - bh, peak = d.n === hi;
      return `<rect class="r3b-bar${peak ? " is-peak" : ""}" x="${x.toFixed(1)}" y="${y.toFixed(1)}" width="${bw.toFixed(1)}" height="${bh.toFixed(1)}"></rect>` +
        `<text class="ae-plot-ticklabel" x="${(L + i * slot + slot / 2).toFixed(1)}" y="166" text-anchor="middle">${esc(d.h)}</text>`;
    }).join("");
    const axes = `<path class="ae-plot-axis" d="M${L} ${T} L${L} ${B} L${R} ${B}"></path>`;
    const grid = `<line class="ae-plot-crosshair" x1="${L}" y1="${T + 4}" x2="${R}" y2="${T + 4}"></line>` +
      `<text class="ae-plot-ticklabel" x="${L - 6}" y="${T + 8}" text-anchor="end">14</text>` +
      `<text class="ae-plot-ticklabel" x="${L - 6}" y="${B}" text-anchor="end">0</text>`;
    return `<div class="r3b-plotwrap"><svg class="ae-plot" viewBox="0 0 480 182" role="img" aria-label="Completions and posts per hour, peaking at 17:00">${axes}${grid}${bars}</svg></div>`;
  };

  // ── the by-repo distribution as ruled meters ─────────────────────────
  const byRepoMeters = () => {
    const maxR = Math.max.apply(null, S.byRepo.map((x) => x[1]));
    return `<div class="rep-dist">${S.byRepo.map(([r, n]) => `<div class="rep-dist-row">
        <span class="ae-num">${esc(r)}</span>
        <span class="ae-meter"><span class="ae-meter-fill" style="width:${Math.round((n / maxR) * 100)}%"></span></span>
        <span class="ae-num ae-strong">${n}</span>
      </div>`).join("")}</div>`;
  };

  // ── the pipeline: hairline stages, status on the glyph, the one block warns ─
  const pipeline = () => `<div class="r3b-pipe">${S.pipeline.stages.map((st) => {
    const blocked = st.state !== "done";
    const glyph = blocked ? parts.icon("warn") : parts.icon("tick");
    return `<div class="r3b-stage${blocked ? " is-blocked" : ""}">
        <span class="r3b-stage-head"><span class="r3b-stage-glyph">${glyph}</span><span class="r3b-stage-label">${esc(st.label)}</span></span>
        <span class="r3b-stage-note">${esc(st.note)}</span>
      </div>`;
  }).join("")}</div>`;

  // ── the real diff — status on the gutter glyph, never a filled row ───
  const diffBlock = () => {
    const D = S.diff;
    const rows = D.lines.map(([k, txt]) => {
      const sign = k === "add" ? "+" : k === "del" ? "−" : " ";
      const gc = k === "add" ? " r3b-add" : k === "del" ? " r3b-del" : "";
      return `<div class="r3b-line"><span class="r3b-gut${gc}">${sign}</span><span>${esc(txt)}</span></div>`;
    }).join("");
    return `<figure class="r3b-exhibit"><figcaption>diff · ${esc(D.file)} — ${esc(D.caption)}</figcaption>
      <div class="r3b-code r3b-scroll">${rows}</div></figure>`;
  };

  // ── the real deploy terminal — ✔ rides the ok hue, prompts stay faint ──
  const terminalBlock = () => {
    const Tm = S.terminal;
    const rows = Tm.lines.map((l) => {
      const ok = l.charAt(0) === "✔";
      const rest = l.slice(1).trim();
      const mark = ok
        ? `<span class="r3b-mark r3b-ok">✔</span>`
        : `<span class="r3b-mark r3b-prompt">&gt;</span>`;
      return `<div class="r3b-tline">${mark}<span>${esc(rest)}</span></div>`;
    }).join("");
    return `<figure class="r3b-exhibit"><figcaption>terminal · ${esc(Tm.caption)}</figcaption>
      <div class="r3b-code r3b-scroll"><span class="r3b-cap">bastion-deploy@sanctum</span>${rows}</div></figure>`;
  };

  // ── callouts: icon-forward status rows (hue on the glyph) ────────────
  const callouts = () => `<div class="r3b-callouts">${S.callouts.map((c) =>
    `<span class="ae-status">${parts.icon(c.kind)}<span class="ae-status-label">${esc(c.text)}</span></span>`).join("")}</div>`;

  // ── the open asks as a scannable chip/label cluster ──────────────────
  const asksList = () => `<div class="r3b-callouts">${DATA.asks.map((a) => `<span class="ae-status">
      ${parts.icon("warn")}<span class="ae-status-label"><span class="ae-item">${esc(a.title)}</span>
      &ensp;<a href="#0" class="ae-tag">powder ${esc(a.card)}</a>
      <span class="ae-chip ae-cat-2">${esc(a.age)}</span></span></span>`).join("")}</div>`;

  // ── a small kind glyph for the ledger tape (shape scans; hue stays on the
  //    chip word — law-pure: the glyph differentiates by form, not colour) ──
  const GA = 'class="ae-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"';
  const KG = {
    shipped: `<svg ${GA}><path d="M12 3v13"></path><path d="m7 8 5-5 5 5"></path><path d="M5 21h14"></path></svg>`,
    report: `<svg ${GA}><path d="M14 3H6a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"></path><path d="M14 3v5h5"></path><path d="M9 13h6"></path><path d="M9 17h4"></path></svg>`,
    receipt: `<svg ${GA}><circle cx="12" cy="12" r="9"></circle><path d="m8.5 12 2.5 2.5 4.5-4.5"></path></svg>`,
    blocked: `<svg ${GA}><circle cx="12" cy="12" r="9"></circle><path d="m5.6 5.6 12.8 12.8"></path></svg>`,
    question: `<svg ${GA}><circle cx="12" cy="12" r="9"></circle><path d="M9.2 9a3 3 0 0 1 5.6 1c0 2-3 2.5-3 4"></path><path d="M12 17h.01"></path></svg>`,
    release: `<svg ${GA}><path d="M3 12V6a1 1 0 0 1 1-1h6l9 9-7 7-9-9z"></path><path d="M7.5 8.5h.01"></path></svg>`,
    note: `<svg ${GA}><path d="M12 20h9"></path><path d="M16.5 3.5a2.1 2.1 0 0 1 3 3L8 18l-4 1 1-4z"></path></svg>`,
  };
  const kindGlyph = (k) => KG[k] || KG.note;

  // ── one wire event as a mono tape row ────────────────────────────────
  const tapeRow = (e) => `<a class="r3b-tape-row" href="#0">
      <span class="r3b-tape-time">${esc(e.t)}</span>
      <span class="r3b-tape-glyph">${kindGlyph(e.kind)}</span>
      <span class="r3b-tape-body">
        <span class="ae-chip ae-cat-${e.cat}">${esc(e.kind)}</span>
        <span class="r3b-tape-agent">${esc(e.agent)}</span>
        <span class="r3b-tape-text">${esc(e.title)}</span>
      </span>
    </a>`;

  // ── the provenance footer every generated report signs ───────────────
  const provenance = () => `<p class="r2r-prov">${esc(S.query.resolved)} · sources: wire · powder · git · synthesized ${GEN} · 14.2s</p>`;

  Object.assign(window.SPECS, {

    // ══ DOC-16 — THE STORY SCROLL ═══════════════════════════════════════
    // One continuous narrative arc in three acts, each opening on a lifted
    // pull line and closing on its instrument exhibit; a thin act-marker rail
    // (mono numerals on a hairline spine) runs the left margin.
    "DOC-16": {
      label: "The story scroll",
      thesis: "The report is one narrative arc — what happened, the incident, what needs you — read down a hairline act-rail, each act opening on a pull line and closing on the instrument that proves it.",
      build() {
        const act = (num, kicker, here, pull, pullBy, body, exhibit) => `
          <div class="r3b-actnum${here ? " is-here" : ""}">${here ? `<span class="r3b-actmark"></span>` : ""}<span class="r3b-actnum-v">${num}</span></div>
          <section class="r3b-actbody">
            <p class="ae-h">${esc(kicker)}</p>
            <blockquote class="ae-pull">${esc(pull)}<span class="ae-pull-by">${esc(pullBy)}</span></blockquote>
            <p>${esc(body)}</p>
            ${exhibit}
          </section>`;

        const act1Exhibit = `
          <figure class="r3b-exhibit"><figcaption>the redesign's path through the window — every stage green but live-fire · reveals stage by stage on scroll</figcaption>${pipeline()}</figure>
          <figure class="r3b-exhibit"><figcaption>throughput per hour, 06:00–19:00 — the 17:00 crest is the merge train landing</figcaption>
            <div class="r3b-trendrow"><span class="ae-h">VELOCITY</span><span class="ae-dim">peak 14 · 17:00 UTC</span></div>${hourlySpark("r3b-spark-lg")}</figure>`;

        const doc = `<article class="ae-doc">
            <header>
              <div class="r2r-rhead"><p class="ae-plate-cap">FLEET DIGEST · PAST 24H</p><span class="r2r-cache">synthesized ${GEN} · 14.2s</span></div>
              <h1>${esc(S.headline)}</h1>
              <p class="ae-lede">${esc(S.tldr)}</p>
            </header>
            ${numBand()}
            <div class="r3b-acts">
              ${act("01", "WHAT HAPPENED", true, S.themes[0].title, "act one · velocity", S.themes[0].body, act1Exhibit)}
              ${act("02", "THE INCIDENT", false, S.themes[1].title, "act two · reliability", S.themes[1].body, diffBlock() + terminalBlock())}
              ${act("03", "WHAT NEEDS YOU", false, S.themes[2].title, "act three · risk", S.themes[2].body, callouts() + asksList())}
            </div>
            ${provenance()}
          </article>`;
        return parts.shell("reports", doc);
      },
    },

    // ══ DOC-17 — THE DASHBOARD-FIRST FOLD ══════════════════════════════
    // Screen one is all signal: headline, stat badges, the hourly chart, the
    // by-repo meters, the pipeline, the callouts — the whole state, no scroll.
    // Below a labelled fold rule, each theme's .ae-fold opens into its
    // narrative + evidence + exhibit (skim in 5 seconds, read in 5 minutes).
    "DOC-17": {
      label: "The dashboard-first fold",
      thesis: "Everything you need to know the fleet's state sits above one labelled fold — numbers, chart, meters, pipeline, callouts — and the narrative lives below it inside disclosures you open only for depth.",
      build() {
        const panel = (title, meta, body) => `<section>
            <div class="r3b-panel-h"><p class="ae-h">${esc(title)}</p>${meta ? `<span class="r3b-fold-metric">${esc(meta)}</span>` : ""}</div>
            ${body}
          </section>`;

        const aboveFold = `<div class="r3b-dash">
            ${numBand()}
            <div class="r3b-dash-2col">
              ${panel("VELOCITY · PER HOUR", "peak 14 · 17:00", hourlyPlot())}
              ${panel("BY REPO", "8 repos", byRepoMeters())}
            </div>
            ${panel("THE PATH", "6 of 7 stages clear", pipeline())}
            ${callouts()}
          </div>`;

        const foldMetric = ["5 glass PRs", "1 incident", "1 blocked · 2 questions"];
        const foldExhibit = ["", diffBlock() + terminalBlock(), asksList()];
        const themeFolds = S.themes.map((t, i) => `<details class="ae-fold"${i === 0 ? " open" : ""}>
            <summary><span>${esc(t.title)}&ensp;${kindTag(t.kind)}</span><span class="r3b-fold-metric">${esc(foldMetric[i])}</span></summary>
            <p>${esc(t.body)}</p>
            ${evLine(t.evidence)}
            ${foldExhibit[i]}
          </details>`).join("");
        const decisionsFold = `<details class="ae-fold"><summary><span>Decisions ratified this window</span><span class="r3b-fold-metric">${DATA.synthesis.decisions.length}</span></summary>
            <ul>${DATA.synthesis.decisions.map((d) => `<li>${esc(d)}</li>`).join("")}</ul></details>`;

        const doc = `<article class="ae-doc">
            <header>
              <div class="r2r-rhead"><p class="ae-plate-cap">FLEET DIGEST · PAST 24H · SIGNAL</p><span class="r2r-cache">live · synthesized ${GEN}</span></div>
              <h1>${esc(S.headline)}</h1>
            </header>
            ${aboveFold}
            <div class="r3b-fold-hr"><span class="r3b-fold-cap">THE FOLD · read on ↓</span></div>
            ${themeFolds}
            ${decisionsFold}
            ${provenance()}
          </article>`;
        return parts.shell("reports", doc);
      },
    },

    // ══ DOC-18 — THE ANNOTATED LEDGER ══════════════════════════════════
    // The wire's own record is the spine: a dense chronological tape (mono,
    // a glyph per kind). At the three theme moments the synthesis interrupts
    // full-width — the model's voice literally annotating the machine record,
    // each annotation carrying its badge row and evidence.
    "DOC-18": {
      label: "The annotated ledger",
      thesis: "The report is the raw wire annotated: a mono event tape carries the window chronologically, and at three moments the LLM's synthesis breaks in full-width as an accented margin note grown to the measure.",
      build() {
        const W = DATA.wire;
        const legendKinds = ["shipped", "report", "receipt", "blocked", "question", "release", "note"];
        const legend = `<div class="r3b-legend">${legendKinds.map((k) =>
          `<span class="r3b-leg">${kindGlyph(k)}${esc(k)}</span>`).join("")}</div>`;

        const annot = (t, badges, evExhibit) => `<div class="r3b-annot">
            <p class="r3b-annot-k">SYNTHESIS · <b>${esc(t.kind)}</b></p>
            <h3>${esc(t.title)}</h3>
            <div class="r3b-badges">${kindTag(t.kind)}${badges}</div>
            <p>${esc(t.body)}</p>
            ${evLine(t.evidence)}
            ${evExhibit || ""}
          </div>`;

        const velocityBadges = parts.statBand([[S.numbers.completed, "completed"], [S.numbers.prs, "PRs"], [S.numbers.deploys, "deploy"]]);
        const reliabilityBadges = parts.statBand([[S.numbers.incidents, "incident", true], [1, "recovered"]]) + `<span class="ae-tag">41GB reclaimed</span>`;
        const riskBadges = parts.statBand([[S.numbers.blocked, "blocked", true], [S.numbers.questions, "questions"]]);

        const velocityExhibit = `<figure class="r3b-exhibit"><figcaption>throughput per hour — the crest is this record's merge train · draws in on reveal</figcaption>
            <div class="r3b-trendrow"><span class="ae-h">PER HOUR</span><span class="ae-dim">peak 14 · 17:00</span></div>${hourlySpark("r3b-spark-lg")}</figure>`;
        const reliabilityExhibit = diffBlock() + terminalBlock();

        const tape = [
          tapeRow(W[0]), tapeRow(W[1]), tapeRow(W[2]), tapeRow(W[3]),
          annot(S.themes[0], velocityBadges, velocityExhibit),
          tapeRow(W[4]),
          annot(S.themes[2], riskBadges, asksList()),
          tapeRow(W[5]), tapeRow(W[6]),
          annot(S.themes[1], reliabilityBadges, reliabilityExhibit),
          tapeRow(W[7]), tapeRow(W[8]), tapeRow(W[9]),
        ].join("");

        const doc = `<article class="ae-doc">
            <header>
              <div class="r2r-rhead"><p class="ae-plate-cap">THE RECORD · FLEET · PAST 24H</p><span class="r2r-cache">synthesized ${GEN} · 14.2s</span></div>
              <h1>${esc(S.headline)}</h1>
              <p class="ae-lede">${esc(S.tldr)}</p>
            </header>
            <div class="r3b-ledger">
              ${legend}
              <div class="r3b-tape">${tape}</div>
            </div>
            ${provenance()}
          </article>`;
        return parts.shell("reports", doc);
      },
    },

  });
})();
