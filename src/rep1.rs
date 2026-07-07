//! REP-1 (glass-913): the window-parameterized narrative report, locked as
//! Glass's primary view after the operator's lab-3 walk. Consumes
//! glass-917's shelf-fetched fleet-retro spec through the shared
//! `glance_catalog` renderer -- no fourth report renderer, no re-derived
//! synthesis. Ticket-grouping/SDLC-arc structure is enforced by grouping
//! citations into `Disclosure(Timeline)` structural components deterministically
//! (string matching over citation titles), not by asking a model to group them,
//! so grouping cannot silently regress under window changes the way a
//! prompt-only ask could.
//!
//! Window tabs match the operator's lab-3-locked design (30m/1h/24h/7d).
//! Only `24h` and `7d` are wired to real data -- they are the only windows
//! fleet-retro's nightly/weekly cron actually publishes (glass-917). `30m`
//! and `1h` render as visibly disabled tabs pending glass-919's on-demand
//! synthesis service; this is the same descoping team-lead already ruled on
//! for glass-917, applied consistently here rather than re-litigated.

use std::collections::BTreeMap;
use std::convert::Infallible;

use axum::extract::Path as AxumPath;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::response::sse::{Event, KeepAlive, Sse};
use chrono::{DateTime, Utc};
use glance_catalog::component::Component;
use glance_catalog::render::{RenderContext, render_component};
use glance_catalog::structural::{Disclosure, Hero, Narrative, Timeline, TimelineEntry};
use serde::Deserialize;
use serde_json::json;

/// The four windows the operator's lab-3 design locked. Only the two
/// fleet-retro actually schedules resolve to real data today.
struct WindowTab {
    id: &'static str,
    label: &'static str,
    live: bool,
}

const WINDOW_TABS: [WindowTab; 4] = [
    WindowTab {
        id: "30m",
        label: "30m",
        live: false,
    },
    WindowTab {
        id: "1h",
        label: "1h",
        live: false,
    },
    WindowTab {
        id: "24h",
        label: "24h",
        live: true,
    },
    WindowTab {
        id: "7d",
        label: "7d",
        live: true,
    },
];

/// Renders the tab bar server-side from `WINDOW_TABS` -- the single source
/// of truth for tab id/label/live-state, so the client never carries its
/// own duplicate copy that could drift.
fn render_tabs_html(active: &str) -> String {
    WINDOW_TABS
        .iter()
        .map(|tab| {
            let classes = if tab.id == active {
                "rep1-tab active"
            } else {
                "rep1-tab"
            };
            let disabled = if tab.live {
                String::new()
            } else {
                r#" disabled title="needs glass-919 on-demand synthesis""#.to_string()
            };
            format!(
                r#"<button class="{classes}" data-win="{}"{disabled}>{}</button>"#,
                tab.id, tab.label
            )
        })
        .collect()
}

/// Maps REP-1's lab-locked window ids to the shelf slugs fleet-retro
/// actually publishes under (glass-917: `daily`/`weekly`, not `24h`/`7d`).
fn shelf_window_for(tab_id: &str) -> Option<&'static str> {
    match tab_id {
        "24h" => Some("daily"),
        "7d" => Some("weekly"),
        _ => None,
    }
}

#[derive(Debug, Deserialize)]
struct RawSpec {
    generated_at: String,
    components: Vec<RawComponent>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum RawComponent {
    Hero(Hero),
    Narrative {
        narrative: Narrative,
        citations: Vec<RawCitation>,
    },
    // REP-1's locked design is narrative-column-first; these sections are
    // REP-2/REP-3 territory the operator explicitly nixed. The payload is
    // parsed (so an unrecognized-shape spec still deserializes) and then
    // deliberately discarded in `build_components` -- see the match arm
    // there for why.
    #[allow(dead_code)]
    Table(serde_json::Value),
    #[allow(dead_code)]
    Timeline(serde_json::Value),
    #[allow(dead_code)]
    Receipts(serde_json::Value),
    #[allow(dead_code)]
    Footer(serde_json::Value),
    #[allow(dead_code)]
    Provenance(serde_json::Value),
}

#[derive(Debug, Deserialize, Clone)]
struct RawCitation {
    id: String,
    title: String,
}

/// Deterministic ticket-id extraction over a citation's title: scans
/// whitespace/punctuation-delimited tokens for the first one shaped like
/// `<slug>-<digits>` (e.g. `glass-915`, `misty-step-921`). No model call --
/// this is exactly the "structural, not prompt-only" grouping glass-913's
/// acceptance requires, so ticket grouping cannot regress under window
/// changes the way an LLM-prompted grouping ask could.
fn extract_ticket_id(title: &str) -> Option<String> {
    title
        .split(|c: char| c.is_whitespace() || "()[]{}:,;\"'“”—".contains(c))
        .find_map(|token| {
            let token = token.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '-');
            let (prefix, suffix) = token.rsplit_once('-')?;
            let looks_like_ticket = !prefix.is_empty()
                && !suffix.is_empty()
                && suffix.chars().all(|c| c.is_ascii_digit())
                && prefix
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
                // A repo/ticket slug has a letter in it somewhere; without
                // this a bare ISO date fragment like "2026-07-06" (all
                // digits and hyphens) false-matches as a ticket id.
                && prefix.chars().any(|c| c.is_ascii_lowercase());
            looks_like_ticket.then(|| token.to_string())
        })
}

/// Groups citations by their extracted ticket id (citations with no
/// detectable ticket id fall into `"general"`), preserving first-seen order
/// of both tickets and citations within a ticket.
fn group_citations_by_ticket(citations: &[RawCitation]) -> Vec<(String, Vec<RawCitation>)> {
    let mut order: Vec<String> = Vec::new();
    let mut groups: BTreeMap<String, Vec<RawCitation>> = BTreeMap::new();
    for citation in citations {
        let ticket = extract_ticket_id(&citation.title).unwrap_or_else(|| "general".to_string());
        if !groups.contains_key(&ticket) {
            order.push(ticket.clone());
        }
        groups.entry(ticket).or_default().push(citation.clone());
    }
    order
        .into_iter()
        .map(|ticket| {
            let citations = groups.remove(&ticket).unwrap_or_default();
            (ticket, citations)
        })
        .collect()
}

fn cite_href(ref_id: &str) -> String {
    format!("#cite-{ref_id}")
}

/// Builds the ordered catalog components for REP-1: Hero, Narrative (if the
/// pack shipped one), then one `Disclosure(Timeline)` per ticket group so
/// SDLC-arc grouping is a structural catalog kind, not a synthesis-prompt
/// ask that regresses under window changes.
fn build_components(raw: &RawSpec) -> Vec<Component> {
    let mut components = Vec::new();
    let mut all_citations: Vec<RawCitation> = Vec::new();

    for component in &raw.components {
        match component {
            RawComponent::Hero(hero) => components.push(Component::Hero(hero.clone())),
            RawComponent::Narrative {
                narrative,
                citations,
            } => {
                components.push(Component::Narrative(narrative.clone()));
                all_citations.extend(citations.iter().cloned());
            }
            RawComponent::Table(_)
            | RawComponent::Timeline(_)
            | RawComponent::Receipts(_)
            | RawComponent::Footer(_)
            | RawComponent::Provenance(_) => {
                // REP-1's locked design is narrative-column-first; these
                // sections are REP-2/REP-3 territory the operator explicitly
                // nixed. Citations (extracted above) are the one thing from
                // the wider pack REP-1 needs.
            }
        }
    }

    for (ticket, citations) in group_citations_by_ticket(&all_citations) {
        let heading = if ticket == "general" {
            "Other cited activity".to_string()
        } else {
            format!("Ticket: {ticket}")
        };
        let entries: Vec<TimelineEntry> = citations
            .iter()
            .map(|citation| TimelineEntry {
                at: raw.generated_at.clone(),
                actor: ticket.clone(),
                kind: "citation".to_string(),
                summary: citation.title.clone(),
                link: Some(cite_href(&citation.id)),
                detail: Vec::new(),
            })
            .collect();
        if entries.is_empty() {
            continue;
        }
        components.push(Component::Disclosure(Disclosure {
            heading: heading.clone(),
            children: vec![Component::Timeline(Timeline {
                heading,
                entries,
                empty_note: None,
            })],
        }));
    }

    components
}

fn render_all(raw: &RawSpec) -> Result<String, String> {
    let now: DateTime<Utc> = DateTime::parse_from_rfc3339(&raw.generated_at)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now());
    let ctx = RenderContext {
        now,
        cite_href: &cite_href,
    };
    let components = build_components(raw);
    let mut html = String::new();
    for component in &components {
        component.validate().map_err(|err| {
            format!("REP-1 refuses to render an invalid catalog component: {err}")
        })?;
        html.push_str(&render_component(component, &ctx));
    }
    Ok(html)
}

/// `GET /api/rep1/{window}` (window = one of `WINDOW_TABS`). Streams an
/// instant `skeleton` event, then a `full` event whose `html` field is
/// pre-rendered by `glance_catalog::render` -- the client only injects it,
/// it never reconstructs report structure from raw JSON itself.
pub async fn rep1_report(
    AxumPath(window): AxumPath<String>,
) -> Result<impl IntoResponse, StatusCode> {
    let Some(tab) = WINDOW_TABS.iter().find(|tab| tab.id == window) else {
        return Err(StatusCode::NOT_FOUND);
    };
    let shelf_window = shelf_window_for(tab.id);

    let stream = async_stream::stream! {
        yield Ok::<_, Infallible>(
            Event::default()
                .event("skeleton")
                .data(json!({"stage": "skeleton", "window": window}).to_string()),
        );

        let Some(shelf_window) = shelf_window else {
            yield Ok::<_, Infallible>(
                Event::default().event("error").data(
                    json!({
                        "stage": "error",
                        "window": window,
                        "message": format!(
                            "{window} needs glass-919's on-demand synthesis service; only 24h/7d are live today"
                        ),
                    })
                    .to_string(),
                ),
            );
            return;
        };

        let (is_hit, outcome) = crate::window_report::fetch_window(shelf_window, "fleet").await;
        match outcome {
            Ok(spec_json) => match serde_json::from_value::<RawSpec>(spec_json) {
                Ok(raw) => match render_all(&raw) {
                    Ok(html) => {
                        yield Ok::<_, Infallible>(
                            Event::default().event("full").data(
                                json!({
                                    "stage": "full",
                                    "window": window,
                                    "cache": if is_hit { "hit" } else { "miss" },
                                    "generated_at": raw.generated_at,
                                    "html": html,
                                })
                                .to_string(),
                            ),
                        );
                    }
                    Err(err) => {
                        crate::canary::report_error(
                            "glass.rep1.render.failed",
                            "route=/api/rep1/{window} error_kind=render",
                        );
                        yield Ok::<_, Infallible>(
                            Event::default().event("error").data(
                                json!({"stage": "error", "window": window, "message": err}).to_string(),
                            ),
                        );
                    }
                },
                Err(err) => {
                    crate::canary::report_error(
                        "glass.rep1.parse.failed",
                        "route=/api/rep1/{window} upstream=fleet-retro-shelf error_kind=parse",
                    );
                    yield Ok::<_, Infallible>(
                        Event::default().event("error").data(
                            json!({
                                "stage": "error",
                                "window": window,
                                "message": format!("could not parse fleet-retro spec: {err}"),
                            })
                            .to_string(),
                        ),
                    );
                }
            },
            Err(message) => {
                yield Ok::<_, Infallible>(
                    Event::default().event("error").data(
                        json!({"stage": "error", "window": window, "message": message}).to_string(),
                    ),
                );
            }
        }
    };

    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

const REP1_SHELL: &str = r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Glass — Fleet report</title>
<link rel="stylesheet" href="/aesthetic.css">
<script>
try {
  var m = localStorage.getItem('ae-mode');
  if (m === 'dark' || m === 'light') {
    document.documentElement.classList.add(m);
    document.documentElement.style.colorScheme = m;
  }
} catch (e) {}
</script>
<style>
.rep1-shell { max-width: 720px; margin: 0 auto; padding: var(--ae-space-6) var(--ae-space-5); }
.rep1-tabs { display: flex; gap: var(--ae-space-2); margin-bottom: var(--ae-space-5); flex-wrap: wrap; }
.rep1-tab { padding: var(--ae-space-2) var(--ae-space-4); border: 1px solid var(--ae-line); background: var(--ae-surface); color: var(--ae-ink); font-family: var(--ae-font-mono); font-size: 13px; cursor: pointer; }
.rep1-tab.active { background: var(--ae-ink); color: var(--ae-surface); }
.rep1-tab:disabled { opacity: 0.4; cursor: not-allowed; }
.rep1-sub { color: var(--ae-ink-muted); font-size: 13px; margin-bottom: var(--ae-space-5); }
.rep1-raw-link { display: inline-block; margin-top: var(--ae-space-6); font-size: 13px; }
.rep1-loading { color: var(--ae-ink-muted); padding: var(--ae-space-8) 0; text-align: center; }
</style>
</head>
<body>
<div class="ae-shell">
  <aside class="ae-rail">
    <a class="ae-logo ae-logo-compact" href="/">
      <span class="ae-app-mark"><svg class="ae-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M11 6 8 9"></path><path d="m16 7-8 8"></path><rect x="4" y="2" width="16" height="20"></rect></svg></span>
      <span class="ae-name">Glass</span>
    </a>
    <p class="ae-h">Report</p>
    <a href="/rep1" id="rep1-nav-active">Fleet report</a>
    <a href="/">Raw live feed</a>
  </aside>
  <main class="ae-desk">
    <div class="rep1-shell">
      <div class="rep1-tabs" id="rep1-tabs">{{TABS_HTML}}</div>
      <p class="rep1-sub" id="rep1-sub">Synthesized from the fleet-retro pack (git, Powder, bb, feed, receipts, moments) + canary + landmark.</p>
      <div id="rep1-body"><p class="rep1-loading">Loading&hellip;</p></div>
      <a class="rep1-raw-link" href="/">View raw per-agent feed &rarr;</a>
    </div>
  </main>
</div>
<script>
(function(){
  // The tab bar is server-rendered from WINDOW_TABS (single source of
  // truth for id/label/live-state) -- this script only wires clicks and
  // toggles the active class, it never duplicates the tab list.
  var tabsEl = document.getElementById('rep1-tabs');
  var bodyEl = document.getElementById('rep1-body');

  function wireTabs() {
    Array.prototype.forEach.call(tabsEl.querySelectorAll('.rep1-tab'), function(btn){
      btn.addEventListener('click', function(){
        if (btn.disabled) return;
        Array.prototype.forEach.call(tabsEl.querySelectorAll('.rep1-tab'), function(other){
          other.classList.remove('active');
        });
        btn.classList.add('active');
        load(btn.getAttribute('data-win'));
      });
    });
  }

  function load(win) {
    bodyEl.innerHTML = '<p class="rep1-loading">Loading&hellip;</p>';
    var es = new EventSource('/api/rep1/' + win);
    es.addEventListener('skeleton', function(){
      bodyEl.innerHTML = '<p class="rep1-loading">Synthesizing&hellip;</p>';
    });
    es.addEventListener('full', function(ev){
      var data = JSON.parse(ev.data);
      bodyEl.innerHTML = data.html;
      es.close();
    });
    es.addEventListener('error', function(ev){
      try {
        var data = JSON.parse(ev.data);
        bodyEl.innerHTML = '<p class="rep1-loading">' + data.message + '</p>';
      } catch (e) {
        bodyEl.innerHTML = '<p class="rep1-loading">Report unavailable.</p>';
      }
      es.close();
    });
  }

  wireTabs();
  load('24h');
})();
</script>
</body>
</html>"#;

pub async fn rep1_shell() -> impl IntoResponse {
    let html = REP1_SHELL.replace("{{TABS_HTML}}", &render_tabs_html("24h"));
    axum::response::Html(html)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_ticket_id_finds_a_leading_ticket_slug() {
        assert_eq!(
            extract_ticket_id("misty-step-921 (team-lead) \u{2014} commented: hi"),
            Some("misty-step-921".to_string())
        );
    }

    #[test]
    fn extract_ticket_id_finds_a_trailing_parenthesized_ticket_slug() {
        assert_eq!(
            extract_ticket_id("feat(secrets): read-guard against ~/.secrets (harness-kit-913)"),
            Some("harness-kit-913".to_string())
        );
    }

    #[test]
    fn extract_ticket_id_returns_none_when_no_ticket_shaped_token_exists() {
        assert_eq!(extract_ticket_id("just a plain sentence with no ids"), None);
    }

    #[test]
    fn extract_ticket_id_does_not_false_match_a_bare_iso_date() {
        // Regression: live fleet-retro citation titles carry bare dates like
        // "2026-07-06" mid-sentence, which is digits-and-hyphens shaped just
        // like a ticket id (prefix "2026-07", suffix "06") -- caught live
        // against the real daily shelf spec, where it fabricated two fake
        // ticket groups.
        assert_eq!(
            extract_ticket_id("incident on 2026-07-06 caused a rotation"),
            None
        );
        assert_eq!(
            extract_ticket_id("glass-915 shipped on 2026-07-07"),
            Some("glass-915".to_string())
        );
    }

    #[test]
    fn group_citations_by_ticket_buckets_and_preserves_order() {
        let citations = vec![
            RawCitation {
                id: "a".into(),
                title: "glass-915 did a thing".into(),
            },
            RawCitation {
                id: "b".into(),
                title: "unrelated note with no ticket".into(),
            },
            RawCitation {
                id: "c".into(),
                title: "glass-915 did another thing".into(),
            },
        ];
        let groups = group_citations_by_ticket(&citations);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].0, "glass-915");
        assert_eq!(groups[0].1.len(), 2);
        assert_eq!(groups[1].0, "general");
        assert_eq!(groups[1].1.len(), 1);
    }

    #[test]
    fn build_components_emits_hero_narrative_and_one_disclosure_per_ticket() {
        let raw = RawSpec {
            generated_at: "2026-07-07T00:00:00Z".to_string(),
            components: vec![
                RawComponent::Hero(Hero {
                    title: "Fleet retro".into(),
                    summary: vec![glance_catalog::inline::InlineNode::Text {
                        text: "window".into(),
                    }],
                    stats: vec![],
                    image_intent: None,
                }),
                RawComponent::Narrative {
                    narrative: Narrative {
                        heading: "What mattered".into(),
                        status: glance_catalog::structural::NarrativeStatus::Ok {
                            paragraphs: vec![vec![glance_catalog::inline::InlineNode::Text {
                                text: "narrative text".into(),
                            }]],
                        },
                    },
                    citations: vec![RawCitation {
                        id: "x".into(),
                        title: "glass-915 shipped".into(),
                    }],
                },
            ],
        };
        let components = build_components(&raw);
        assert!(matches!(components[0], Component::Hero(_)));
        assert!(matches!(components[1], Component::Narrative(_)));
        assert!(matches!(components[2], Component::Disclosure(_)));
    }

    #[test]
    fn render_all_produces_html_carrying_the_narrative_and_a_ticket_disclosure() {
        let raw = RawSpec {
            generated_at: "2026-07-07T00:00:00Z".to_string(),
            components: vec![
                RawComponent::Hero(Hero {
                    title: "Fleet retro".into(),
                    summary: vec![glance_catalog::inline::InlineNode::Text {
                        text: "window".into(),
                    }],
                    stats: vec![],
                    image_intent: None,
                }),
                RawComponent::Narrative {
                    narrative: Narrative {
                        heading: "What mattered".into(),
                        status: glance_catalog::structural::NarrativeStatus::Ok {
                            paragraphs: vec![vec![glance_catalog::inline::InlineNode::Text {
                                text: "narrative text".into(),
                            }]],
                        },
                    },
                    citations: vec![RawCitation {
                        id: "x".into(),
                        title: "glass-915 shipped".into(),
                    }],
                },
            ],
        };
        let html = render_all(&raw).expect("renders");
        assert!(html.contains("narrative text"));
        assert!(html.contains("Ticket: glass-915"));
        assert!(html.contains("data-glance-component=\"disclosure\""));
    }

    #[test]
    fn shelf_window_for_maps_only_the_two_live_windows() {
        assert_eq!(shelf_window_for("24h"), Some("daily"));
        assert_eq!(shelf_window_for("7d"), Some("weekly"));
        assert_eq!(shelf_window_for("30m"), None);
        assert_eq!(shelf_window_for("1h"), None);
    }

    #[test]
    fn render_tabs_html_marks_the_active_tab_and_disables_non_live_windows() {
        let html = render_tabs_html("24h");
        assert!(html.contains(r#"data-win="30m" disabled"#));
        assert!(html.contains(r#"data-win="1h" disabled"#));
        assert!(html.contains(r#"class="rep1-tab active" data-win="24h">"#));
        assert!(html.contains(r#"class="rep1-tab" data-win="7d">"#));
    }
}
