pub(crate) const REPORTS_PATH: &str = "/reports";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Place {
    Now,
    NeedsYou,
    Reports,
}

pub(crate) struct Shell<'a> {
    pub title: &'a str,
    pub active: Option<Place>,
    pub needs_you_count: Option<usize>,
    pub sanctum_url: &'a str,
    pub styles: &'a str,
    pub body: &'a str,
    pub scripts: &'a str,
}

pub(crate) fn render_shell(page: Shell<'_>) -> String {
    let title = escape_html(page.title);
    let sanctum_url = escape_html(page.sanctum_url);
    let rail = render_rail(page.active, page.needs_you_count, &sanctum_url);
    let topbar = render_topbar(page.needs_you_count);
    let page_scripts = if page.scripts.trim().is_empty() {
        String::new()
    } else {
        format!("<script>\n{}\n</script>", page.scripts)
    };

    format!(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title}</title>
<link rel="stylesheet" href="/aesthetic.css">
<script>
try {{
  var m = localStorage.getItem('ae-mode');
  if (m === 'dark' || m === 'light') {{
    document.documentElement.classList.add(m);
    document.documentElement.style.colorScheme = m;
  }} else {{
    document.documentElement.style.colorScheme = 'light dark';
  }}
}} catch (e) {{}}
</script>
<style>
.ae-shell.glass-shell {{
  grid-template-columns: 15rem minmax(0, 1fr);
}}
.glass-topbar {{
  display: none;
}}
.glass-nav-scrim {{
  display: none;
}}
.glass-rail {{
  padding: 1.4em;
}}
.glass-rail-head {{
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--ae-space-3);
  padding-bottom: 1.1em;
  margin-bottom: 0.5em;
  border-bottom: 1px solid var(--ae-line);
}}
.glass-rail-group {{
  margin-top: 1.5em;
}}
.glass-rail-group .ae-h {{
  margin: 0 0 0.5em;
}}
.glass-rail-nav,
.glass-rail-foot {{
  display: grid;
}}
.glass-rail-link {{
  display: flex;
  align-items: center;
  gap: var(--ae-space-3);
  width: auto;
  padding: 0.42em 0 0.42em 1.4em;
  margin-left: -1.4em;
  border-left: 2px solid transparent;
  color: var(--ae-ink-muted);
  text-decoration: none;
}}
.glass-rail-link:hover {{
  color: var(--ae-ink);
}}
.glass-rail-link[aria-current] {{
  color: var(--ae-ink);
  font-weight: var(--ae-w-medium);
  border-left-color: var(--ae-ink);
}}
.glass-rail-link > .ae-icon {{
  flex: none;
  color: var(--ae-ink-faint);
}}
.glass-rail-link[aria-current] > .ae-icon {{
  color: var(--ae-ink);
}}
.glass-rail-link-label {{
  flex: 1 1 auto;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}}
.glass-rail-link-badge {{
  flex: none;
  display: inline-flex;
  align-items: baseline;
  gap: 0.25em;
  font-family: var(--ae-font-mono);
  font-size: 12px;
  color: var(--ae-ink-muted);
}}
.glass-rail-foot {{
  margin-top: auto;
  padding-top: 1.2em;
  border-top: 1px solid var(--ae-line);
}}
.glass-rail-foot .ae-mode {{
  margin-top: 0.45em;
}}
.glass-sheet-close {{
  display: none;
}}
@media (max-width: 48rem) {{
  .ae-shell.glass-shell {{
    grid-template-columns: 1fr;
    grid-template-rows: auto minmax(0, 1fr);
  }}
  .glass-topbar {{
    display: flex;
    grid-column: 1;
    grid-row: 1;
    align-items: center;
    gap: var(--ae-space-3);
    padding: 0.7em 1rem;
    border-bottom: 1px solid var(--ae-line);
  }}
  .glass-burger,
  .glass-sheet-close {{
    display: inline-flex;
    align-items: center;
    justify-content: center;
    background: transparent;
    border: 0;
    padding: 0.2em;
    color: var(--ae-ink);
    cursor: pointer;
  }}
  .glass-burger .ae-icon,
  .glass-sheet-close .ae-icon {{
    width: 1.4em;
    height: 1.4em;
  }}
  .glass-top-logo {{
    color: var(--ae-ink);
    text-decoration: none;
  }}
  .glass-top-needs {{
    margin-left: auto;
    display: inline-flex;
    align-items: center;
    gap: 0.35em;
    color: var(--ae-ink);
    text-decoration: none;
    font-family: var(--ae-font-mono);
    font-size: 13px;
  }}
  .glass-top-needs .ae-icon {{
    align-self: center;
    color: var(--ae-warn, var(--ae-ink));
  }}
  .ae-shell.glass-shell .ae-desk {{
    grid-column: 1;
    grid-row: 2;
    min-height: 0;
    padding: 1.6em 1.2rem;
  }}
  .ae-shell.glass-shell .ae-rail {{
    position: fixed;
    z-index: 30;
    inset-block: 0;
    inset-inline-start: 0;
    width: min(82vw, 20rem);
    box-sizing: border-box;
    display: flex;
    flex-direction: column;
    gap: 0;
    border-top: 0;
    border-right: 1px solid var(--ae-line);
    background: var(--ae-surface);
    color: var(--ae-ink-muted);
    padding: 1.2em 1.3em;
    overflow-y: auto;
    transform: translateX(-100%);
    transition: transform var(--ae-quick) var(--ae-ease);
  }}
  .dark .ae-shell.glass-shell .ae-rail,
  [data-ae-mode='dark'] .ae-shell.glass-shell .ae-rail {{
    background: var(--ae-wash);
  }}
  .ae-shell.glass-shell[data-nav-open="true"] .ae-rail {{
    transform: translateX(0);
  }}
  .ae-shell.glass-shell[data-nav-open="true"] .glass-nav-scrim {{
    display: block;
    position: fixed;
    z-index: 20;
    inset: 0;
    background: var(--ae-surface);
    border: 0;
    padding: 0;
    opacity: 0.62;
  }}
  .glass-rail-head {{
    padding-bottom: 1em;
    margin-bottom: 0.4em;
  }}
  .glass-rail .ae-h {{
    display: block;
  }}
  .glass-rail-link {{
    width: auto;
    white-space: normal;
  }}
}}
{styles}
</style>
</head>
<body>
<div class="ae-shell glass-shell" data-glass-shell>
  {topbar}
  {rail}
  <button class="glass-nav-scrim" type="button" aria-label="Close navigation" data-glass-nav-close></button>
  <main class="ae-desk glass-desk">
{body}
  </main>
</div>
<script>
{SHELL_SCRIPT}
{MODE_SCRIPT}
</script>
{page_scripts}
</body>
</html>"#,
        styles = page.styles,
        body = page.body,
    )
}

fn render_topbar(needs_you_count: Option<usize>) -> String {
    let count = needs_you_count
        .map(|count| format!(r#"<span data-needs-you-count>{count}</span>"#))
        .unwrap_or_else(|| r#"<span data-needs-you-count>?</span>"#.to_string());
    let label = needs_you_count
        .map(|count| format!("Needs you {count}"))
        .unwrap_or_else(|| "Needs you".to_string());
    format!(
        r#"<header class="glass-topbar">
    <button class="glass-burger" type="button" aria-label="Open navigation" aria-expanded="false" aria-controls="glass-rail" data-glass-nav-open>{MENU_SVG}</button>
    <a class="ae-logo ae-logo-compact glass-top-logo" href="/" aria-label="Glass">
      <span class="ae-app-mark">{APP_MARK_SVG}</span><span class="ae-name">GLASS</span>
    </a>
    <a class="glass-top-needs" href="/needs-you" aria-label="{label}">{WARN_SVG}{count}</a>
  </header>"#
    )
}

fn render_rail(active: Option<Place>, needs_you_count: Option<usize>, sanctum_url: &str) -> String {
    format!(
        r#"<aside id="glass-rail" class="ae-rail glass-rail" aria-label="Glass places">
    <div class="glass-rail-head">
      <a class="ae-logo ae-logo-compact" href="/" aria-label="Glass">
        <span class="ae-app-mark">{APP_MARK_SVG}</span>
        <span class="ae-name">GLASS</span>
      </a>
      <button class="glass-sheet-close" type="button" aria-label="Close navigation" data-glass-nav-close>{X_SVG}</button>
    </div>
    <div class="glass-rail-group">
      <p class="ae-h">PLACES</p>
      <nav class="glass-rail-nav" aria-label="Places">
        {now}
        {needs_you}
        {reports}
      </nav>
    </div>
    <div class="glass-rail-foot">
      <a class="glass-rail-link" data-sanctum-home href="{sanctum_url}" aria-label="Back to Sanctum" title="Back to Sanctum">{HOME_SVG}<span class="glass-rail-link-label">Sanctum</span></a>
      <a class="glass-rail-link" href="/setup">{PLUG_SVG}<span class="glass-rail-link-label">Wire an agent</span></a>
      <button class="ae-mode" type="button" aria-label="Theme">{SUN_SVG}{MOON_SVG}</button>
    </div>
  </aside>"#,
        now = place_link(Place::Now, "/", "Now", ACTIVITY_SVG, None, active),
        needs_you = place_link(
            Place::NeedsYou,
            "/needs-you",
            "Needs you",
            INBOX_SVG,
            needs_you_count,
            active,
        ),
        reports = place_link(
            Place::Reports,
            REPORTS_PATH,
            "Reports",
            FILE_TEXT_SVG,
            None,
            active
        ),
    )
}

fn place_link(
    place: Place,
    href: &str,
    label: &str,
    icon: &str,
    count: Option<usize>,
    active: Option<Place>,
) -> String {
    let current = if active == Some(place) {
        r#" aria-current="page""#
    } else {
        ""
    };
    let badge = count
        .map(|count| {
            format!(
                r#" <span class="glass-rail-link-badge">&middot; <span data-needs-you-count>{count}</span></span>"#
            )
        })
        .unwrap_or_default();
    format!(
        r#"<a class="glass-rail-link" href="{href}"{current}>{icon}<span class="glass-rail-link-label">{label}</span>{badge}</a>"#
    )
}

fn escape_html(raw: &str) -> String {
    raw.chars()
        .flat_map(|ch| match ch {
            '&' => "&amp;".chars().collect::<Vec<_>>(),
            '<' => "&lt;".chars().collect(),
            '>' => "&gt;".chars().collect(),
            '"' => "&quot;".chars().collect(),
            '\'' => "&#39;".chars().collect(),
            _ => vec![ch],
        })
        .collect()
}

const APP_MARK_SVG: &str = r#"<svg class="ae-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M11 6 8 9"></path><path d="m16 7-8 8"></path><rect x="4" y="2" width="16" height="20"></rect></svg>"#;

const HOME_SVG: &str = r#"<svg class="ae-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M15 21v-8a1 1 0 0 0-1-1h-4a1 1 0 0 0-1 1v8"></path><path d="M3 10a2 2 0 0 1 .709-1.528l7-6a2 2 0 0 1 2.582 0l7 6A2 2 0 0 1 21 10v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"></path></svg>"#;

const ACTIVITY_SVG: &str = r#"<svg class="ae-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M22 12h-2.48a2 2 0 0 0-1.93 1.46l-2.35 8.36a.25.25 0 0 1-.48 0L9.24 2.18a.25.25 0 0 0-.48 0l-2.35 8.36A2 2 0 0 1 4.49 12H2"></path></svg>"#;

const INBOX_SVG: &str = r#"<svg class="ae-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M22 12h-6l-2 3h-4l-2-3H2"></path><path d="M5.45 5.11 2 12v6a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2v-6l-3.45-6.89A2 2 0 0 0 16.76 4H7.24a2 2 0 0 0-1.79 1.11z"></path></svg>"#;

const FILE_TEXT_SVG: &str = r#"<svg class="ae-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z"></path><path d="M14 2v4a2 2 0 0 0 2 2h4"></path><path d="M10 9H8"></path><path d="M16 13H8"></path><path d="M16 17H8"></path></svg>"#;

const PLUG_SVG: &str = r#"<svg class="ae-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M12 22v-5"></path><path d="M9 8V2"></path><path d="M15 8V2"></path><path d="M18 8v5a4 4 0 0 1-4 4h-4a4 4 0 0 1-4-4V8Z"></path></svg>"#;

const MENU_SVG: &str = r#"<svg class="ae-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><line x1="4" x2="20" y1="6" y2="6"></line><line x1="4" x2="20" y1="12" y2="12"></line><line x1="4" x2="20" y1="18" y2="18"></line></svg>"#;

const X_SVG: &str = r#"<svg class="ae-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M18 6 6 18"></path><path d="m6 6 12 12"></path></svg>"#;

const WARN_SVG: &str = r#"<svg class="ae-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="m21.73 18-8-14a2 2 0 0 0-3.48 0l-8 14A2 2 0 0 0 4 20h16a2 2 0 0 0 1.73-2Z"></path><path d="M12 9v4"></path><path d="M12 17h.01"></path></svg>"#;

const SUN_SVG: &str = r#"<svg class="ae-icon ae-sun" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><circle cx="12" cy="12" r="4"></circle><path d="M12 2v2"></path><path d="M12 20v2"></path><path d="m4.93 4.93 1.41 1.41"></path><path d="m17.66 17.66 1.41 1.41"></path><path d="M2 12h2"></path><path d="M20 12h2"></path><path d="m6.34 17.66-1.41 1.41"></path><path d="m19.07 4.93-1.41 1.41"></path></svg>"#;

const MOON_SVG: &str = r#"<svg class="ae-icon ae-moon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M12 3a6 6 0 0 0 9 9 9 9 0 1 1-9-9Z"></path></svg>"#;

const SHELL_SCRIPT: &str = r#"(() => {
  const shell = document.querySelector('[data-glass-shell]');
  if (!shell) return;
  const open = shell.querySelector('[data-glass-nav-open]');
  const closers = shell.querySelectorAll('[data-glass-nav-close]');
  const setOpen = (isOpen) => {
    shell.dataset.navOpen = isOpen ? 'true' : 'false';
    if (open) open.setAttribute('aria-expanded', String(isOpen));
  };
  if (open) open.addEventListener('click', () => setOpen(true));
  closers.forEach((el) => el.addEventListener('click', () => setOpen(false)));
  shell.querySelectorAll('.glass-rail a').forEach((link) => {
    link.addEventListener('click', () => setOpen(false));
  });
  document.addEventListener('keydown', (event) => {
    if (event.key === 'Escape') setOpen(false);
  });
  setOpen(false);
})();"#;

const MODE_SCRIPT: &str = r#"(() => {
  const root = document.documentElement;
  const modes = ['system', 'dark', 'light'];
  let activeTransition = null;
  let easingTimer = 0;
  let runId = 0;
  let targetMode = null;
  const storedMode = () => {
    try {
      const mode = localStorage.getItem('ae-mode');
      return modes.includes(mode) ? mode : 'system';
    } catch (e) {
      return 'system';
    }
  };
  const currentMode = () => targetMode || root.dataset.mode || storedMode();
  const reducedMode = matchMedia('(prefers-reduced-motion: reduce)');
  const clearAnimation = () => {
    if (activeTransition && activeTransition.skipTransition) activeTransition.skipTransition();
    activeTransition = null;
    if (easingTimer) { clearTimeout(easingTimer); easingTimer = 0; }
    root.classList.remove('ae-vt-mode', 'ae-mode-easing');
  };
  const updateButtons = (mode) => {
    document.querySelectorAll('.ae-mode').forEach((btn) => {
      btn.dataset.mode = mode;
      btn.setAttribute('aria-label', `color mode: ${mode}`);
      btn.setAttribute('title', `Color mode: ${mode}`);
    });
  };
  const applyMode = (mode) => {
    root.classList.toggle('dark', mode === 'dark');
    root.classList.toggle('light', mode === 'light');
    root.style.colorScheme = mode === 'system' ? 'light dark' : mode;
    root.dataset.mode = mode;
    try { localStorage.setItem('ae-mode', mode); } catch (e) {}
    updateButtons(mode);
  };
  applyMode(storedMode());
  document.querySelectorAll('.ae-mode').forEach((btn) => {
    btn.addEventListener('click', () => {
      const mode = currentMode();
      const nextMode = modes[(modes.indexOf(mode) + 1) % modes.length];
      const id = ++runId;
      targetMode = nextMode;
      const flip = () => { if (id !== runId) return; applyMode(nextMode); };
      clearAnimation();
      if (reducedMode.matches) {
        flip();
        targetMode = null;
      } else if (document.startViewTransition) {
        root.classList.add('ae-vt-mode');
        activeTransition = document.startViewTransition(flip);
        easingTimer = setTimeout(() => {
          if (id !== runId) return;
          root.classList.remove('ae-vt-mode');
          easingTimer = 0;
        }, 180);
        activeTransition.finished.finally(() => {
          if (id !== runId) return;
          root.classList.remove('ae-vt-mode');
          activeTransition = null;
          targetMode = null;
          if (easingTimer) { clearTimeout(easingTimer); easingTimer = 0; }
        });
      } else {
        root.classList.add('ae-mode-easing');
        flip();
        easingTimer = setTimeout(() => {
          if (id !== runId) return;
          root.classList.remove('ae-mode-easing');
          easingTimer = 0;
          targetMode = null;
        }, 180);
      }
    });
  });
})();"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_marks_active_place_and_needs_you_count() {
        let html = render_shell(Shell {
            title: "Glass",
            active: Some(Place::NeedsYou),
            needs_you_count: Some(3),
            sanctum_url: "/",
            styles: "",
            body: "<p>body</p>",
            scripts: "",
        });

        assert!(html.contains(r#"<div class="ae-shell glass-shell" data-glass-shell>"#));
        assert!(html.contains(
            r#"<aside id="glass-rail" class="ae-rail glass-rail" aria-label="Glass places">"#
        ));
        assert!(
            html.contains(r#"<a class="glass-rail-link" href="/needs-you" aria-current="page">"#)
        );
        assert!(html.contains(r#"<span data-needs-you-count>3</span>"#));
        assert!(!html.contains(r#"href="/clips""#));
        assert_eq!(html.matches(r#"aria-current="page""#).count(), 1);
    }

    #[test]
    fn shell_degrades_needs_you_count_without_breaking_the_link() {
        let html = render_shell(Shell {
            title: "Glass",
            active: Some(Place::Now),
            needs_you_count: None,
            sanctum_url: "/",
            styles: "",
            body: "<p>body</p>",
            scripts: "",
        });

        assert!(html.contains(r#"<a class="glass-rail-link" href="/needs-you">"#));
        assert!(
            html.contains(
                r#"<a class="glass-top-needs" href="/needs-you" aria-label="Needs you">"#
            )
        );
        assert!(!html.contains("Needs you &middot;"));
    }
}
