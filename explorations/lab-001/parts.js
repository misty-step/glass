window.SPECS = window.SPECS || {};
// Shared parts library. Options compose these; they never re-invent kit
// primitives. Everything returns an HTML string; token-pure kit classes only.
const parts = (() => {
  const esc = (s) => String(s ?? "").replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" }[c]));

  const ICON_ATTRS = 'class="ae-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"';
  const ICONS = {
    glass: `<svg ${ICON_ATTRS}><path d="M11 6 8 9"></path><path d="m16 7-8 8"></path><rect x="4" y="2" width="16" height="20"></rect></svg>`,
    home: `<svg ${ICON_ATTRS}><path d="M15 21v-8a1 1 0 0 0-1-1h-4a1 1 0 0 0-1 1v8"></path><path d="M3 10a2 2 0 0 1 .709-1.528l7-6a2 2 0 0 1 2.582 0l7 6A2 2 0 0 1 21 10v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"></path></svg>`,
    sun: `<svg class="ae-icon ae-sun" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><circle cx="12" cy="12" r="4"></circle><path d="M12 2v2"></path><path d="M12 20v2"></path><path d="m4.93 4.93 1.41 1.41"></path><path d="m17.66 17.66 1.41 1.41"></path><path d="M2 12h2"></path><path d="M20 12h2"></path><path d="m6.34 17.66-1.41 1.41"></path><path d="m19.07 4.93-1.41 1.41"></path></svg>`,
    moon: `<svg class="ae-icon ae-moon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M12 3a6 6 0 0 0 9 9 9 9 0 1 1-9-9Z"></path></svg>`,
    ok: `<svg class="ae-icon ae-ok" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><circle cx="12" cy="12" r="10"></circle><path d="m9 12 2 2 4-4"></path></svg>`,
    warn: `<svg class="ae-icon ae-warn" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="m21.73 18-8-14a2 2 0 0 0-3.48 0l-8 14A2 2 0 0 0 4 20h16a2 2 0 0 0 1.73-2Z"></path><path d="M12 9v4"></path><path d="M12 17h.01"></path></svg>`,
    dash: `<svg ${ICON_ATTRS}><path d="M10.1 2.18a9.93 9.93 0 0 1 3.8 0"></path><path d="M17.6 3.71a9.95 9.95 0 0 1 2.69 2.7"></path><path d="M21.82 10.1a9.93 9.93 0 0 1 0 3.8"></path><path d="M20.29 17.6a9.95 9.95 0 0 1-2.7 2.69"></path><path d="M13.9 21.82a9.94 9.94 0 0 1-3.8 0"></path><path d="M6.4 20.29a9.95 9.95 0 0 1-2.69-2.7"></path><path d="M2.18 13.9a9.93 9.93 0 0 1 0-3.8"></path><path d="M3.71 6.4a9.95 9.95 0 0 1 2.7-2.69"></path></svg>`,
    tick: `<svg ${ICON_ATTRS}><path d="M20 6 9 17l-5-5"></path></svg>`,
  };
  const icon = (n) => ICONS[n] || "";

  // status glyph for an agent state (status rides the glyph — kit law)
  const stateIcon = (state) => state === "blocked" ? icon("warn") : state === "quiet" ? icon("dash") : icon("ok");

  // the shipped rail (baseline). opts: {active, compact}
  function rail(active, opts = {}) {
    const link = (href, label, key) =>
      `<a href="#0" ${active === key ? 'aria-current="page"' : ""}>${label}</a>`;
    return `<aside class="ae-rail">
      <div class="ae-logo"><span class="ae-app-mark">${icon("glass")}</span><span class="ae-name">Glass</span></div>
      <p class="ae-h">PLACES</p>
      <nav>
        ${link("/", "Now", "now")}
        ${link("/needs-you", "Needs you&ensp;· " + DATA.stats.needYou, "needs")}
        ${link("/reports", "Reports", "reports")}
        ${link("/clips", "Clips", "clips")}
      </nav>
      <div class="ae-rail-foot">
        <a href="#0">${icon("home")} Sanctum</a>
        <a href="#0">Wire an agent</a>
        <button class="ae-mode" type="button" aria-label="Theme">${icon("sun")}${icon("moon")}</button>
      </div>
    </aside>`;
  }

  // wrap a desk body in the shipped shell
  function shell(active, deskHtml) {
    return `<div class="ae-shell">${rail(active)}<main class="ae-desk">${deskHtml}</main></div>`;
  }

  function statBand(items) {
    return `<div class="ae-stat-badges">` + items.map(([v, l, warn]) =>
      `<span class="ae-stat-badge">${warn ? icon("warn") : ""}<span class="ae-stat-value">${esc(v)}</span><span class="ae-stat-label">${esc(l)}</span></span>`).join("") + `</div>`;
  }

  // baseline wall card
  function wallCard(a) {
    const quiet = a.state === "quiet";
    const trace = (a.trace || []).map((v) => v ? icon("tick") : icon("warn")).join("");
    return `<a class="ae-wall-card${quiet ? " mk-quiet" : ""}" href="#0">
      <span>
        <span class="ae-wall-head">${stateIcon(a.state)}<span class="ae-item">${esc(a.name)}</span><span class="ae-tag">powder ${esc(a.card)}</span></span>
        <span class="ae-wall-meta">${esc(a.act)}</span>
      </span>
      <span class="ae-wall-figure"><span class="ae-wall-time">${esc(a.age)}</span>${trace ? `<span class="ae-wall-trace">${trace}</span>` : ""}</span>
    </a>`;
  }

  // baseline wire row
  function wireRow(e) {
    return `<a class="ae-list-row" href="#0">
      <span class="ae-list-cell ae-list-time"><span class="ae-list-value">${esc(e.t)}</span></span>
      <span class="ae-list-cell"><span class="ae-list-label">AGENT</span><span class="ae-list-value">${esc(e.agent)}</span></span>
      <span class="ae-list-cell"><span class="ae-list-label">KIND</span><span class="ae-list-value"><span class="ae-chip ae-cat-${e.cat}">${esc(e.kind)}</span></span></span>
      <span class="ae-list-cell"><span class="ae-list-label">EVENT</span><span class="ae-list-value">${esc(e.title)}</span></span>
    </a>`;
  }

  return { esc, icon, stateIcon, rail, shell, statBand, wallCard, wireRow };
})();
