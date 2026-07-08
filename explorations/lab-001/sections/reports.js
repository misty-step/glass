// Reports section — /reports (REP-*) and the report document (DOC-*).
// Structurally distinct option-builder layouts, kit primitives only, real
// DATA content. REP-1 and DOC-1 are the shipped baselines.
(() => {
  const esc = parts.esc;
  const R = DATA.reports;
  const r001 = DATA.r001;

  // ── shared: the library table (baseline columns) ──────────────────────
  const libHead = "<th>ID</th><th>REPORT</th><th>WINDOW</th><th>SCOPE</th><th>GENERATED</th>";
  const libRows = (list) => list.map((r) => `<tr>
      <td data-label="ID"><span class="ae-item">${esc(r.id)}</span></td>
      <td data-label="REPORT"><a href="#0">${esc(r.title)}</a></td>
      <td data-label="WINDOW">${esc(r.window)}</td>
      <td data-label="SCOPE">${esc(r.scope)}</td>
      <td data-label="GENERATED">${esc(r.by)}</td>
    </tr>`).join("");
  const plate = (cap, head, rows, note) => `<section class="ae-plate">
      <p class="ae-plate-cap">${esc(cap)}</p>
      <div class="rep-scroll"><table class="ae-table"><thead><tr>${head}</tr></thead><tbody>${rows}</tbody></table></div>
      ${note ? `<p class="ae-plate-note">${esc(note)}</p>` : ""}
    </section>`;
  const libPlate = (cap, list) => plate(cap, libHead, libRows(list));

  // ── shared: the shipped generator, rebuilt in kit primitives ──────────
  const seg = (label, on) =>
    `<button type="button" class="ae-button${on ? "" : "-quiet"} ae-button-compact">${esc(label)}</button>`;
  const genRow = (h, buttons, tail) =>
    `<div class="rep-gen-row"><span class="ae-h">${esc(h)}</span><span class="rep-chips">${buttons}</span>${tail || ""}</div>`;
  const tag = (s) => `<span class="ae-tag">${esc(s)}</span>`;
  const LAST_WEEK = "2026-06-29 → 07-06";

  const generator = () => `<section class="rep-gen">
      <p class="ae-plate-cap">GENERATE A REPORT</p>
      ${genRow("WINDOW", [seg("Today"), seg("Yesterday"), seg("This week"), seg("Last week", true), seg("Custom")].join(""), tag(LAST_WEEK))}
      ${genRow("SCOPE", [seg("Whole fleet", true), seg("One agent"), seg("One repo")].join(""))}
      ${genRow("KIND", [seg("Activity digest", true), seg("Backlog"), seg("Review index"), seg("Fleet digest")].join(""))}
      <span><button type="button" class="ae-button">Generate report</button></span>
    </section>`;

  // ── shared DOC helpers ────────────────────────────────────────────────
  const compHead = "<th>CARD</th><th>COMPLETION</th><th>REPO</th><th>PRI</th><th>AT</th>";
  const compRows = (list) => list.map((c) => `<tr>
      <td data-label="CARD"><a href="#0" class="ae-item">${esc(c.card)}</a></td>
      <td data-label="COMPLETION">${esc(c.title)}</td>
      <td data-label="REPO">${esc(c.repo)}</td>
      <td data-label="PRI">${esc(c.pri)}</td>
      <td data-label="AT">${esc(c.at)}</td>
    </tr>`).join("");
  const MORE = "+ 50 more completions in the full ledger · showing the 8 most recent";
  const docMeta = () =>
    `<div class="rep-chips">${tag("JUL 7 – JUL 8")}${tag("FLEET")}${tag(esc(r001.generated))}</div>`;
  const ledeSentence = `Fleet activity, ${r001.window}: <span class="ae-num ae-strong">58</span> completed cards, 0 posts, 0 clips, 0 blocked.`;

  const maxRepo = Math.max.apply(null, r001.byRepo.map((x) => x[1]));
  const distRows = () => r001.byRepo.map(([repo, n]) => `<div class="rep-dist-row">
      <span class="ae-num">${esc(repo)}</span>
      <span class="ae-meter"><span class="ae-meter-fill" style="width:${Math.round((n / maxRepo) * 100)}%"></span></span>
      <span class="ae-num ae-strong">${n}</span>
    </div>`).join("");

  Object.assign(window.SPECS, {

    // ══ REP: the /reports page ═════════════════════════════════════════
    "REP-1": {
      label: "Baseline — shipped",
      thesis: "A hairline generator (window/scope/kind option rows + Generate) sits above a numbered library plate of every report, newest first.",
      build() {
        return parts.shell("reports", `<div class="rep-stack">
          ${generator()}
          ${libPlate("PLATE 1 · THE LIBRARY · EVERY GENERATED REPORT, NEWEST FIRST", R)}
        </div>`);
      },
    },

    "REP-2": {
      label: "Library-first",
      thesis: "Inverts the shipped premise that generation is primary: the library table IS the page, and generation collapses to one quiet folded 'New report…' row you rarely open.",
      build() {
        const compactGen = `<div class="rep-gen">
            ${genRow("WINDOW", [seg("Last week", true), seg("This week"), seg("Custom")].join(""), tag(LAST_WEEK))}
            ${genRow("SCOPE", [seg("Whole fleet", true), seg("One agent"), seg("One repo")].join(""))}
            ${genRow("KIND", [seg("Activity digest", true), seg("Backlog"), seg("Review index")].join(""))}
            <span><button type="button" class="ae-button">Generate report</button></span>
          </div>`;
        return parts.shell("reports", `<div class="rep-stack">
          ${libPlate("THE LIBRARY · 4 REPORTS · NEWEST FIRST", R)}
          <details class="ae-fold">
            <summary>New report…</summary>
            ${compactGen}
          </details>
        </div>`);
      },
    },

    "REP-3": {
      label: "Sentence-builder",
      thesis: "The generator reads as one plain-English sentence whose nouns are editable tokens — 'Show me [activity] for [the whole fleet] over [last week]' — with the library demoted beneath.",
      build() {
        const slot = (s) => `<button type="button" class="rep-slot">${esc(s)}</button>`;
        const sentence = `<section class="rep-gen">
            <p class="ae-plate-cap">GENERATE A REPORT</p>
            <p>Show me ${slot("activity")} for ${slot("the whole fleet")} over ${slot("last week")} ${tag(LAST_WEEK)}</p>
            <span><button type="button" class="ae-button">Generate report</button></span>
          </section>`;
        return parts.shell("reports", `<div class="rep-stack">
          ${sentence}
          ${libPlate("THE LIBRARY · NEWEST FIRST", R)}
        </div>`);
      },
    },

    "REP-4": {
      label: "Calendar-spine",
      thesis: "Reorganizes reports by time instead of by recency: a hairline day spine down the page, each report pinned to the window it covers, standing digests showing the daily rhythm.",
      build() {
        const day = (date, who, active, body) => `<li class="ae-trail-item${active ? " is-active" : ""}">
            <div class="ae-trail-head"><span class="ae-trail-time">${esc(date)}</span><span class="ae-trail-who">${esc(who)}</span></div>
            <div class="ae-trail-body">${body}</div>
          </li>`;
        const link = (r) => `<a href="#0" class="ae-item">${esc(r.id)}</a> ${esc(r.title)}`;
        const spine = `<ul class="ae-trail">
            ${day("MON JUL 6", "standing", false, `<span class="ae-dim">weekly digest window opens</span> — ${link(R[0])} <span class="ae-tag">auto · Mon 06:00</span>`)}
            ${day("TUE JUL 7", "", false, `${link(R[3])} <span class="ae-tag">phaedrus · 18:28</span><br><span class="ae-dim">covers Jul 7 – Jul 8</span>`)}
            ${day("WED JUL 8", "today", true, `${link(R[1])} <span class="ae-tag">auto · 06:00</span><br>${link(R[2])} <span class="ae-tag">phaedrus · 18:40</span>`)}
          </ul>`;
        return parts.shell("reports", `<div class="rep-stack">
          <div class="rep-gen-row"><span class="ae-h">THIS WEEK · 2026-W28</span><button type="button" class="ae-button-quiet ae-button-compact">New report…</button></div>
          ${spine}
        </div>`);
      },
    },

    "REP-5": {
      label: "Split-browse tabs",
      thesis: "The navigable unit becomes the report kind: kind tabs across the top, each its own filtered plate with a per-kind generator, so you browse a type before you generate one.",
      build() {
        const tabs = `<div class="ae-tabs" role="tablist">
            <button type="button" role="tab" aria-selected="true">Activity</button>
            <button type="button" role="tab" aria-selected="false">Backlog</button>
            <button type="button" role="tab" aria-selected="false">Review</button>
            <button type="button" role="tab" aria-selected="false">Fleet</button>
          </div>`;
        const activity = R.filter((r) => r.kind === "activity-digest");
        const perKindGen = `<div class="rep-gen-row">
            <span class="ae-h">NEW ACTIVITY DIGEST</span>
            <span class="rep-chips">${seg("Last week", true)}${seg("This week")}${seg("Custom")}</span>
            <span class="rep-chips">${seg("Fleet", true)}${seg("Agent")}${seg("Repo")}</span>
            <button type="button" class="ae-button ae-button-compact">Generate</button>
          </div>`;
        return parts.shell("reports", `<div class="rep-stack">
          ${tabs}
          ${perKindGen}
          ${libPlate("ACTIVITY DIGESTS · 3", activity)}
        </div>`);
      },
    },

    "REP-6": {
      label: "Command-ledger",
      thesis: "A trading-terminal cut: the generator is one resolved mono command line, the library a dense blotter of every run beneath it — the operator's densest possible reading.",
      build() {
        const cmd = `<section class="rep-gen">
            <p class="ae-plate-cap">COMMAND</p>
            <div class="rep-cmd">
              <span class="rep-cmd-prompt">glass&nbsp;▸</span>
              <span class="rep-cmd-body">report activity · fleet · ${esc(LAST_WEEK)}</span>
              <button type="button" class="ae-button ae-button-compact">Run</button>
            </div>
          </section>`;
        const blotter = `<section>
            <p class="ae-plate-cap">BLOTTER · 4 RUNS · NEWEST FIRST</p>
            <div class="lab-rows">
              ${R.map((r) => `<div class="rep-gen-row"><span class="ae-num ae-item">${esc(r.id)}</span><span class="ae-tag">${esc(r.kind)}</span><span class="ae-num">${esc(r.window)}</span><span class="ae-dim">${esc(r.scope)}</span><a href="#0">${esc(r.title)}</a><span class="ae-num ae-dim">${esc(r.by)}</span></div>`).join("")}
            </div>
          </section>`;
        return parts.shell("reports", `<div class="rep-stack">${cmd}${blotter}</div>`);
      },
    },

    // ══ DOC: the report document (/reports/R-001) ══════════════════════
    "DOC-1": {
      label: "Baseline — shipped",
      thesis: "The shipped doc: an ID/title header with window·scope·generated metadata, a lede totals sentence, then the completions table.",
      build() {
        const doc = `<article class="ae-doc">
            <header>
              <p class="ae-plate-cap">R-001</p>
              <h1>${esc(r001.title)}</h1>
              ${docMeta()}
              <p class="ae-lede">${ledeSentence}</p>
            </header>
            ${plate("PLATE 1 · POWDER COMPLETIONS", compHead, compRows(r001.completions), MORE)}
          </article>`;
        return parts.shell("reports", doc);
      },
    },

    "DOC-2": {
      label: "Executive lede-first",
      thesis: "Conclusion up front: a findings box with the four headline numbers and an accent takeaway, every completion folded away into disclosure sections for the reader who wants the detail.",
      build() {
        const findings = `<section class="ae-findings">
            <p class="ae-findings-title">FINDINGS · ${esc(r001.window)}</p>
            <div class="rep-dist">
              <div class="rep-dist-row"><span>Completed cards</span><span></span><span class="ae-num ae-strong">58</span></div>
              <div class="rep-dist-row"><span>Glass posts</span><span></span><span class="ae-num">0</span></div>
              <div class="rep-dist-row"><span>Clips</span><span></span><span class="ae-num">0</span></div>
              <div class="rep-dist-row"><span>Blocked events</span><span></span><span class="ae-num">0</span></div>
            </div>
            <p class="ae-rec">A clean sweep: 58 cards closed across 8 repos with nothing left blocked. Glass (14) and Canary (9) carried the day.</p>
          </section>`;
        const foldTable = `<details class="ae-fold">
            <summary>All 58 completions</summary>
            <div class="rep-scroll"><table class="ae-table"><thead><tr>${compHead}</tr></thead><tbody>${compRows(r001.completions)}</tbody></table></div>
            <p class="ae-plate-note">${esc(MORE)}</p>
          </details>
          <details class="ae-fold"><summary>By repo</summary><div class="rep-dist">${distRows()}</div></details>`;
        const doc = `<article class="ae-doc">
            <header><p class="ae-plate-cap">R-001</p><h1>${esc(r001.title)}</h1>${docMeta()}</header>
            ${findings}
            ${foldTable}
          </article>`;
        return parts.shell("reports", doc);
      },
    },

    "DOC-3": {
      label: "Per-repo chapters",
      thesis: "Reorganizes the digest by repo instead of by time: each repo is a chapter headed by a distribution meter, its own completions listed beneath, the long tail summarized as a count.",
      build() {
        const samples = {};
        r001.completions.forEach((c) => { (samples[c.repo] || (samples[c.repo] = [])).push(c); });
        const chapter = ([repo, n]) => {
          const rows = samples[repo] || [];
          const shown = rows.map((c) => `<div><a href="#0" class="ae-item">${esc(c.card)}</a> ${esc(c.title)} <span class="ae-tag">${esc(c.pri)} · ${esc(c.at)}</span></div>`).join("");
          const rest = n - rows.length;
          const body = rows.length
            ? `<div class="lab-rows">${shown}</div>${rest > 0 ? `<p class="ae-plate-note">+ ${rest} more in ${esc(repo)}</p>` : ""}`
            : `<p class="ae-plate-note">${n} completions — none in the recent sample</p>`;
          return `<section>
              <div class="rep-dist-row"><span class="ae-h" style="margin:0">${esc(repo).toUpperCase()}</span><span class="ae-meter"><span class="ae-meter-fill" style="width:${Math.round((n / maxRepo) * 100)}%"></span></span><span class="ae-num ae-strong">${n}</span></div>
              ${body}
            </section>`;
        };
        const doc = `<article class="ae-doc">
            <header><p class="ae-plate-cap">R-001</p><h1>${esc(r001.title)}</h1>${docMeta()}<p class="ae-lede">${ledeSentence}</p></header>
            <div class="rep-stack">${r001.byRepo.map(chapter).join("")}</div>
          </article>`;
        return parts.shell("reports", doc);
      },
    },

    "DOC-4": {
      label: "Ledger-pure",
      thesis: "Inverts the report-as-narrative: no lede, no findings, no prose — one continuous mono ledger of every completion in tabular numerals, the auditor's cut at maximum density.",
      build() {
        const doc = `<article class="ae-doc">
            <p class="ae-plate-cap">R-001 · ACTIVITY LEDGER · FLEET · 2026-07-07 → 07-08 · 58 ROWS</p>
            <div class="rep-scroll"><table class="ae-table"><thead><tr>${compHead}</tr></thead><tbody>${compRows(r001.completions)}
              <tr><td data-label="CARD" colspan="5"><span class="ae-dim">+ 50 more rows · 2026-07-07 23:22 → 06-… — full ledger</span></td></tr>
            </tbody></table></div>
          </article>`;
        return parts.shell("reports", doc);
      },
    },

    "DOC-5": {
      label: "Evidence-forward",
      thesis: "Reverses the reading order of the row: every completion leads with its evidence link and card tag, the narrative demoted to a numbered figure caption under the table.",
      build() {
        const rows = r001.completions.map((c) => `<tr>
            <td data-label="EVIDENCE"><a href="#0" class="ae-item">powder ↗</a></td>
            <td data-label="CARD"><span class="ae-tag">${esc(c.card)}</span></td>
            <td data-label="COMPLETION">${esc(c.title)}</td>
            <td data-label="REPO">${esc(c.repo)}</td>
          </tr>`).join("");
        const figure = `<figure>
            <div class="rep-scroll"><table class="ae-table"><thead><tr><th>EVIDENCE</th><th>CARD</th><th>COMPLETION</th><th>REPO</th></tr></thead><tbody>${rows}</tbody></table></div>
            <figcaption>${esc(r001.window)} — 58 cards closed across 8 repos, none blocked. Showing the 8 most recent; the rest carry the same evidence links.</figcaption>
          </figure>`;
        const doc = `<article class="ae-doc">
            <header><p class="ae-plate-cap">R-001</p><h1>${esc(r001.title)}</h1>${docMeta()}</header>
            ${figure}
          </article>`;
        return parts.shell("reports", doc);
      },
    },

    "DOC-6": {
      label: "Dashboard-doc",
      thesis: "Instruments first: a stat band and a per-repo meter panel read the whole digest at a glance, with the completions table folded below for anyone who drills in.",
      build() {
        const band = parts.statBand([
          ["58", "completed"],
          ["8", "repos active"],
          ["14", "top · glass"],
          ["0", "blocked"],
        ]);
        const distPanel = `<section class="ae-plate">
            <p class="ae-plate-cap">FIG 1 · COMPLETIONS BY REPO</p>
            <div class="rep-dist">${distRows()}</div>
          </section>`;
        const foldTable = `<details class="ae-fold">
            <summary>Completions ledger</summary>
            <div class="rep-scroll"><table class="ae-table"><thead><tr>${compHead}</tr></thead><tbody>${compRows(r001.completions)}</tbody></table></div>
            <p class="ae-plate-note">${esc(MORE)}</p>
          </details>`;
        const doc = `<article class="ae-doc">
            <header><p class="ae-plate-cap">R-001</p><h1>${esc(r001.title)}</h1>${docMeta()}</header>
            <div class="rep-stack">
              ${band}
              ${distPanel}
              ${foldTable}
            </div>
          </article>`;
        return parts.shell("reports", doc);
      },
    },

  });
})();
