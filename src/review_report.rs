//! Review report surface (glass-902): narration before raw diff, rendered as
//! a `glance_catalog` REPORT composition. This module owns the view model and
//! the deterministic reviewer-agent sanity check; live PR/card ingestion can
//! feed the same model later without adding another report renderer.

use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use chrono::{DateTime, Utc};
use glance_catalog::leaf::{Diff, Metric};
use glance_catalog::structural::{
    Cell, CellValue, ColumnSpec, Disclosure, Hero, Narrative, NarrativeStatus, Row, Table,
};
use glance_catalog::{
    Component, InlineNode, REPORT, RenderContext, render_component, validate_layout,
};

use crate::{needs_you, sanctum_url, shell};

#[derive(Debug, Clone)]
struct ReviewReport {
    id: &'static str,
    generated_at: &'static str,
    title: &'static str,
    pull_request: PullRequestRef,
    base_sha: &'static str,
    files: Vec<FileNarration>,
    ticket: TicketContext,
    vision: VisionContext,
    gates: Vec<GateClaim>,
}

#[derive(Debug, Clone)]
struct PullRequestRef {
    repo: &'static str,
    number: &'static str,
    url: &'static str,
}

#[derive(Debug, Clone)]
struct FileNarration {
    path: &'static str,
    diff: &'static str,
    claims: Vec<NarratedClaim>,
}

#[derive(Debug, Clone)]
struct NarratedClaim {
    nature: &'static str,
    reason: &'static str,
    signatures: Vec<&'static str>,
    citation: &'static str,
    evidence_terms: Vec<&'static str>,
}

#[derive(Debug, Clone)]
struct TicketContext {
    card_id: &'static str,
    criteria: Vec<AcceptanceClaim>,
}

#[derive(Debug, Clone)]
struct AcceptanceClaim {
    criterion: &'static str,
    status: &'static str,
    note: &'static str,
    citation: &'static str,
}

#[derive(Debug, Clone)]
struct VisionContext {
    reference: &'static str,
    verdict: &'static str,
    note: &'static str,
}

#[derive(Debug, Clone)]
struct GateClaim {
    command: &'static str,
    status: &'static str,
    citation: &'static str,
}

pub async fn review_sample_shell() -> Response {
    let needs_you_count = needs_you::awaiting_input_count().await;
    match render_review_shell(&sample_review(), needs_you_count) {
        Ok(html) => Html(html).into_response(),
        Err(message) => (StatusCode::INTERNAL_SERVER_ERROR, message).into_response(),
    }
}

pub(crate) fn generate_sample_review_html() -> Result<(String, String), String> {
    let report = sample_review();
    render_report_body(&report).map(|html| (report.title.to_string(), html))
}

fn render_review_shell(
    report: &ReviewReport,
    needs_you_count: Option<usize>,
) -> Result<String, String> {
    let body = render_report_body(report)?;
    let body = REVIEW_BODY
        .replace("{{BODY}}", &body)
        .replace("{{REVIEW_ID}}", &html_escape(report.id));
    Ok(shell::render_shell(shell::Shell {
        title: report.title,
        active: None,
        needs_you_count,
        sanctum_url: &sanctum_url(),
        styles: REVIEW_STYLE,
        body: &body,
        scripts: "",
    }))
}

fn render_report_body(report: &ReviewReport) -> Result<String, String> {
    reviewer_agent_sanity_check(report).map_err(|findings| findings.join("\n"))?;
    let generated_at = DateTime::parse_from_rfc3339(report.generated_at)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|err| format!("review report generated_at is invalid: {err}"))?;
    let ctx = RenderContext {
        now: generated_at,
        cite_href: &cite_href,
    };
    let components = build_components(report);
    validate_layout(&components, &REPORT)
        .map_err(|err| format!("review report refuses invalid REPORT layout: {err}"))?;
    Ok(components
        .iter()
        .map(|component| render_component(component, &ctx))
        .collect())
}

fn reviewer_agent_sanity_check(report: &ReviewReport) -> Result<(), Vec<String>> {
    let mut findings = Vec::new();
    for file in &report.files {
        if file.diff.trim().is_empty() {
            findings.push(format!("{} has no raw diff", file.path));
            continue;
        }
        for claim in &file.claims {
            if claim.citation.trim().is_empty() {
                findings.push(format!("{} claim has no file:line citation", file.path));
            }
            for term in &claim.evidence_terms {
                if !file.diff.contains(term) {
                    findings.push(format!(
                        "{} narration claims `{}` but raw diff lacks evidence term `{}`",
                        file.path, claim.nature, term
                    ));
                }
            }
        }
    }
    if findings.is_empty() {
        Ok(())
    } else {
        Err(findings)
    }
}

fn build_components(report: &ReviewReport) -> Vec<Component> {
    vec![
        Component::Hero(hero(report)),
        Component::Narrative(opening_narrative(report)),
        Component::Table(change_table(report)),
        Component::Table(ticket_table(report)),
        Component::Table(vision_table(report)),
        Component::Table(gate_table(report)),
        Component::Table(citation_table(report)),
        Component::Disclosure(raw_diff_disclosure(report)),
    ]
}

fn hero(report: &ReviewReport) -> Hero {
    Hero {
        title: "Narrated review".to_string(),
        summary: vec![
            InlineNode::Text {
                text: format!("{} for ", report.title),
            },
            InlineNode::Cite {
                text: format!(
                    "{}#{}",
                    report.pull_request.repo, report.pull_request.number
                ),
                ref_id: report.pull_request.url.to_string(),
            },
            InlineNode::Text {
                text: " at base ".to_string(),
            },
            InlineNode::Cite {
                text: report.base_sha.to_string(),
                ref_id: format!("sha:{}", report.base_sha),
            },
            InlineNode::Text {
                text: ", checked against ".to_string(),
            },
            InlineNode::Cite {
                text: report.ticket.card_id.to_string(),
                ref_id: report.ticket.card_id.to_string(),
            },
            InlineNode::Text {
                text: " and ".to_string(),
            },
            InlineNode::Cite {
                text: report.vision.reference.to_string(),
                ref_id: report.vision.reference.to_string(),
            },
            InlineNode::Text {
                text: ".".to_string(),
            },
        ],
        stats: vec![
            Metric {
                label: "Files".to_string(),
                value: report.files.len().to_string(),
            },
            Metric {
                label: "Criteria".to_string(),
                value: report.ticket.criteria.len().to_string(),
            },
            Metric {
                label: "Reviewer sanity".to_string(),
                value: "pass".to_string(),
            },
            Metric {
                label: "Raw diff".to_string(),
                value: "disclosed".to_string(),
            },
        ],
        image_intent: None,
    }
}

fn opening_narrative(report: &ReviewReport) -> Narrative {
    Narrative {
        heading: "Operator read".to_string(),
        status: NarrativeStatus::Ok {
            paragraphs: vec![
                vec![
                    InlineNode::Text {
                        text: "The first read is the narrated surface: file-by-file nature, reason, and signatures; the raw diff remains collapsed until needed."
                            .to_string(),
                    },
                    InlineNode::Cite {
                        text: "src/lib.rs:777".to_string(),
                        ref_id: "src/lib.rs:777".to_string(),
                    },
                ],
                vec![
                    InlineNode::Text {
                        text: "The sample verdict is read-only: Glass shows enough context to approve or reject elsewhere, but exposes no approve, merge, reply, or feedback action."
                            .to_string(),
                    },
                    InlineNode::Cite {
                        text: report.ticket.card_id.to_string(),
                        ref_id: report.ticket.card_id.to_string(),
                    },
                ],
            ],
        },
    }
}

fn change_table(report: &ReviewReport) -> Table {
    let columns = vec![
        column("file", "File", false, true),
        column("nature", "Nature", false, false),
        column("reason", "Reason", false, false),
        column("signatures", "Signatures", false, false),
        column("citation", "Citation", false, false),
    ];
    let rows = report
        .files
        .iter()
        .flat_map(|file| {
            file.claims.iter().map(|claim| Row {
                cells: vec![
                    text_cell("file", file.path),
                    text_cell("nature", claim.nature),
                    text_cell("reason", claim.reason),
                    text_cell("signatures", claim.signatures.join(", ")),
                    link_cell("citation", claim.citation, cite_href(claim.citation)),
                ],
            })
        })
        .collect();
    Table {
        heading: "Change context".to_string(),
        columns,
        rows,
        empty_note: None,
        demoted_note: None,
    }
}

fn ticket_table(report: &ReviewReport) -> Table {
    let columns = vec![
        column("criterion", "Criterion", false, true),
        column("status", "Status", false, false),
        column("note", "Note", false, false),
        column("card", "Card", false, false),
    ];
    let rows = report
        .ticket
        .criteria
        .iter()
        .map(|criterion| Row {
            cells: vec![
                text_cell("criterion", criterion.criterion),
                text_cell("status", criterion.status),
                text_cell("note", criterion.note),
                link_cell("card", criterion.citation, cite_href(criterion.citation)),
            ],
        })
        .collect();
    Table {
        heading: "Powder ticket".to_string(),
        columns,
        rows,
        empty_note: None,
        demoted_note: None,
    }
}

fn vision_table(report: &ReviewReport) -> Table {
    Table {
        heading: "VISION.md alignment".to_string(),
        columns: vec![
            column("reference", "Reference", false, true),
            column("verdict", "Verdict", false, false),
            column("note", "Why it matters", false, false),
        ],
        rows: vec![Row {
            cells: vec![
                link_cell(
                    "reference",
                    report.vision.reference,
                    cite_href(report.vision.reference),
                ),
                text_cell("verdict", report.vision.verdict),
                text_cell("note", report.vision.note),
            ],
        }],
        empty_note: None,
        demoted_note: None,
    }
}

fn gate_table(report: &ReviewReport) -> Table {
    Table {
        heading: "Gate evidence".to_string(),
        columns: vec![
            column("command", "Command", false, true),
            column("status", "Status", false, false),
            column("citation", "Citation", false, false),
        ],
        rows: report
            .gates
            .iter()
            .map(|gate| Row {
                cells: vec![
                    text_cell("command", gate.command),
                    text_cell("status", gate.status),
                    link_cell("citation", gate.citation, cite_href(gate.citation)),
                ],
            })
            .collect(),
        empty_note: None,
        demoted_note: None,
    }
}

fn citation_table(report: &ReviewReport) -> Table {
    let mut rows = vec![
        citation_row(
            "Change",
            report.pull_request.url,
            "Sample review fixture PR/diff",
        ),
        citation_row("Change", &format!("sha:{}", report.base_sha), "Base SHA"),
        citation_row("Ticket", report.ticket.card_id, "Powder card"),
        citation_row("Vision", report.vision.reference, "North-star reference"),
    ];
    for file in &report.files {
        for claim in &file.claims {
            rows.push(citation_row("File", claim.citation, file.path));
        }
    }
    for gate in &report.gates {
        rows.push(citation_row("Gate", gate.citation, gate.command));
    }
    Table {
        heading: "Citation index".to_string(),
        columns: vec![
            column("layer", "Layer", false, false),
            column("source", "Source", false, true),
            column("claim", "Anchors", false, false),
        ],
        rows,
        empty_note: None,
        demoted_note: None,
    }
}

fn raw_diff_disclosure(report: &ReviewReport) -> Disclosure {
    Disclosure {
        heading: "Raw diff".to_string(),
        children: vec![Component::Diff(Diff {
            unified: report
                .files
                .iter()
                .map(|file| file.diff)
                .collect::<Vec<_>>()
                .join("\n"),
        })],
    }
}

fn column(key: &str, label: &str, numeric: bool, emphasize: bool) -> ColumnSpec {
    ColumnSpec {
        key: key.to_string(),
        label: label.to_string(),
        numeric,
        emphasize,
    }
}

fn text_cell(key: &str, text: impl Into<String>) -> Cell {
    Cell {
        column_key: key.to_string(),
        value: CellValue::Text { text: text.into() },
    }
}

fn link_cell(key: &str, text: impl Into<String>, href: impl Into<String>) -> Cell {
    Cell {
        column_key: key.to_string(),
        value: CellValue::Link {
            text: text.into(),
            href: href.into(),
        },
    }
}

fn citation_row(layer: &str, source: &str, claim: &str) -> Row {
    Row {
        cells: vec![
            text_cell("layer", layer),
            link_cell("source", source, cite_href(source)),
            text_cell("claim", claim),
        ],
    }
}

fn cite_href(ref_id: &str) -> String {
    format!("#cite-{}", citation_anchor(ref_id))
}

fn citation_anchor(ref_id: &str) -> String {
    ref_id
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect()
}

fn html_escape(raw: &str) -> String {
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

fn sample_review() -> ReviewReport {
    ReviewReport {
        id: "sample",
        generated_at: "2026-07-07T12:00:00Z",
        title: "Review surface sample diff",
        pull_request: PullRequestRef {
            repo: "misty-step/glass",
            number: "fixture-902",
            url: "fixture://glass-902/sample-diff",
        },
        base_sha: "983a04f",
        files: vec![
            FileNarration {
                path: "src/lib.rs",
                diff: LIB_ROUTE_DIFF,
                claims: vec![NarratedClaim {
                    nature: "Adds the review route without changing the frozen surface-kind contract.",
                    reason: "Operators need a stable URL for the narrated review before they choose to inspect raw diff.",
                    signatures: vec!["mod review_report", "GET /review/sample"],
                    citation: "src/lib.rs:777",
                    evidence_terms: vec!["mod review_report", "/review/sample"],
                }],
            },
            FileNarration {
                path: "src/review_report.rs",
                diff: REVIEW_MODULE_DIFF,
                claims: vec![NarratedClaim {
                    nature: "Builds a catalog REPORT from hero, narrative, tables, and a raw-diff disclosure.",
                    reason: "The card explicitly says this is a report type, not a bespoke renderer.",
                    signatures: vec![
                        "build_components",
                        "validate_layout(&components, &REPORT)",
                        "reviewer_agent_sanity_check",
                    ],
                    citation: "src/review_report.rs:1",
                    evidence_terms: vec![
                        "fn build_components",
                        "validate_layout(&components, &REPORT)",
                        "fn reviewer_agent_sanity_check",
                    ],
                }],
            },
        ],
        ticket: TicketContext {
            card_id: "glass-902",
            criteria: vec![
                AcceptanceClaim {
                    criterion: "One PR/diff can be reviewed without opening raw diff first.",
                    status: "satisfied by fixture route",
                    note: "/review/sample leads with narrative/tables and keeps raw diff collapsed.",
                    citation: "glass-902",
                },
                AcceptanceClaim {
                    criterion: "All three context layers present and cited.",
                    status: "satisfied",
                    note: "Change file:line, Powder card id, and VISION.md ref all render in the report.",
                    citation: "glass-902",
                },
                AcceptanceClaim {
                    criterion: "Wrong narration is caught by reviewer-agent sanity check.",
                    status: "satisfied by seeded test",
                    note: "The checker rejects claims whose required evidence term is absent from that file's diff.",
                    citation: "glass-902",
                },
            ],
        },
        vision: VisionContext {
            reference: "VISION.md#live-stage",
            verdict: "advances the north star",
            note: "Glass stays the operator's live stage: read-only, evidence-rich, and one-way.",
        },
        gates: vec![
            GateClaim {
                command: "./scripts/check.sh",
                status: "repo gate required before PR",
                citation: "AGENTS.md#gate",
            },
            GateClaim {
                command: "cargo build --release --locked",
                status: "release build required before PR",
                citation: "glass-902#hard-contract",
            },
        ],
    }
}

const LIB_ROUTE_DIFF: &str = r#"diff --git a/src/lib.rs b/src/lib.rs
@@
 mod rep1;
+mod review_report;
 mod window_report;
@@
         .route("/rep1", get(rep1::rep1_shell))
         .route("/api/rep1/{window}", get(rep1::rep1_report))
+        .route("/review/sample", get(review_report::review_sample_shell))
         .route("/backlog/{repo}", get(backlog_report::backlog_shell))
"#;

const REVIEW_MODULE_DIFF: &str = r#"diff --git a/src/review_report.rs b/src/review_report.rs
new file mode 100644
@@
+fn build_components(report: &ReviewReport) -> Vec<Component> {
+    vec![
+        Component::Hero(hero(report)),
+        Component::Narrative(opening_narrative(report)),
+        Component::Table(change_table(report)),
+        Component::Table(ticket_table(report)),
+        Component::Table(vision_table(report)),
+        Component::Table(gate_table(report)),
+        Component::Table(citation_table(report)),
+        Component::Disclosure(raw_diff_disclosure(report)),
+    ]
+}
+
+fn render_report_body(report: &ReviewReport) -> Result<String, String> {
+    reviewer_agent_sanity_check(report).map_err(|findings| findings.join("\n"))?;
+    let components = build_components(report);
+    validate_layout(&components, &REPORT)?;
+    Ok(components.iter().map(|component| render_component(component, &ctx)).collect())
+}
+
+fn reviewer_agent_sanity_check(report: &ReviewReport) -> Result<(), Vec<String>> {
+    // A claim must point at evidence terms present in that file's raw diff.
+}
"#;

const REVIEW_STYLE: &str = r#"
.review-shell { max-width: 920px; margin: 0 auto; padding: var(--ae-space-6) var(--ae-space-5); }
.review-status { margin: 0 0 var(--ae-space-5); padding: var(--ae-space-3) var(--ae-space-4); border: 1px solid var(--ae-line); color: var(--ae-ink-muted); font-size: 13px; }
.review-status b { color: var(--ae-ink); }
.review-body { display: grid; gap: var(--ae-space-5); }
.review-body .ae-disclosure { margin-top: var(--ae-space-2); }
"#;

const REVIEW_BODY: &str = r#"
    <div class="review-shell" data-review-id="{{REVIEW_ID}}">
      <p class="review-status" data-reviewer-sanity="pass"><b>Reviewer sanity check:</b> passed. This is a read-only surface; approve, merge, or reply elsewhere.</p>
      <div class="review-body">{{BODY}}</div>
    </div>
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_components_validate_as_report_profile() {
        let components = build_components(&sample_review());
        validate_layout(&components, &REPORT).expect("REPORT layout");
        assert!(matches!(components.first(), Some(Component::Hero(_))));
        assert!(matches!(components.last(), Some(Component::Disclosure(_))));
    }

    #[test]
    fn render_report_body_carries_all_three_context_citations() {
        let html = render_report_body(&sample_review()).expect("render report");
        assert!(html.contains("Change context"));
        assert!(html.contains("src/lib.rs:777"));
        assert!(html.contains("Powder ticket"));
        assert!(html.contains("glass-902"));
        assert!(html.contains("VISION.md#live-stage"));
        assert!(html.contains("Raw diff"));
    }

    #[test]
    fn reviewer_agent_sanity_check_rejects_wrong_narration_seed() {
        let mut report = sample_review();
        report.files[0].claims.push(NarratedClaim {
            nature: "Adds an in-Glass approval button.",
            reason: "This is deliberately wrong; Glass is one-way.",
            signatures: vec!["approveReviewButton"],
            citation: "src/lib.rs:777",
            evidence_terms: vec!["approveReviewButton"],
        });

        let findings = reviewer_agent_sanity_check(&report).expect_err("wrong narration must fail");
        assert!(
            findings
                .iter()
                .any(|finding| finding.contains("approveReviewButton")),
            "missing planted false-claim finding: {findings:?}"
        );
    }

    #[test]
    fn reviewer_agent_sanity_check_accepts_the_sample_fixture() {
        reviewer_agent_sanity_check(&sample_review()).expect("sample fixture is internally cited");
    }
}
