// Round 3 — REPORT DOCUMENTS, lane A (DOC-13, DOC-14, DOC-15).
// The operator killed the round-2 report docs: a report must be a component
// library of generative-UI instruments — assembled, composed, delightful to
// read — never a wall of prose. This lane builds its OWN instrument set and
// composes three structurally distinct documents from it. All content is
// DATA.synthesis (true fleet history, window 2026-07-07 18:00 → 18:30 UTC).
// Kit primitives + r3a- helpers; the shared mono code/diff/terminal chrome
// (r3b-*) is reused READ-ONLY. Static mockups stay static — intended reveals
// are noted in figcaptions only. Written as a SEPARATE Object.assign block so
// lane B (DOC-16..18) and this lane never touch each other's code.
(() => {
  const esc = parts.esc;
  const S = DATA.synthesis;
  const GEN = "18:28";
  // categorical hue per theme kind — identity, not severity (kept consistent
  // with the report family so a kind reads the same colour across docs).
  const KIND = { velocity: 1, reliability: 6, risk: 0 };
  const kindChip = (k) => `<span class="ae-chip ae-cat-${KIND[k] ?? 5}">${esc(k)}</span>`;

  // ── the header stat band (the header summary, not a prose run) ───────
  const numBand = () => parts.statBand([
    [S.numbers.completed, "completed"],
    [S.numbers.prs, "PRs merged"],
    [S.numbers.deploys, "deploy"],
    [S.numbers.releases, "release"],
    [S.numbers.blocked + S.numbers.questions, "need you", true],
  ]);

  // ── the velocity trend: a banner pen-stroke over the hourly series ───
  const spark = (cls) => {
    const H = S.hourly, lo = 2, hi = 14;
    const pts = H.map((d, i) => {
      const x = (i / (H.length - 1)) * 100;
      const y = 21 - ((d.n - lo) / (hi - lo)) * 18;
      return `${x.toFixed(2)},${y.toFixed(2)}`;
    }).join(" ");
    return `<svg class="ae-spark ${cls || ""}" viewBox="0 0 100 24" preserveAspectRatio="none" aria-hidden="true"><polyline points="${pts}"></polyline></svg>`;
  };

  // ── the pipeline as the kit's real .ae-flow diagram — hairline nodes,
  //    orthogonal wires, ✓ on cleared stages, the blocked stage dashed-faint
  //    with a warn glyph. (Lane B drew the same stages as stacked hairline
  //    rows; this is the SVG node/wire spine — a different instrument.) ──
  const flow = () => {
    const St = S.pipeline.stages;
    const NW = 80, NH = 52, GAP = 24, PAD = 12, TOP = 40;
    const W = PAD * 2 + St.length * NW + (St.length - 1) * GAP;
    const nx = (i) => PAD + i * (NW + GAP);
    const wires = St.slice(1).map((_, i) => {
      const x1 = nx(i) + NW, x2 = nx(i + 1);
      const cls = St[i + 1].state === "done" ? "is-reached" : "is-locked";
      return `<path class="ae-wire ${cls}" d="M ${x1} ${TOP + NH / 2} L ${x2} ${TOP + NH / 2}"></path>`;
    }).join("");
    const nodes = St.map((st, i) => {
      const done = st.state === "done";
      const x = nx(i), cx = x + NW / 2, ncls = done ? "is-done" : "is-locked";
      const glyph = done
        ? `<path class="ae-icon ae-ok" d="M ${x + NW - 22} ${TOP + 13} l 3 3 l 6 -7"></path>`
        : `<path class="ae-icon ae-warn" d="M ${x + NW - 24} ${TOP + 18} l 6 -10 l 6 10 z M ${x + NW - 18} ${TOP + 11} v 3 M ${x + NW - 18} ${TOP + 16} v 0.5"></path>`;
      return `<rect class="ae-node ${ncls}" x="${x}" y="${TOP}" width="${NW}" height="${NH}"></rect>
        <text class="ae-node-kicker" x="${cx}" y="${TOP + 18}" text-anchor="middle" dominant-baseline="middle">${String(i + 1).padStart(2, "0")}</text>
        <text class="ae-node-label${done ? "" : " is-locked"}" x="${cx}" y="${TOP + 37}" text-anchor="middle" dominant-baseline="middle">${esc(st.label)}</text>
        <text class="ae-node-port${done ? "" : " is-locked"}" x="${cx}" y="${TOP + NH + 14}" text-anchor="middle">${esc(st.note)}</text>
        ${glyph}`;
    }).join("");
    return `<div class="r3a-flowwrap"><svg class="ae-flow" viewBox="0 0 ${W} 120" role="img" aria-label="The redesign's path: six stages cleared, live-fire blocked on the Canary key">${wires}${nodes}</svg></div>`;
  };

  // ── the real diff — status on the gutter glyph, never a filled row
  //    (reuses the shared r3b- mono-block chrome, read-only) ───────────
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

  // ── the real deploy terminal — ✔ rides the ok hue, prompts stay faint ─
  const terminalBlock = () => {
    const Tm = S.terminal;
    const rows = Tm.lines.map((l) => {
      const ok = l.charAt(0) === "✔";
      const rest = ok ? l.slice(1).trim() : l.replace(/^>\s?/, "");
      const mark = ok
        ? `<span class="r3b-mark r3b-ok">✔</span>`
        : `<span class="r3b-mark r3b-prompt">&gt;</span>`;
      return `<div class="r3b-tline">${mark}<span>${esc(rest)}</span></div>`;
    }).join("");
    return `<div class="r3b-code r3b-scroll"><span class="r3b-cap">bastion-deploy@sanctum</span>${rows}</div>`;
  };

  // ── the by-repo distribution as a ruled meter stack ──────────────────
  const repoMeters = () => {
    const maxR = Math.max.apply(null, S.byRepo.map((x) => x[1]));
    return `<div class="r3a-repo">${S.byRepo.map(([r, n]) => `<div class="r3a-repo-row">
        <span class="ae-num r3a-repo-name">${esc(r)}</span>
        <span class="ae-meter"><span class="ae-meter-fill" style="width:${Math.round((n / maxR) * 100)}%"></span></span>
        <span class="ae-num ae-strong r3a-repo-n">${n}</span>
      </div>`).join("")}</div>`;
  };

  // ── the hourly throughput as an upright bar readout (CSS columns, the
  //    peak in accent) — the discrete cousin of the banner spark ───────
  const bars = () => {
    const H = S.hourly, hi = 14;
    return `<div class="r3a-bars" role="img" aria-label="Throughput per hour, peaking at 14 around 17:00 UTC">${
      H.map((d) => `<span class="r3a-bar-col">
        <span class="r3a-bar-track"><span class="r3a-bar-fill${d.n === hi ? " is-peak" : ""}" style="height:${Math.round((d.n / hi) * 100)}%"></span></span>
        <span class="r3a-bar-h">${esc(d.h)}</span>
      </span>`).join("")}</div>`;
  };

  // ── callouts: icon-forward status lines (hue on the glyph only) ──────
  const calloutLines = () => `<div class="r3a-calls">${S.callouts.map((c) =>
    `<span class="ae-status">${parts.icon(c.kind)}<span class="ae-status-label">${esc(c.text)}</span></span>`).join("")}</div>`;

  // ── the open asks as scannable status rows with a card tag + age chip ─
  const asksLines = () => `<div class="r3a-calls">${DATA.asks.map((a) => `<span class="ae-status">
      ${parts.icon("warn")}<span class="ae-status-label"><span class="ae-item">${esc(a.title)}</span>
      &ensp;<a href="#0" class="ae-tag">powder ${esc(a.card)}</a>
      <span class="ae-chip ae-cat-2">${esc(a.age)}</span></span></span>`).join("")}</div>`;

  // ── the decisions ratified this window, as ticked icon-rows ──────────
  const decisionRows = () => `<div class="r3a-decisions">${S.decisions.map((d) =>
    `<span class="ae-icon-row"><span class="ae-list-icon">${parts.icon("ok")}</span><span class="ae-icon-row-main"><span>${esc(d)}</span></span></span>`).join("")}</div>`;

  // ── a compact evidence tape: a few wire beats, mono, a kind chip ─────
  const evidenceTape = (idx) => `<div class="r3a-tape">${idx.map((i) => {
    const e = DATA.wire[i];
    return `<a class="r3a-tape-row" href="#0"><span class="r3a-tape-t">${esc(e.t)}</span>
      <span class="ae-chip ae-cat-${e.cat} r3a-tape-kind">${esc(e.kind)}</span>
      <span class="r3a-tape-text">${esc(e.title)}</span></a>`;
  }).join("")}</div>`;

  // ── the provenance line every generated report signs (existing helper) ─
  const provenance = () => `<p class="r2r-prov">${esc(S.query.resolved)} · sources: wire · powder · git · synthesized ${GEN} · 14.2s</p>`;

  Object.assign(window.SPECS, {

    // ══ DOC-13 — THE INSTRUMENT BRIEF ══════════════════════════════════
    // A hero band (headline + stat badges + banner spark), then a strict
    // call-and-response: each theme's prose is immediately answered by the
    // one instrument that proves it — the .ae-flow pipeline after velocity,
    // the diff after reliability, the warn callouts after risk. Closes on
    // ratified decisions as ticked rows and the provenance signature.
    "DOC-13": {
      label: "The instrument brief",
      thesis: "The report is a proof loop — every claim in prose is answered on the next line by the single instrument that substantiates it (pipeline, diff, callouts), never prose stacked on prose.",
      build() {
        const proof = (theme, exhibitCap, exhibit) => `<section class="r3a-proof">
            <div class="r3a-proof-claim">
              <p class="ae-h">${esc(theme.title.toUpperCase())}&ensp;${kindChip(theme.kind)}</p>
              <p>${esc(theme.body)}</p>
            </div>
            <figure class="r3b-exhibit r3a-proof-ex"><figcaption>${esc(exhibitCap)}</figcaption>${exhibit}</figure>
          </section>`;

        const doc = `<article class="ae-doc r3a-brief">
            <header class="r3a-hero">
              <div class="r2r-rhead"><p class="ae-plate-cap">FLEET DIGEST · PAST 24H · BRIEF</p><span class="r2r-cache">synthesized ${GEN} · 14.2s</span></div>
              <h1>${esc(S.headline)}</h1>
              ${numBand()}
              <div class="r3a-hero-trend">
                <span class="ae-h">VELOCITY</span>${spark("r3a-spark-hero")}<span class="ae-dim">peak 14 · 17:00 UTC</span>
              </div>
            </header>
            ${proof(S.themes[0], "the redesign's path — six stages cleared, live-fire the one dashed block · stages resolve left-to-right on reveal", flow())}
            ${proof(S.themes[1], "the smallest ship of the window, then the deploy that carried it — status rides the gutter and the ✔, never the row", diffBlock() + terminalBlock())}
            ${proof(S.themes[2], "the only human-gated items in the window", calloutLines() + asksLines())}
            <section class="r3a-close">
              <p class="ae-h">RATIFIED THIS WINDOW</p>
              ${decisionRows()}
            </section>
            ${provenance()}
          </article>`;
        return parts.shell("reports", doc);
      },
    },

    // ══ DOC-14 — THE MAGAZINE SPREAD ═══════════════════════════════════
    // Editorial asymmetry: a full-measure opening (headline, lede, one
    // pull-quote), then narrative rows where the story runs in the wide
    // column and a numbered exhibit sits in a narrow sidebar — repo meters,
    // agent highlights, the deploy terminal as FIG. Collapses to one column
    // on the phone (sidebar drops beneath its paragraph).
    "DOC-14": {
      label: "The magazine spread",
      thesis: "The report reads like a feature article — a full-measure lede and pull-quote open it, then each theme runs as narrative in the wide column with its proving exhibit set as a captioned sidebar figure.",
      build() {
        const row = (theme, sideCap, side) => `<section class="r3a-spread-row">
            <div class="r3a-spread-body">
              <p class="ae-h">${esc(theme.title.toUpperCase())}&ensp;${kindChip(theme.kind)}</p>
              <p>${esc(theme.body)}</p>
            </div>
            <figure class="r3a-spread-side"><figcaption>${esc(sideCap)}</figcaption>${side}</figure>
          </section>`;

        const highlights = `<div class="r3a-hl">${S.agentHighlights.map((h) =>
          `<div class="r3a-hl-row"><span class="ae-chip ae-cat-1">${esc(h.agent)}</span><span class="r3a-hl-line">${esc(h.line)}</span></div>`).join("")}</div>`;

        const doc = `<article class="ae-doc r3a-mag">
            <header class="r3a-mag-open">
              <div class="r2r-rhead"><p class="ae-plate-cap">FLEET DIGEST · PAST 24H · THE FEATURE</p><span class="r2r-cache">synthesized ${GEN} · 14.2s</span></div>
              <h1>${esc(S.headline)}</h1>
              <p class="ae-lede">${esc(S.tldr)}</p>
              <blockquote class="ae-pull">${esc(S.decisions[0])}<span class="ae-pull-by">ratified · VISION.md</span></blockquote>
            </header>
            ${row(S.themes[0], "where the 58 completions landed, by repository", repoMeters())}
            ${row(S.themes[1], "the sanctum deploy that carried the merges live, 18:24 UTC", terminalBlock())}
            ${row(S.themes[2], "who moved the needle — and who is waiting on you", highlights + asksLines())}
            ${provenance()}
          </article>`;
        return parts.shell("reports", doc);
      },
    },

    // ══ DOC-15 — THE CONSOLE READOUT ═══════════════════════════════════
    // The inversion of DOC-14: maximum instrument, minimum narrative. One
    // tldr line and a stat band head a tight grid of labelled readout cells
    // — hourly bars, the pipeline spine, the per-repo meter stack, callout
    // status lines, and a short evidence tape. Prose exists only as the one
    // standfirst line and the cell captions.
    "DOC-15": {
      label: "The console readout",
      thesis: "The report is an instrument console — the whole state is read off a grid of labelled cells (bars, pipeline, meters, status lines, evidence tape) with prose demoted to a single standfirst line and cell captions.",
      build() {
        const cell = (cap, body, wide) => `<figure class="r3a-cell${wide ? " r3a-cell-wide" : ""}"><figcaption>${esc(cap)}</figcaption>${body}</figure>`;

        const doc = `<article class="ae-doc r3a-console">
            <header class="r3a-console-head">
              <div class="r2r-rhead"><p class="ae-plate-cap">CONSOLE · FLEET · PAST 24H</p><span class="r2r-cache">live · synthesized ${GEN}</span></div>
              <p class="r3a-console-line"><span class="ae-strong">${esc(S.headline)}.</span> ${esc(S.tldr)}</p>
              ${numBand()}
            </header>
            <div class="r3a-grid">
              ${cell("throughput per hour — peak 14 at 17:00 · bars grow in on reveal", bars())}
              ${cell("by repository — 58 completions across 8 repos", repoMeters())}
              ${cell("the path — six stages cleared, live-fire blocked", flow(), true)}
              ${cell("state of play", calloutLines())}
              ${cell("evidence tape — the window's load-bearing beats", evidenceTape([0, 1, 2, 3, 4]), true)}
            </div>
            ${provenance()}
          </article>`;
        return parts.shell("reports", doc);
      },
    },

  });
})();
