// SHELL-7..11 — round 2. The left rail WON as a direction; these five
// aggressively translate shadcn's sidebar STRUCTURE (per-place icons, labeled
// groups, considered spacing, active indicator, a foot account block, a phone
// slide-over sheet) into kit vocabulary — hairlines, radius 0, ink registers,
// 13px chrome. Never shadcn's rounded/shadowed STYLE. Every desk reuses the
// same NOW composition (statBand + wall + wire) so chrome is judged against
// identical content. All content is DATA.
(function () {
  const P = parts, esc = P.esc, icon = P.icon;
  const S = DATA.stats;

  // ── Lucide place glyphs (inline, .ae-icon; kit's own attrs) ─────────
  const ATTRS = 'class="ae-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"';
  const LU = {
    activity: '<path d="M22 12h-2.48a2 2 0 0 0-1.93 1.46l-2.35 8.36a.25.25 0 0 1-.48 0L9.24 2.18a.25.25 0 0 0-.48 0l-2.35 8.36A2 2 0 0 1 4.49 12H2"/>',
    inbox: '<path d="M22 12h-6l-2 3h-4l-2-3H2"/><path d="M5.45 5.11 2 12v6a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2v-6l-3.45-6.89A2 2 0 0 0 16.76 4H7.24a2 2 0 0 0-1.79 1.11z"/>',
    filetext: '<path d="M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z"/><path d="M14 2v4a2 2 0 0 0 2 2h4"/><path d="M10 9H8"/><path d="M16 13H8"/><path d="M16 17H8"/>',
    plug: '<path d="M12 22v-5"/><path d="M9 8V2"/><path d="M15 8V2"/><path d="M18 8v5a4 4 0 0 1-4 4h-4a4 4 0 0 1-4-4V8Z"/>',
    folder: '<path d="M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z"/>',
    user: '<circle cx="12" cy="12" r="10"/><circle cx="12" cy="10" r="3"/><path d="M7 20.662V19a2 2 0 0 1 2-2h6a2 2 0 0 1 2 2v1.662"/>',
    menu: '<line x1="4" x2="20" y1="6" y2="6"/><line x1="4" x2="20" y1="12" y2="12"/><line x1="4" x2="20" y1="18" y2="18"/>',
    x: '<path d="M18 6 6 18"/><path d="m6 6 12 12"/>',
    chevrons: '<path d="m11 17-5-5 5-5"/><path d="m18 17-5-5 5-5"/>',
  };
  const I = (n) => LU[n] ? `<svg ${ATTRS}>${LU[n]}</svg>` : icon(n);
  const themeBtn = `<button class="ae-mode" type="button" aria-label="Theme">${icon("sun")}${icon("moon")}</button>`;

  // per-place metadata (icon + live figure) — the shadcn place set, minus
  // retired Clips, Sanctum/Wire relegated to the foot.
  const PLACES = [
    { key: "now", label: "Now", icon: "activity", live: `<span class="ae-num">${S.live}</span> live` },
    { key: "needs", label: "Needs you", icon: "inbox", badge: S.needYou, live: `${icon("warn")}<span class="ae-num">${S.needYou}</span>` },
    { key: "reports", label: "Reports", icon: "filetext", live: `<span class="ae-num">${DATA.reports.length}</span> filed` },
  ];
  // scopes = the lens the wire/reports read through (shadcn's second group)
  const SCOPES = [["Fleet", DATA.synthesis.numbers.completed, true],
    ...DATA.synthesis.byRepo.slice(0, 3).map(([r, n]) => [r, n, false])];

  // ── shared rail pieces (composed by SHELL-7 and the phone sheet) ────
  const railHead = () =>
    `<div class="r2s-head"><span class="ae-logo"><span class="ae-app-mark">${icon("glass")}</span><span class="ae-name">GLASS</span></span></div>`;

  function placeLink(p, active, mode) {
    let trail = "";
    if (mode === "live") trail = `<span class="r2s-link-fig">${p.live}</span>`;
    else if (p.badge != null) trail = `<span class="r2s-link-badge">${icon("warn")}<span class="ae-num">${p.badge}</span></span>`;
    return `<a class="r2s-link" href="#0"${active === p.key ? ' aria-current="page"' : ""}>${I(p.icon)}<span class="r2s-link-label">${p.label}</span>${trail}</a>`;
  }
  const placesGroup = (active, mode) =>
    `<div class="r2s-group"><p class="ae-h">PLACES</p><nav>${PLACES.map((p) => placeLink(p, active, mode)).join("")}</nav></div>`;

  const scopesGroup = () =>
    `<div class="r2s-group"><p class="ae-h">SCOPES</p><nav>${SCOPES.map(([label, n, act]) =>
      `<a class="r2s-link${act ? " is-active" : ""}" href="#0">${I("folder")}<span class="r2s-link-label">${esc(label)}</span><span class="r2s-link-fig"><span class="ae-num">${n}</span></span></a>`).join("")}</nav>`
    + `</div>`;

  const footAccount = () =>
    `<div class="r2s-foot">
      <div class="r2s-foot-links">
        <a class="r2s-link r2s-link-sm" href="#0">${I("home")}<span class="r2s-link-label">Sanctum</span></a>
        <a class="r2s-link r2s-link-sm" href="#0">${I("plug")}<span class="r2s-link-label">Wire an agent</span></a>
      </div>
      <a class="r2s-user" href="#0">
        <span class="ae-app-mark">${I("user")}</span>
        <span class="r2s-user-id"><span class="ae-item">phaedrus</span><span class="ae-dim">mistystep.io</span></span>
        ${themeBtn}
      </a>
    </div>`;

  // the phone top bar (hidden on desktop for 7–10; always shown in 11)
  const topbar = () =>
    `<header class="r2s-topbar">
      <button class="r2s-burger" type="button" aria-label="Open navigation">${I("menu")}</button>
      <span class="ae-logo ae-logo-compact"><span class="ae-app-mark">${icon("glass")}</span><span class="ae-name">GLASS</span></span>
      <a class="r2s-topbadge" href="#0" aria-label="Needs you"><span class="ae-status">${icon("warn")}<span class="ae-num">${S.needYou}</span></span></a>
    </header>`;

  // ── shared NOW desk body (shipped composition, via parts) ───────────
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
    "SHELL-7": {
      label: "shadcn translation",
      thesis: "The closest kit translation of shadcn's grouped sidebar: per-place Lucide icons in labeled PLACES and SCOPES groups above a pinned account foot, active carried by ink weight and a hairline indicator bar.",
      build() {
        const rail = `<aside class="r2s-rail">${railHead()}${placesGroup("now", false)}${scopesGroup()}${footAccount()}</aside>`;
        return `<div class="r2s-shell">${rail}${topbar()}<main class="r2s-desk">${nowDeskBody()}</main></div>`;
      },
    },

    "SHELL-8": {
      label: "Collapsed icon rail (inversion)",
      thesis: "Inverts 'labels always visible': the rail collapses to a 56px icon dock — app-mark only, centered place glyphs with labels implied by tooltips, active as a hairline box — trading the wordmark for maximum desk width.",
      build() {
        const dot = (k) => k === "needs" ? '<span class="r2s-mini-dot" aria-hidden="true"></span>' : "";
        const miniLinks = PLACES.map((p) =>
          `<a class="r2s-mini-link" href="#0" title="${p.label}" aria-label="${p.label}"${p.key === "now" ? ' aria-current="page"' : ""}>${I(p.icon)}${dot(p.key)}</a>`).join("");
        const rail = `<aside class="r2s-rail-mini">
          <span class="ae-app-mark r2s-mini-head" title="GLASS">${icon("glass")}</span>
          <a class="r2s-mini-link" href="#0" title="Expand navigation" aria-label="Expand navigation">${I("chevrons")}</a>
          <nav class="r2s-mini-nav">${miniLinks}</nav>
          <div class="r2s-mini-foot">
            <a class="r2s-mini-link" href="#0" title="Sanctum" aria-label="Sanctum">${I("home")}</a>
            <a class="r2s-mini-link" href="#0" title="Wire an agent" aria-label="Wire an agent">${I("plug")}</a>
            ${themeBtn}
            <span class="ae-app-mark" title="phaedrus">${I("user")}</span>
          </div>
        </aside>`;
        return `<div class="r2s-shell-mini">${rail}${topbar()}<main class="r2s-desk">${nowDeskBody()}</main></div>`;
      },
    },

    "SHELL-9": {
      label: "Live-figure rail",
      thesis: "Every place carries one quiet live figure inline — Now 6 live, Needs you a warn glyph and 3, Reports 4 filed — so the rail reads its own state at a glance without the noisy per-place trace of round 1.",
      build() {
        const rail = `<aside class="r2s-rail">${railHead()}${placesGroup("now", "live")}${footAccount()}</aside>`;
        return `<div class="r2s-shell">${rail}${topbar()}<main class="r2s-desk">${nowDeskBody()}</main></div>`;
      },
    },

    "SHELL-10": {
      label: "Rail + contextual panel",
      thesis: "shadcn's dual-pane sidebar (sidebar-07): a slim primary rail of places feeds a secondary contextual panel that expands the active place — Now's live agents and session count — before the desk.",
      build() {
        const rail = `<aside class="r2s-rail r2s-rail-slim">${railHead()}${placesGroup("now", false)}${footAccount()}</aside>`;
        const recents = DATA.agents.filter((a) => a.state === "publishing").map((a) =>
          `<a class="r2s-panel-item" href="#0">${P.stateIcon(a.state)}<span class="r2s-panel-name">${esc(a.name)}</span><span class="r2s-panel-age">${esc(a.age)}</span></a>`).join("");
        const panel = `<div class="r2s-panel">
          <p class="ae-h">NOW</p>
          <p class="ae-dim r2s-panel-sub">${S.live} live &middot; ${S.quiet} quiet</p>
          <p class="ae-h">RECENT AGENTS</p>
          <div class="r2s-panel-list">${recents}</div>
          <p class="ae-h">SESSIONS TODAY</p>
          <p><span class="ae-num ae-strong">${S.sessionsToday}</span> <span class="ae-dim">open</span></p>
        </div>`;
        return `<div class="r2s-shell-panel">${rail}${panel}${topbar()}<main class="r2s-desk">${nowDeskBody()}</main></div>`;
      },
    },

    "SHELL-11": {
      label: "Phone sheet (390-first)",
      thesis: "The phone spec, built 390-first: a thin top bar (burger + GLASS + a needs-you badge) opening a left slide-over navigation sheet, rendered OPEN over the dimmed NOW desk — desktop just frames it narrow and centered.",
      build() {
        const sheet = `<div class="r2s-sheet">
          <div class="r2s-sheet-head">
            <span class="ae-logo ae-logo-compact"><span class="ae-app-mark">${icon("glass")}</span><span class="ae-name">GLASS</span></span>
            <button class="r2s-sheet-close" type="button" aria-label="Close navigation">${I("x")}</button>
          </div>
          ${placesGroup("now", false)}
          ${scopesGroup()}
          ${footAccount()}
        </div>`;
        return `<div class="r2s-phone">
          ${topbar()}
          <div class="r2s-phone-body">
            <div class="r2s-phone-desk">${nowDeskBody()}</div>
            <div class="r2s-scrim" aria-hidden="true"></div>
            ${sheet}
          </div>
        </div>`;
      },
    },
  });
})();
