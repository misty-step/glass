pub(crate) const REPORTS_PATH: &str = "/reports";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Place {
    Now,
    NeedsYou,
    Reports,
    Clips,
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
.ae-rail-foot {{
  display: grid;
  gap: 0.2em;
}}
@media (max-width: 48rem) {{
  .ae-rail-foot {{
    display: flex;
    align-items: center;
    gap: var(--ae-space-5);
  }}
}}
{styles}
</style>
</head>
<body>
<div class="ae-shell" data-glass-shell>
  {rail}
  <main class="ae-desk">
{body}
  </main>
</div>
<script>
{MODE_SCRIPT}
</script>
{page_scripts}
</body>
</html>"#,
        styles = page.styles,
        body = page.body,
    )
}

fn render_rail(active: Option<Place>, needs_you_count: Option<usize>, sanctum_url: &str) -> String {
    let needs_you_label = match needs_you_count {
        Some(count) => format!("Needs you &middot; {count}"),
        None => "Needs you".to_string(),
    };
    format!(
        r#"<aside class="ae-rail" aria-label="Glass places">
    <a class="ae-logo ae-logo-compact" href="/" aria-label="Glass">
      <span class="ae-app-mark">{APP_MARK_SVG}</span>
      <span class="ae-name">GLASS</span>
    </a>
    <p class="ae-h">PLACES</p>
    {now}
    {needs_you}
    {reports}
    {clips}
    <div class="ae-rail-foot">
      <a data-sanctum-home href="{sanctum_url}" aria-label="Back to Sanctum" title="Back to Sanctum">{HOME_SVG} Sanctum</a>
      <a href="/setup">Wire an agent</a>
      <button class="ae-mode" type="button" aria-label="Theme">{SUN_SVG}{MOON_SVG}</button>
    </div>
  </aside>"#,
        now = place_link(Place::Now, "/", "Now", active),
        needs_you = place_link(Place::NeedsYou, "/needs-you", &needs_you_label, active),
        reports = place_link(Place::Reports, REPORTS_PATH, "Reports", active),
        clips = place_link(Place::Clips, "/clips", "Clips", active),
    )
}

fn place_link(place: Place, href: &str, label: &str, active: Option<Place>) -> String {
    let current = if active == Some(place) {
        r#" aria-current="page""#
    } else {
        ""
    };
    format!(r#"<a href="{href}"{current}>{label}</a>"#)
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

const SUN_SVG: &str = r#"<svg class="ae-icon ae-sun" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><circle cx="12" cy="12" r="4"></circle><path d="M12 2v2"></path><path d="M12 20v2"></path><path d="m4.93 4.93 1.41 1.41"></path><path d="m17.66 17.66 1.41 1.41"></path><path d="M2 12h2"></path><path d="M20 12h2"></path><path d="m6.34 17.66-1.41 1.41"></path><path d="m19.07 4.93-1.41 1.41"></path></svg>"#;

const MOON_SVG: &str = r#"<svg class="ae-icon ae-moon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M12 3a6 6 0 0 0 9 9 9 9 0 1 1-9-9Z"></path></svg>"#;

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

        assert!(html.contains(r#"<div class="ae-shell" data-glass-shell>"#));
        assert!(html.contains(r#"<aside class="ae-rail" aria-label="Glass places">"#));
        assert!(
            html.contains(r#"<a href="/needs-you" aria-current="page">Needs you &middot; 3</a>"#)
        );
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

        assert!(html.contains(r#"<a href="/needs-you">Needs you</a>"#));
        assert!(!html.contains("Needs you &middot;"));
    }
}
