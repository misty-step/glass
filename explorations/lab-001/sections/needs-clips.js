// Section lane: the Needs You inbox (NEED-1..6) and the Clips queue
// (CLIP-1..6). Baseline is the shipped composition; options 2-6 are
// structurally distinct re-framings, one explicit inversion each. All
// content is real, from DATA. Compose kit primitives + parts helpers only.
(() => {
  const esc = parts.esc;

  // asks oldest->newest by the age string's magnitude (42m < 1h < 3h)
  const asks = DATA.asks;
  const answered = DATA.answered;
  const clips = DATA.clips;

  // shared: the shipped ask meta line
  const askMeta = (a) =>
    `${esc(a.agent)} &middot; powder ${esc(a.card)} &middot; asked ${esc(a.age)} &middot; ${esc(a.blocks)}`;

  Object.assign(window.SPECS, {

    // ---------------------------------------------------------------- NEEDS
    "NEED-1": {
      label: "Baseline — shipped",
      thesis: "A single WAITING-ON-YOU list: each ask a hairline row with a dim meta line and a quiet Answer button, answered work folded away below.",
      build() {
        const rows = asks.map((a) => `<div class="nc-row">
          <span class="nc-row-text">
            <span class="ae-item">${esc(a.title)}</span>
            <span class="nc-meta">${askMeta(a)}</span>
          </span>
          <button type="button" class="ae-button ae-button-quiet">Answer</button>
        </div>`).join("");
        const ans = answered.map((a) => `<div class="nc-row">
          <span class="nc-row-text">
            <span class="ae-item">${esc(a.title)}</span>
            <span class="nc-meta">${esc(a.agent)} &middot; answered ${esc(a.when)}</span>
            <span class="nc-meta">answered: ${esc(a.answer)}</span>
          </span>
        </div>`).join("");
        const desk = `
          <p class="ae-h">WAITING ON YOU &middot; ${asks.length}</p>
          <div class="nc-list">${rows}</div>
          <details class="ae-fold nc-wide">
            <summary><span class="ae-dim">ANSWERED</span><span class="ae-dim">${answered.length} from API</span></summary>
            <div class="nc-list">${ans}</div>
          </details>`;
        return parts.shell("needs", desk);
      },
    },

    "NEED-2": {
      label: "One ask at a time",
      thesis: "INVERTS the list: one ask fills the surface with its full body and an inline answer, a 1-of-3 pager the only nod to the queue — depth over breadth.",
      build() {
        const a = asks[0];
        const pager = asks.map((_, i) =>
          `<button role="tab" type="button"${i === 0 ? ' aria-selected="true"' : ""}>${i + 1}</button>`).join("");
        const desk = `
          <div class="nc-single">
            <div class="ae-tabs" role="tablist" aria-label="Open asks">${pager}</div>
            <p class="ae-h">ASK 1 OF ${asks.length} &middot; ${esc(a.age)} OLD</p>
            <p class="ae-strong">${esc(a.title)}</p>
            <p class="nc-meta">${esc(a.agent)} &middot; powder ${esc(a.card)}</p>
            <p>${esc(a.body)}</p>
            <p class="ae-status">${parts.icon("warn")}<span class="ae-status-label">blocks ${esc(a.blocks)}</span></p>
            <div class="nc-form">
              <label class="ae-label" for="need2-a">Your answer</label>
              <textarea id="need2-a" class="ae-input" rows="4" placeholder="Type your answer&hellip;"></textarea>
              <div class="nc-actions">
                <button type="button" class="ae-button">Answer &amp; relay</button>
                <a href="#0">Open card</a>
              </div>
            </div>
          </div>`;
        return parts.shell("needs", desk);
      },
    },

    "NEED-3": {
      label: "Consequences board",
      thesis: "Leads with stakes: a three-column matrix — what it blocks / the ask / your options — each ask a row across, so the cost of not answering reads first.",
      build() {
        const head = `
          <div class="nc-consq-lead"><span class="ae-h">WHAT IT BLOCKS</span></div>
          <div><span class="ae-h">THE ASK</span></div>
          <div><span class="ae-h">YOUR OPTIONS</span></div>`;
        const rows = asks.map((a) => `
          <div class="nc-consq-lead">
            <span class="ae-status">${parts.icon("warn")}<span class="ae-status-label">${esc(a.blocks)}</span></span>
          </div>
          <div>
            <span class="ae-item">${esc(a.title)}</span>
            <span class="nc-meta">${esc(a.agent)} &middot; ${esc(a.card)} &middot; ${esc(a.age)}</span>
          </div>
          <div class="nc-actions">
            <button type="button" class="ae-button ae-button-compact">Answer</button>
            <a href="#0">Open card</a>
          </div>`).join("");
        const desk = `
          <p class="ae-h">WAITING ON YOU &middot; ${asks.length}</p>
          <div class="nc-consq">${head}${rows}</div>`;
        return parts.shell("needs", desk);
      },
    },

    "NEED-4": {
      label: "Age-escalated ledger",
      thesis: "Reorders by time, oldest-first, and makes the age the loudest element — a mono ledger where staleness, not the ask text, sets the hierarchy.",
      build() {
        const oldestFirst = [...asks].reverse(); // DATA is newest-first (42m,1h,3h)
        const rows = oldestFirst.map((a) => `
          <div class="nc-ledger-row">
            <span class="ae-num ae-strong">${esc(a.age)}</span>
            <span class="nc-row-text">
              <span class="ae-item">${esc(a.title)}</span>
              <span class="nc-meta">${esc(a.agent)} &middot; powder ${esc(a.card)} &middot; blocks ${esc(a.blocks)}</span>
            </span>
          </div>`).join("");
        const desk = `
          <p class="ae-h">OLDEST FIRST &middot; ${asks.length} WAITING</p>
          <div class="nc-wide">${rows}</div>`;
        return parts.shell("needs", desk);
      },
    },

    "NEED-5": {
      label: "Inline-answer rows",
      thesis: "INVERTS the dialog: every ask carries its own answer field and send inline — no modal at all — with answered asks settling into a trail beneath.",
      build() {
        const rows = asks.map((a, i) => `
          <div class="nc-row nc-row-top">
            <span class="nc-row-text">
              <span class="ae-item">${esc(a.title)}</span>
              <span class="nc-meta">${askMeta(a)}</span>
              <div class="nc-form">
                <label class="ae-label" for="need5-${i}">Answer</label>
                <textarea id="need5-${i}" class="ae-input" rows="2" placeholder="Type your answer&hellip;"></textarea>
                <div class="nc-actions"><button type="button" class="ae-button ae-button-compact">Relay to Powder</button></div>
              </div>
            </span>
          </div>`).join("");
        const trail = answered.map((a) => `
          <li class="ae-trail-item">
            <div class="ae-trail-head">
              <span class="ae-trail-time">${esc(a.when)}</span>
              <span class="ae-trail-who">${esc(a.agent)}</span>
            </div>
            <div class="ae-trail-body"><span class="ae-item">${esc(a.title)}</span><br><span class="ae-dim">${esc(a.answer)}</span></div>
          </li>`).join("");
        const desk = `
          <p class="ae-h">WAITING ON YOU &middot; ${asks.length}</p>
          <div class="nc-list">${rows}</div>
          <p class="ae-h nc-mt">ANSWERED</p>
          <ul class="ae-trail nc-wide">${trail}</ul>`;
        return parts.shell("needs", desk);
      },
    },

    "NEED-6": {
      label: "Context-rich cards",
      thesis: "Each ask a framed card carrying its full body, the blocker as a status line, and Answer beside Open-card — trades the dense list for readable, self-contained context.",
      build() {
        const cards = asks.map((a) => `
          <article class="nc-card">
            <span class="ae-status">${parts.icon("warn")}<span class="ae-status-label">blocks ${esc(a.blocks)}</span></span>
            <span class="ae-item">${esc(a.title)}</span>
            <span class="nc-meta">${esc(a.agent)} &middot; powder ${esc(a.card)} &middot; asked ${esc(a.age)}</span>
            <p class="nc-figcap">${esc(a.body)}</p>
            <div class="nc-actions">
              <button type="button" class="ae-button ae-button-compact">Answer</button>
              <a href="#0">Open card</a>
            </div>
          </article>`).join("");
        const desk = `
          <p class="ae-h">WAITING ON YOU &middot; ${asks.length}</p>
          <div class="nc-cards">${cards}</div>`;
        return parts.shell("needs", desk);
      },
    },

    // ---------------------------------------------------------------- CLIPS
    "CLIP-1": {
      label: "Baseline — shipped",
      thesis: "A hero with a queued-clips count over a review table: created / draft caption / surface / evidence / note, one row per clip.",
      build() {
        const rows = clips.map((c) => `<tr>
          <td data-label="Created" class="num">${esc(c.when)}</td>
          <td data-label="Draft caption" class="ae-item">${esc(c.caption)}</td>
          <td data-label="Surface">${esc(c.session)}</td>
          <td data-label="Evidence"><a href="#0">post</a></td>
          <td data-label="Note">${esc(c.title)}</td>
        </tr>`).join("");
        const desk = `
          <p class="ae-h">CLIPS</p>
          <p class="ae-strong">Clip review queue</p>
          <p class="ae-dim">Marked live-stage moments, packaged with post context and draft captions.</p>
          ${parts.statBand([[clips.length, "Queued clips"]])}
          <figure class="ae-plate nc-mt">
            <figcaption class="ae-plate-cap">REVIEW CANDIDATES</figcaption>
            <table class="ae-table">
              <thead><tr><th class="num">Created</th><th>Draft caption</th><th>Surface</th><th>Evidence</th><th>Note</th></tr></thead>
              <tbody>${rows}</tbody>
            </table>
          </figure>`;
        return parts.shell("clips", desk);
      },
    },

    "CLIP-2": {
      label: "Contact sheet",
      thesis: "Imports the darkroom light-table: a grid of framed slides, each a mono timestamp over the moment's title and its caption as a figcaption.",
      build() {
        const slides = clips.map((c) => `
          <figure class="nc-card">
            <span class="ae-tag">${esc(c.when)}</span>
            <span class="ae-item">${esc(c.title)}</span>
            <figcaption class="nc-figcap">${esc(c.caption)}</figcaption>
            <span class="nc-meta">${esc(c.agent)} &middot; ${esc(c.session)}</span>
            <div class="nc-actions"><a href="#0">Open post</a></div>
          </figure>`).join("");
        const desk = `
          <p class="ae-h">CLIPS &middot; CONTACT SHEET &middot; ${clips.length}</p>
          <div class="nc-cards">${slides}</div>`;
        return parts.shell("clips", desk);
      },
    },

    "CLIP-3": {
      label: "Review bench",
      thesis: "REVERSES the hierarchy to detail-first: the top clip fills a workbench with an editable caption and a Keep/Drop pair, the rest a thin strip below.",
      build() {
        const lead = clips[0];
        const rest = clips.slice(1).map((c) => `
          <div class="nc-row">
            <span class="nc-row-text">
              <span class="ae-item">${esc(c.title)}</span>
              <span class="nc-meta">${esc(c.when)} &middot; ${esc(c.agent)} &middot; ${esc(c.caption)}</span>
            </span>
            <a href="#0">Review</a>
          </div>`).join("");
        const desk = `
          <p class="ae-h">REVIEWING 1 OF ${clips.length}</p>
          <div class="nc-single">
            <p class="ae-strong">${esc(lead.title)}</p>
            <p class="nc-meta">${esc(lead.when)} &middot; ${esc(lead.agent)} &middot; ${esc(lead.session)}</p>
            <div class="nc-form">
              <label class="ae-label" for="clip3-cap">Draft caption</label>
              <textarea id="clip3-cap" class="ae-input" rows="2">${esc(lead.caption)}</textarea>
              <div class="nc-actions">
                <button type="button" class="ae-button">Keep</button>
                <button type="button" class="ae-button ae-button-quiet">Drop</button>
                <a href="#0">Open post</a>
              </div>
            </div>
          </div>
          <p class="ae-h nc-mt">NEXT IN QUEUE</p>
          <div class="nc-list">${rest}</div>`;
        return parts.shell("clips", desk);
      },
    },

    "CLIP-4": {
      label: "Capture trail",
      thesis: "Substitutes the table for time: clips as a chronological trail of capture moments down the day's spine, each with its actor and caption.",
      build() {
        const items = clips.map((c) => `
          <li class="ae-trail-item">
            <div class="ae-trail-head">
              <span class="ae-trail-time">${esc(c.when)}</span>
              <span class="ae-trail-who">${esc(c.agent)}</span>
            </div>
            <div class="ae-trail-body"><span class="ae-item">${esc(c.title)}</span><br><span class="ae-dim">${esc(c.caption)}</span></div>
          </li>`).join("");
        const desk = `
          <p class="ae-h">CLIPS CAPTURED TODAY &middot; ${clips.length}</p>
          <ul class="ae-trail nc-wide">${items}</ul>`;
        return parts.shell("clips", desk);
      },
    },

    "CLIP-5": {
      label: "Archival ledger",
      thesis: "The densest cut: a bare numbered plate with when / agent / session / caption in mono, no hero — the queue as an archival instrument.",
      build() {
        const rows = clips.map((c) => `<tr>
          <td data-label="When" class="num">${esc(c.when)}</td>
          <td data-label="Agent">${esc(c.agent)}</td>
          <td data-label="Session">${esc(c.session)}</td>
          <td data-label="Caption" class="ae-item">${esc(c.caption)}</td>
        </tr>`).join("");
        const desk = `
          <p class="ae-h">CLIP LEDGER &middot; ${clips.length} CAPTURED</p>
          <figure class="ae-plate">
            <figcaption class="ae-plate-cap">FIG. 1 — CLIPS, NEWEST FIRST</figcaption>
            <table class="ae-table">
              <thead><tr><th class="num">When</th><th>Agent</th><th>Session</th><th>Caption</th></tr></thead>
              <tbody>${rows}</tbody>
            </table>
            <p class="ae-plate-note">One-way queue &middot; captured via MCP capture_clip or POST /api/clips.</p>
          </figure>`;
        return parts.shell("clips", desk);
      },
    },

    "CLIP-6": {
      label: "Clips are wire rows",
      thesis: "INVERTS the bespoke surface: clips are just another event kind, rendered through the shared wire row with a 'clip' category chip — zero clip-specific UI.",
      build() {
        const asEvents = clips.map((c) => ({ t: c.when, agent: c.agent, kind: "clip", cat: 7, title: c.title }));
        const rows = asEvents.map((e) => parts.wireRow(e)).join("");
        const desk = `
          <p class="ae-h">ACTIVITY &middot; CLIPS ONLY</p>
          <div class="ae-list-rows">${rows}</div>`;
        return parts.shell("clips", desk);
      },
    },

  });
})();
