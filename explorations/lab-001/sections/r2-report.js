// Round 2 — REPORTS as ask & render (REP-7..12) and the synthesis document
// itself (DOC-7..12). Round-1 premise (a library of persisted artifacts) was
// killed: there is NO library page. The operator picks scope × window and the
// synthesized report renders IMMEDIATELY in place; repeat queries serve a cache
// with a quiet "cached · generated" note. The document is a generative-UI
// narrative (from DATA.synthesis), never a completions table. Kit primitives
// only; all content from DATA; helpers appended to frame.css are prefixed r2r-.
(() => {
  const esc = parts.esc;
  const S = DATA.synthesis;
  const GEN = "18:28";

  // ── synthesis building blocks (shared by REP embeds and DOC options) ──
  const KIND = { velocity: 1, reliability: 6, risk: 0 };
  const kindTag = (k) => `<span class="ae-chip ae-cat-${KIND[k] ?? 5}">${esc(k)}</span>`;
  const evChips = (ev) => ev.map(([l]) => `<a href="#0" class="ae-tag">${esc(l)}</a>`).join(" ");

  const numBand = () => parts.statBand([
    [S.numbers.completed, "completed"],
    [S.numbers.prs, "PRs merged"],
    [S.numbers.deploys, "deploy"],
    [S.numbers.blocked + S.numbers.questions, "need you", true],
  ]);

  const maxRepo = Math.max.apply(null, S.byRepo.map((x) => x[1]));
  const repoMeters = () => S.byRepo.map(([r, n]) => `<div class="rep-dist-row">
      <span class="ae-num">${esc(r)}</span>
      <span class="ae-meter"><span class="ae-meter-fill" style="width:${Math.round((n / maxRepo) * 100)}%"></span></span>
      <span class="ae-num ae-strong">${n}</span>
    </div>`).join("");

  const decisionsList = () => `<ul>${S.decisions.map((d) => `<li>${esc(d)}</li>`).join("")}</ul>`;

  // the standing provenance footer every report renders
  const provenance = (cached) => `<p class="r2r-prov">${esc(S.query.resolved)} · sources: wire · powder · git · generated ${GEN} · ${cached ? "cached" : "live"}</p>`;
  const cacheNote = (cached) => cached
    ? `<span class="r2r-cache">cached · generated ${GEN}</span>`
    : `<span class="r2r-cache">generated ${GEN} · 14.2s</span>`;

  // a compact theme block for the embedded (in-place) report
  const themeBrief = (t) => `<section class="r2r-theme">
      <p class="r2r-theme-h"><span class="ae-item">${esc(t.title)}</span> ${kindTag(t.kind)}</p>
      <p>${esc(t.body)}</p>
      <p class="r2r-ev"><span class="r2r-ev-k">evidence</span> ${evChips(t.evidence)}</p>
    </section>`;

  // THE embedded report the query surfaces render in place. This is what the
  // ask produces; the DOC-* options below explore its full-document forms.
  const miniReport = (cached = true) => `<article class="ae-doc r2r-report">
      <header>
        <div class="r2r-rhead"><p class="ae-plate-cap">FLEET · PAST 24H</p>${cacheNote(cached)}</div>
        <h1>${esc(S.headline)}</h1>
        <p class="ae-lede">${esc(S.tldr)}</p>
      </header>
      ${numBand()}
      <div class="r2r-themes">${S.themes.map(themeBrief).join("")}</div>
      ${provenance(cached)}
    </article>`;

  // ── shared query controls ─────────────────────────────────────────────
  const scopeChip = (v) => `<button type="button" class="r2r-chip"><span class="r2r-k">SCOPE</span>${esc(v)}&ensp;▾</button>`;
  const windowChip = (v) => `<button type="button" class="r2r-chip"><span class="r2r-k">WINDOW</span>${esc(v)}&ensp;▾</button>`;
  const runBtn = (label = "Run") => `<button type="button" class="ae-button ae-button-compact">${esc(label)}</button>`;
  const queryBar = () => `<div class="r2r-bar">
      ${scopeChip("Whole fleet")}
      ${windowChip("Past 24h")}
      <span class="r2r-bar-run">${runBtn()}</span>
    </div>`;

  Object.assign(window.SPECS, {

    // ══ REP: the ask & render query surface ════════════════════════════
    "REP-7": {
      label: "Query bar",
      thesis: "The search-engine cut: one hairline bar of a scope chip + a window chip + Run, and the synthesized report renders in place directly below it.",
      build() {
        return parts.shell("reports", `<div class="r2r-stack">
          ${queryBar()}
          ${miniReport(true)}
        </div>`);
      },
    },

    "REP-8": {
      label: "Sentence-builder",
      thesis: "The query reads as one plain-English sentence whose nouns are editable slot tokens — 'Show me [the whole fleet] over [the past 24h]' — with the report rendered beneath.",
      build() {
        const slot = (s) => `<button type="button" class="rep-slot">${esc(s)}</button>`;
        const sentence = `<p class="r2r-sentence">Show me ${slot("the whole fleet")} over ${slot("the past 24h")}&ensp;${runBtn("Run")}</p>`;
        return parts.shell("reports", `<div class="r2r-stack">
          ${sentence}
          ${miniReport(true)}
        </div>`);
      },
    },

    "REP-9": {
      label: "Split pane",
      thesis: "A thin query rail on the left (Fleet, then every agent as a scope; window presets below) and the report fills the right — pick a scope and the answer swaps in place, no page turn.",
      build() {
        const scopes = ["Whole fleet"].concat(DATA.agents.slice(0, 8).map((a) => a.name));
        const scopeList = `<nav class="r2r-scope-list">${scopes.map((s, i) =>
          `<a href="#0" class="r2r-scope${i === 0 ? " is-active" : ""}">${esc(s)}</a>`).join("")}</nav>`;
        const windows = ["Past hour", "Past 24h", "Past week", "Past month", "Custom…"];
        const winList = `<div class="r2r-win">${windows.map((w, i) =>
          `<button type="button" class="r2r-chip${i === 1 ? " is-active" : ""}">${esc(w)}</button>`).join("")}</div>`;
        const qrail = `<aside class="r2r-qrail">
            <p class="ae-h">SCOPE</p>${scopeList}
            <p class="ae-h r2r-mt">WINDOW</p>${winList}
            <div class="r2r-mt">${runBtn("Run")}</div>
          </aside>`;
        return parts.shell("reports", `<div class="r2r-split">
          ${qrail}
          <div class="r2r-splitmain">${miniReport(true)}</div>
        </div>`);
      },
    },

    "REP-10": {
      label: "Command palette",
      thesis: "The query is a centered command-palette panel — a search input plus scope/window chips and recent runs — floating over the dimmed previous report, dismissed to reveal the answer.",
      build() {
        const winChips = ["Hour", "24h", "Week", "Month", "Custom"].map((w, i) =>
          `<button type="button" class="r2r-chip${i === 1 ? " is-active" : ""}">${esc(w)}</button>`).join("");
        const recent = ["Whole fleet · past 24h", "canary-lane · past week", "repo glass · past month"];
        const recentChips = recent.map((q) => `<a href="#0" class="r2r-qchip">${esc(q)}</a>`).join("");
        const palette = `<div class="r2r-palette ae-panel">
            <p class="ae-plate-cap">NEW REPORT</p>
            <input class="ae-input" type="text" placeholder="scope and window… e.g. canary-lane, past week" aria-label="Query">
            <div class="r2r-palette-row"><p class="ae-h">SCOPE</p>${scopeChip("Whole fleet")}</div>
            <div class="r2r-palette-row"><p class="ae-h">WINDOW</p><span class="r2r-win">${winChips}</span></div>
            <p class="ae-h r2r-mt">RECENT</p>
            <div class="r2r-recent">${recentChips}</div>
            <div class="ae-dialog-acts">${runBtn("Run report")}</div>
          </div>`;
        const screen = `<div class="r2r-modalscreen">
            <div class="r2r-dimmed">${miniReport(true)}</div>
            <div class="r2r-modallayer">${palette}</div>
          </div>`;
        return parts.shell("reports", screen);
      },
    },

    "REP-11": {
      label: "Zero-page inversion",
      thesis: "Inverts the load-bearing assumption that reports need their own place: there is no /reports page — the query bar sits atop NOW and the report renders where the agent desk would be.",
      build() {
        const live = DATA.agents.filter((a) => a.state === "publishing").map((a) => esc(a.name)).join("&ensp;·&ensp;");
        const nowhead = `<div class="r2r-nowhead">
            <p class="ae-h">NOW&ensp;·&ensp;${DATA.stats.live} live&ensp;·&ensp;${DATA.stats.quiet} quiet&ensp;·&ensp;${DATA.stats.needYou} need you</p>
            <p class="r2r-nowlive">${live}</p>
          </div>`;
        return parts.shell("now", `<div class="r2r-stack">
          ${nowhead}
          ${queryBar()}
          ${miniReport(true)}
        </div>`);
      },
    },

    "REP-12": {
      label: "History-as-cache",
      thesis: "The cache is visible but never a library: the query bar on top, one quiet mono line of three recent cached queries as re-runnable chips, and the current report below.",
      build() {
        const recent = ["Whole fleet · past 24h", "canary-lane · past week", "repo glass · past month"];
        const cacheLine = `<p class="r2r-cacheline"><span class="r2r-cacheline-k">cached</span>${recent.map((q, i) =>
          `<a href="#0" class="r2r-qchip${i === 0 ? " is-active" : ""}">${esc(q)}</a>`).join("")}</p>`;
        return parts.shell("reports", `<div class="r2r-stack">
          ${queryBar()}
          ${cacheLine}
          ${miniReport(true)}
        </div>`);
      },
    },

    // ══ DOC: the synthesis document itself ═════════════════════════════
    "DOC-7": {
      label: "Briefing memo",
      thesis: "The staff-brief cut: a headline, a lede that IS the tl;dr, then one titled section per theme with its kind tag and evidence chips, closing on a decisions list.",
      build() {
        const themeSection = (t) => `<section class="r2r-doc-theme">
            <h3>${esc(t.title)}&ensp;${kindTag(t.kind)}</h3>
            <p>${esc(t.body)}</p>
            <p class="r2r-ev"><span class="r2r-ev-k">evidence</span> ${evChips(t.evidence)}</p>
          </section>`;
        const doc = `<article class="ae-doc">
            <header>
              <div class="r2r-rhead"><p class="ae-plate-cap">FLEET DIGEST · PAST 24H</p>${cacheNote(false)}</div>
              <h1>${esc(S.headline)}</h1>
              <p class="ae-lede">${esc(S.tldr)}</p>
            </header>
            ${numBand()}
            ${S.themes.map(themeSection).join("")}
            <section class="r2r-doc-theme"><p class="ae-h">DECISIONS THIS WINDOW</p>${decisionsList()}</section>
            ${provenance(false)}
          </article>`;
        return parts.shell("reports", doc);
      },
    },

    "DOC-8": {
      label: "Findings-first",
      thesis: "Conclusion up front: a findings box of every headline number with an accent takeaway, then each theme folded into a disclosure the reader opens only for the detail.",
      build() {
        const numRows = [
          ["Cards completed", S.numbers.completed], ["PRs merged", S.numbers.prs],
          ["Deploys", S.numbers.deploys], ["Incidents", S.numbers.incidents],
          ["Releases", S.numbers.releases], ["Blocked", S.numbers.blocked],
          ["Open questions", S.numbers.questions],
        ].map(([l, v]) => `<div class="rep-dist-row"><span>${esc(l)}</span><span></span><span class="ae-num ae-strong">${v}</span></div>`).join("");
        const findings = `<section class="ae-findings">
            <p class="ae-findings-title">FINDINGS · PAST 24H · ${esc(S.query.resolved)}</p>
            <div class="rep-dist">${numRows}</div>
            <p class="ae-rec">${esc(S.tldr)}</p>
          </section>`;
        const folds = S.themes.map((t) => `<details class="ae-fold">
            <summary>${esc(t.title)}&ensp;${kindTag(t.kind)}</summary>
            <p>${esc(t.body)}</p>
            <p class="r2r-ev"><span class="r2r-ev-k">evidence</span> ${evChips(t.evidence)}</p>
          </details>`).join("");
        const decisions = `<details class="ae-fold"><summary>Decisions this window</summary>${decisionsList()}</details>`;
        const doc = `<article class="ae-doc">
            <header><p class="ae-plate-cap">FLEET DIGEST · PAST 24H</p><h1>${esc(S.headline)}</h1></header>
            ${findings}
            ${folds}
            ${decisions}
            ${provenance(false)}
          </article>`;
        return parts.shell("reports", doc);
      },
    },

    "DOC-9": {
      label: "Narrative + margin",
      thesis: "A prose column carries the story while a right margin rail holds the instruments — the headline numbers and the by-repo meters read alongside the text, not inside it.",
      build() {
        const numStack = [
          ["completed", S.numbers.completed], ["PRs", S.numbers.prs], ["deploys", S.numbers.deploys],
          ["incidents", S.numbers.incidents], ["blocked", S.numbers.blocked], ["questions", S.numbers.questions],
        ].map(([l, v]) => `<div class="r2r-numrow"><span class="ae-num ae-strong">${v}</span><span class="r2r-numl">${esc(l)}</span></div>`).join("");
        const highlights = S.agentHighlights.map((h) =>
          `<p class="r2r-hl"><span class="ae-item">${esc(h.agent)}</span><span class="r2r-hl-line">${esc(h.line)}</span></p>`).join("");
        const prose = `<div class="r2r-mcol">
            <h1>${esc(S.headline)}</h1>
            <p class="ae-lede">${esc(S.tldr)}</p>
            ${S.themes.map((t) => `<p><span class="ae-item">${esc(t.title)}.</span> ${esc(t.body)}</p>`).join("")}
            <p class="ae-h">DECISIONS</p>${decisionsList()}
            ${provenance(false)}
          </div>`;
        const rail = `<aside class="r2r-mrail">
            <p class="ae-h">NUMBERS</p><div class="r2r-nums">${numStack}</div>
            <p class="ae-h r2r-mt">BY REPO</p><div class="rep-dist">${repoMeters()}</div>
            <p class="ae-h r2r-mt">WHO MOVED</p>${highlights}
          </aside>`;
        const doc = `<article class="ae-doc r2r-margin">
            <div class="r2r-rhead"><p class="ae-plate-cap">FLEET DIGEST · PAST 24H</p>${cacheNote(false)}</div>
            <div class="r2r-margin-grid">${prose}${rail}</div>
          </article>`;
        return parts.shell("reports", doc);
      },
    },

    "DOC-10": {
      label: "Theme cards",
      thesis: "The navigable unit is the theme: each becomes a hairline card in a three-up grid headed by its kind tag, with the numbers and decisions collapsed into one footer band beneath.",
      build() {
        const card = (t) => `<section class="r2r-themecard">
            <p class="r2r-themecard-top">${kindTag(t.kind)}</p>
            <h3>${esc(t.title)}</h3>
            <p>${esc(t.body)}</p>
            <p class="r2r-ev"><span class="r2r-ev-k">evidence</span> ${evChips(t.evidence)}</p>
          </section>`;
        const footband = `<div class="r2r-footband">
            ${numBand()}
            <div class="r2r-footband-dec"><p class="ae-h">DECISIONS</p>${decisionsList()}</div>
          </div>`;
        const doc = `<article class="ae-doc">
            <header>
              <div class="r2r-rhead"><p class="ae-plate-cap">FLEET DIGEST · PAST 24H</p>${cacheNote(false)}</div>
              <h1>${esc(S.headline)}</h1>
              <p class="ae-lede">${esc(S.tldr)}</p>
            </header>
            <div class="r2r-themegrid">${S.themes.map(card).join("")}</div>
            ${footband}
            ${provenance(false)}
          </article>`;
        return parts.shell("reports", doc);
      },
    },

    "DOC-11": {
      label: "Timeline-woven",
      thesis: "The narrative is threaded onto the trail spine of the window's real events: each theme is an active node followed by the wire events that prove it, read newest-first down one hairline.",
      build() {
        const W = DATA.wire;
        const node = (t, who, body, active) => `<li class="ae-trail-item${active ? " is-active" : ""}">
            <div class="ae-trail-head"><span class="ae-trail-time">${esc(t)}</span>${who ? `<span class="ae-trail-who">${esc(who)}</span>` : ""}</div>
            <div class="ae-trail-body">${body}</div>
          </li>`;
        const themeNode = (t, time) => node(time, t.kind, `<span class="ae-item">${esc(t.title)}</span> — ${esc(t.body)}`, true);
        const wireNode = (e) => node(e.t, e.agent, `<span class="ae-chip ae-cat-${e.cat}">${esc(e.kind)}</span> ${esc(e.title)}`, false);
        const items = [
          themeNode(S.themes[0], "18:31"),
          wireNode(W[0]), wireNode(W[3]), wireNode(W[7]),
          themeNode(S.themes[2], "17:44"),
          wireNode(W[4]), wireNode(W[6]),
          themeNode(S.themes[1], "17:05"),
          node("17:05", "glass-933-codex", `<span class="ae-chip ae-cat-0">blocked</span> machine out of disk (os error 28) — lane died, work preserved`, false),
          wireNode(W[2]),
        ].join("");
        const doc = `<article class="ae-doc">
            <header>
              <div class="r2r-rhead"><p class="ae-plate-cap">FLEET DIGEST · PAST 24H</p>${cacheNote(false)}</div>
              <h1>${esc(S.headline)}</h1>
              <p class="ae-lede">${esc(S.tldr)}</p>
            </header>
            <ul class="ae-trail r2r-timeline">${items}</ul>
            ${provenance(false)}
          </article>`;
        return parts.shell("reports", doc);
      },
    },

    "DOC-12": {
      label: "Answer-first Q&A",
      thesis: "The document opens as the literal question the query asked — 'What did the fleet do in the past 24h?' — answered in one pull-quote paragraph, with the evidence sections following for anyone who doubts it.",
      build() {
        const answer = `<blockquote class="ae-pull">${esc(S.tldr)}
            <span class="ae-pull-by">synthesized from wire · powder · git · ${GEN}</span>
          </blockquote>`;
        const evidence = S.themes.map((t) => `<section class="r2r-doc-theme">
            <h3>${esc(t.title)}&ensp;${kindTag(t.kind)}</h3>
            <p>${esc(t.body)}</p>
            <p class="r2r-ev"><span class="r2r-ev-k">evidence</span> ${evChips(t.evidence)}</p>
          </section>`).join("");
        const doc = `<article class="ae-doc">
            <header>
              <div class="r2r-rhead"><p class="ae-plate-cap">FLEET · PAST 24H</p>${cacheNote(true)}</div>
              <h1>What did the fleet do in the past 24h?</h1>
            </header>
            ${answer}
            ${numBand()}
            <p class="ae-h r2r-mt">THE EVIDENCE</p>
            ${evidence}
            ${provenance(true)}
          </article>`;
        return parts.shell("reports", doc);
      },
    },

  });
})();
