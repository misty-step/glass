//! Backlog intelligence (glass-914): "what is the state of X repo's
//! backlog" answered from Glass alone. Sibling to REP-1 (glass-913) --
//! same pipeline in spirit (collect -> pack -> catalog render), but this
//! report needs no model synthesis stage at all: every fact rendered is a
//! live Powder card's own field, not a paraphrase that would need a
//! citation gate to trust. Deliberately NOT a second synthesis engine
//! (glass-917's whole point) -- there is no narrative prose here to
//! hallucinate, so there is nothing for a citation gate to guard.
//!
//! Rendering goes through the same `glance_catalog::render` used by REP-1;
//! no new component vocabulary, no new renderer.

use std::collections::BTreeMap;
use std::convert::Infallible;

use axum::extract::Path as AxumPath;
use axum::response::IntoResponse;
use axum::response::sse::{Event, KeepAlive, Sse};
use chrono::Utc;
use glance_catalog::leaf::Metric;
use glance_catalog::structural::{Cell, CellValue, ColumnSpec, Hero, Row, Table};
use glance_catalog::{Component, InlineNode, RenderContext, render_component};
use serde::Deserialize;
use serde_json::json;

/// A card counts as stale if untouched for this long -- long enough that a
/// card genuinely being worked wouldn't trip it, short enough to surface
/// backlog rot the operator asked to see without manually sifting tickets.
const STALE_THRESHOLD_SECONDS: i64 = 14 * 24 * 60 * 60;

fn powder_base_url() -> Option<String> {
    std::env::var("GLASS_POWDER_API_BASE_URL")
        .ok()
        .filter(|value| !value.is_empty())
}

fn powder_api_key() -> Option<String> {
    std::env::var("GLASS_POWDER_API_KEY")
        .ok()
        .filter(|value| !value.is_empty())
}

#[derive(Debug, Deserialize, Clone)]
struct RawCard {
    id: String,
    title: String,
    status: String,
    priority: String,
    #[serde(default)]
    blocked_by: Vec<String>,
    updated_at: i64,
}

#[derive(Debug, Deserialize)]
struct ListCardsResponse {
    cards: Vec<RawCard>,
}

async fn fetch_cards(repo: &str) -> Result<Vec<RawCard>, String> {
    let base = match powder_base_url() {
        Some(base) => base,
        None => {
            crate::canary::report_error(
                "glass.backlog.fetch.failed",
                "route=/api/backlog/{repo} upstream=powder error_kind=missing_base_url",
            );
            return Err("GLASS_POWDER_API_BASE_URL is not configured".to_string());
        }
    };
    let key = match powder_api_key() {
        Some(key) => key,
        None => {
            crate::canary::report_error(
                "glass.backlog.fetch.failed",
                "route=/api/backlog/{repo} upstream=powder error_kind=missing_api_key",
            );
            return Err("GLASS_POWDER_API_KEY is not configured".to_string());
        }
    };
    let url = format!(
        "{}/api/v1/cards?repo={repo}&limit=500",
        base.trim_end_matches('/')
    );
    let response = reqwest::Client::new()
        .get(&url)
        .bearer_auth(key)
        .send()
        .await
        .map_err(|err| {
            crate::canary::report_error(
                "glass.backlog.fetch.failed",
                "route=/api/backlog/{repo} upstream=powder error_kind=transport",
            );
            format!("fetch {url}: {err}")
        })?;
    if !response.status().is_success() {
        crate::canary::report_error(
            "glass.backlog.fetch.failed",
            &format!(
                "route=/api/backlog/{{repo}} upstream=powder upstream_status={} error_kind=upstream_status",
                response.status().as_u16()
            ),
        );
        return Err(format!(
            "fetch {url}: upstream returned {}",
            response.status()
        ));
    }
    response
        .json::<ListCardsResponse>()
        .await
        .map(|body| body.cards)
        .map_err(|err| {
            crate::canary::report_error(
                "glass.backlog.fetch.failed",
                "route=/api/backlog/{repo} upstream=powder error_kind=parse",
            );
            format!("parse {url}: {err}")
        })
}

fn text(s: impl Into<String>) -> Vec<InlineNode> {
    vec![InlineNode::Text { text: s.into() }]
}

/// Builds the deterministic backlog report: a stat hero (total, by-priority,
/// stale, blocked, done%) plus one row-per-card table -- both rendered
/// through the shared catalog, no narrative synthesis stage.
fn build_components(repo: &str, cards: &[RawCard]) -> Vec<Component> {
    let now = Utc::now().timestamp();
    let total = cards.len();
    let done = cards.iter().filter(|c| c.status == "done").count();
    let stale = cards
        .iter()
        .filter(|c| c.status != "done" && now - c.updated_at > STALE_THRESHOLD_SECONDS)
        .count();
    let blocked = cards.iter().filter(|c| !c.blocked_by.is_empty()).count();
    let mut by_priority: BTreeMap<String, usize> = BTreeMap::new();
    for card in cards {
        *by_priority.entry(card.priority.clone()).or_default() += 1;
    }
    let progress_pct = (done * 100).checked_div(total).unwrap_or(0);

    let mut stats = vec![
        Metric {
            label: "Total cards".to_string(),
            value: total.to_string(),
        },
        Metric {
            label: "Done".to_string(),
            value: format!("{progress_pct}%"),
        },
        Metric {
            label: "Stale (>14d)".to_string(),
            value: stale.to_string(),
        },
        Metric {
            label: "Blocked".to_string(),
            value: blocked.to_string(),
        },
    ];
    for (priority, count) in &by_priority {
        stats.push(Metric {
            label: priority.to_uppercase(),
            value: count.to_string(),
        });
    }

    let hero = Component::Hero(Hero {
        title: format!("{repo} backlog"),
        summary: text(format!(
            "{total} card(s) tracked; {done} done, {stale} stale, {blocked} blocked."
        )),
        stats,
        image_intent: None,
    });

    let columns = vec![
        ColumnSpec {
            key: "id".to_string(),
            label: "Card".to_string(),
            numeric: false,
            emphasize: true,
        },
        ColumnSpec {
            key: "title".to_string(),
            label: "Title".to_string(),
            numeric: false,
            emphasize: false,
        },
        ColumnSpec {
            key: "status".to_string(),
            label: "Status".to_string(),
            numeric: false,
            emphasize: false,
        },
        ColumnSpec {
            key: "priority".to_string(),
            label: "Priority".to_string(),
            numeric: false,
            emphasize: false,
        },
        ColumnSpec {
            key: "flags".to_string(),
            label: "Flags".to_string(),
            numeric: false,
            emphasize: false,
        },
    ];

    let mut sorted_cards: Vec<&RawCard> = cards.iter().collect();
    sorted_cards.sort_by(|a, b| a.id.cmp(&b.id));

    let table = if sorted_cards.is_empty() {
        Table {
            heading: "Cards".to_string(),
            columns,
            rows: vec![],
            empty_note: Some(format!("No Powder cards found for repo \"{repo}\".")),
            demoted_note: None,
        }
    } else {
        let rows = sorted_cards
            .iter()
            .map(|card| {
                let mut flags = Vec::new();
                if card.status != "done" && now - card.updated_at > STALE_THRESHOLD_SECONDS {
                    flags.push("stale".to_string());
                }
                if !card.blocked_by.is_empty() {
                    flags.push(format!("blocked by {}", card.blocked_by.join(", ")));
                }
                Row {
                    cells: vec![
                        Cell {
                            column_key: "id".to_string(),
                            value: CellValue::Text {
                                text: card.id.clone(),
                            },
                        },
                        Cell {
                            column_key: "title".to_string(),
                            value: CellValue::Text {
                                text: card.title.clone(),
                            },
                        },
                        Cell {
                            column_key: "status".to_string(),
                            value: CellValue::Text {
                                text: card.status.clone(),
                            },
                        },
                        Cell {
                            column_key: "priority".to_string(),
                            value: CellValue::Text {
                                text: card.priority.to_uppercase(),
                            },
                        },
                        Cell {
                            column_key: "flags".to_string(),
                            value: CellValue::Text {
                                text: if flags.is_empty() {
                                    "-".to_string()
                                } else {
                                    flags.join("; ")
                                },
                            },
                        },
                    ],
                }
            })
            .collect();
        Table {
            heading: "Cards".to_string(),
            columns,
            rows,
            empty_note: None,
            demoted_note: None,
        }
    };

    vec![hero, Component::Table(table)]
}

fn render_all(repo: &str, cards: &[RawCard]) -> String {
    let ctx = RenderContext {
        now: Utc::now(),
        cite_href: &|ref_id| format!("#cite-{ref_id}"),
    };
    build_components(repo, cards)
        .iter()
        .map(|component| render_component(component, &ctx))
        .collect()
}

pub(crate) async fn generate_backlog_html(repo: &str) -> Result<(String, usize), String> {
    let cards = fetch_cards(repo).await?;
    let count = cards.len();
    Ok((render_all(repo, &cards), count))
}

/// `GET /api/backlog/{repo}`. Streams a skeleton event, then a full event
/// whose `html` is pre-rendered through `glance_catalog::render` from a live
/// Powder `list_cards` call -- deterministic aggregation, no model call, no
/// second synthesis engine.
pub async fn backlog_report(AxumPath(repo): AxumPath<String>) -> impl IntoResponse {
    let stream = async_stream::stream! {
        yield Ok::<_, Infallible>(
            Event::default()
                .event("skeleton")
                .data(json!({"stage": "skeleton", "repo": repo}).to_string()),
        );

        match generate_backlog_html(&repo).await {
            Ok((html, count)) => {
                yield Ok::<_, Infallible>(
                    Event::default().event("full").data(
                        json!({"stage": "full", "repo": repo, "count": count, "html": html})
                            .to_string(),
                    ),
                );
            }
            Err(message) => {
                yield Ok::<_, Infallible>(
                    Event::default().event("error").data(
                        json!({"stage": "error", "repo": repo, "message": message}).to_string(),
                    ),
                );
            }
        }
    };
    Sse::new(stream).keep_alive(KeepAlive::default())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(
        id: &str,
        status: &str,
        priority: &str,
        blocked_by: Vec<&str>,
        age_days: i64,
    ) -> RawCard {
        RawCard {
            id: id.to_string(),
            title: format!("{id} title"),
            status: status.to_string(),
            priority: priority.to_string(),
            blocked_by: blocked_by.into_iter().map(String::from).collect(),
            updated_at: Utc::now().timestamp() - age_days * 86_400,
        }
    }

    #[test]
    fn build_components_computes_stale_blocked_and_progress_stats() {
        let cards = vec![
            card("glass-1", "done", "p1", vec![], 30),
            card("glass-2", "ready", "p2", vec![], 30),
            card("glass-3", "ready", "p2", vec!["glass-2"], 1),
        ];
        let components = build_components("glass", &cards);
        let Component::Hero(hero) = &components[0] else {
            panic!("expected hero first");
        };
        let find = |label: &str| {
            hero.stats
                .iter()
                .find(|s| s.label == label)
                .unwrap()
                .value
                .clone()
        };
        assert_eq!(find("Total cards"), "3");
        assert_eq!(find("Done"), "33%");
        assert_eq!(
            find("Stale (>14d)"),
            "1",
            "only glass-2 is non-done and >14d stale"
        );
        assert_eq!(find("Blocked"), "1");
    }

    #[test]
    fn build_components_renders_an_explicit_empty_state_for_zero_cards() {
        let components = build_components("empty-repo", &[]);
        let Component::Table(table) = &components[1] else {
            panic!("expected table second");
        };
        assert!(table.rows.is_empty());
        assert!(table.empty_note.is_some());
    }

    #[test]
    fn render_all_produces_html_with_hero_and_table_markers() {
        let cards = vec![card("glass-1", "ready", "p1", vec![], 1)];
        let html = render_all("glass", &cards);
        assert!(html.contains(r#"data-glance-component="hero""#));
        assert!(html.contains(r#"data-glance-component="table""#));
        assert!(html.contains("glass-1"));
    }
}
