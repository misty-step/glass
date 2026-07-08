use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum ReportComponent {
    Hero {
        kicker: String,
        headline: String,
        figures: Vec<Figure>,
        trend: Vec<SeriesPoint>,
        #[serde(default)]
        peak_label: Option<String>,
    },
    StatBand {
        figures: Vec<Figure>,
    },
    Spark {
        series: Vec<SeriesPoint>,
    },
    Bars {
        series: Vec<SeriesPoint>,
    },
    Meters {
        pairs: Vec<MeterPair>,
    },
    Pipeline {
        stages: Vec<PipelineStage>,
    },
    Trail {
        events: Vec<TrailEvent>,
    },
    Callouts {
        lines: Vec<StatusLine>,
    },
    EvidenceChips {
        links: Vec<EvidenceLink>,
    },
    DiffExhibit {
        file: String,
        lines: Vec<DiffLine>,
    },
    TerminalExhibit {
        lines: Vec<String>,
    },
    PullQuote {
        text: String,
        by: Option<String>,
    },
    BadgeRow {
        badges: Vec<Badge>,
    },
    IconRow {
        rows: Vec<IconRowItem>,
    },
    Prose {
        text: String,
    },
    FigCaption {
        text: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct Figure {
    pub(crate) value: String,
    pub(crate) label: String,
    #[serde(default)]
    pub(crate) warn: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct SeriesPoint {
    pub(crate) label: String,
    pub(crate) value: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct MeterPair {
    pub(crate) label: String,
    pub(crate) value: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct PipelineStage {
    pub(crate) label: String,
    pub(crate) state: PipelineState,
    #[serde(default)]
    pub(crate) note: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PipelineState {
    Done,
    Active,
    Blocked,
    Pending,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct TrailEvent {
    pub(crate) time: String,
    pub(crate) title: String,
    #[serde(default)]
    pub(crate) kind: Option<String>,
    #[serde(default)]
    pub(crate) agent: Option<String>,
    #[serde(default)]
    pub(crate) href: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct StatusLine {
    pub(crate) text: String,
    #[serde(default)]
    pub(crate) status: Option<String>,
    #[serde(default)]
    pub(crate) href: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct EvidenceLink {
    pub(crate) label: String,
    pub(crate) href: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct DiffLine {
    pub(crate) state: DiffState,
    pub(crate) text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum DiffState {
    Add,
    Del,
    Ctx,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct Badge {
    pub(crate) label: String,
    #[serde(default)]
    pub(crate) value: Option<String>,
    #[serde(default)]
    pub(crate) status: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct IconRowItem {
    pub(crate) text: String,
    #[serde(default)]
    pub(crate) icon: Option<String>,
    #[serde(default)]
    pub(crate) meta: Option<String>,
}

pub(crate) fn render_components(components: &[ReportComponent]) -> String {
    components.iter().map(render_component).collect()
}

pub(crate) fn render_component(component: &ReportComponent) -> String {
    match component {
        ReportComponent::Hero {
            kicker,
            headline,
            figures,
            trend,
            peak_label,
        } => hero(kicker, headline, figures, trend, peak_label.as_deref()),
        ReportComponent::StatBand { figures } => stat_band(figures),
        ReportComponent::Spark { series } => spark(series),
        ReportComponent::Bars { series } => bars(series),
        ReportComponent::Meters { pairs } => meters(pairs),
        ReportComponent::Pipeline { stages } => pipeline(stages),
        ReportComponent::Trail { events } => trail(events),
        ReportComponent::Callouts { lines } => callouts(lines),
        ReportComponent::EvidenceChips { links } => evidence_chips(links),
        ReportComponent::DiffExhibit { file, lines } => diff_exhibit(file, lines),
        ReportComponent::TerminalExhibit { lines } => terminal_exhibit(lines),
        ReportComponent::PullQuote { text, by } => pull_quote(text, by.as_deref()),
        ReportComponent::BadgeRow { badges } => badge_row(badges),
        ReportComponent::IconRow { rows } => icon_row(rows),
        ReportComponent::Prose { text } => prose(text),
        ReportComponent::FigCaption { text } => fig_caption(text),
    }
}

fn hero(
    kicker: &str,
    headline: &str,
    figures: &[Figure],
    trend: &[SeriesPoint],
    peak_label: Option<&str>,
) -> String {
    let trend_html = hero_trend(trend, peak_label);
    format!(
        r#"<header class="glass-rep-hero"><p class="ae-plate-cap">{kicker}</p><h2>{headline}</h2>{stats}{trend_html}</header>"#,
        kicker = html_escape(kicker),
        headline = html_escape(headline),
        stats = stat_band(figures),
    )
}

fn hero_trend(series: &[SeriesPoint], peak_label: Option<&str>) -> String {
    if series.is_empty() {
        return String::new();
    }
    let points = spark_points(series);
    let aria = series_aria_label(series);
    let peak = peak_label
        .map(str::to_string)
        .unwrap_or_else(|| default_peak_label(series));
    format!(
        r#"<div class="glass-rep-hero-trend"><span class="ae-h">VELOCITY</span><svg class="ae-spark glass-rep-sparkline glass-rep-hero-spark" viewBox="0 0 100 24" preserveAspectRatio="none" role="img" aria-label="{aria}"><polyline points="{points}"></polyline></svg><span class="ae-dim">{peak}</span></div>"#,
        aria = html_escape(&aria),
        peak = html_escape(&peak),
    )
}

fn stat_band(figures: &[Figure]) -> String {
    let items = figures
        .iter()
        .map(|figure| {
            let warn = if figure.warn { " is-warn" } else { "" };
            format!(
                r#"<span class="ae-stat-badge glass-rep-stat{warn}"><span class="ae-num">{value}</span><span>{label}</span></span>"#,
                value = html_escape(&figure.value),
                label = html_escape(&figure.label),
            )
        })
        .collect::<String>();
    format!(r#"<div class="ae-stat-badges glass-rep-stat-band">{items}</div>"#)
}

fn spark(series: &[SeriesPoint]) -> String {
    if series.is_empty() {
        return r#"<div class="glass-rep-empty">No series.</div>"#.to_string();
    }
    let points = spark_points(series);
    let label = series_aria_label(series);
    format!(
        r#"<figure class="glass-rep-figure glass-rep-spark"><svg class="ae-spark glass-rep-sparkline" viewBox="0 0 100 24" preserveAspectRatio="none" role="img" aria-label="{label}"><polyline points="{points}"></polyline></svg></figure>"#,
        label = html_escape(&label),
    )
}

fn bars(series: &[SeriesPoint]) -> String {
    if series.is_empty() {
        return r#"<div class="glass-rep-empty">No bars.</div>"#.to_string();
    }
    let max = series
        .iter()
        .map(|point| point.value)
        .fold(0.0_f64, f64::max)
        .max(1.0);
    let body = series
        .iter()
        .map(|point| {
            let pct = ((point.value / max) * 100.0).round().clamp(0.0, 100.0);
            let peak = if (point.value - max).abs() < f64::EPSILON {
                " is-peak"
            } else {
                ""
            };
            format!(
                r#"<span class="glass-rep-bar-col" title="{label}: {value}"><span class="glass-rep-bar-track"><span class="glass-rep-bar-fill{peak}" style="height:{pct}%"></span></span><span class="glass-rep-bar-label">{label}</span></span>"#,
                label = html_escape(&point.label),
                value = html_escape(&compact_number(point.value)),
            )
        })
        .collect::<String>();
    format!(r#"<div class="glass-rep-bars" role="img" aria-label="bar chart">{body}</div>"#)
}

fn meters(pairs: &[MeterPair]) -> String {
    if pairs.is_empty() {
        return r#"<div class="glass-rep-empty">No meters.</div>"#.to_string();
    }
    let max = pairs
        .iter()
        .map(|pair| pair.value)
        .fold(0.0_f64, f64::max)
        .max(1.0);
    let rows = pairs
        .iter()
        .map(|pair| {
            let pct = ((pair.value / max) * 100.0).round().clamp(0.0, 100.0);
            format!(
                r#"<div class="glass-rep-meter-row"><span class="ae-num glass-rep-meter-label">{label}</span><span class="ae-meter"><span class="ae-meter-fill" style="width:{pct}%"></span></span><span class="ae-num ae-strong glass-rep-meter-value">{value}</span></div>"#,
                label = html_escape(&pair.label),
                value = html_escape(&compact_number(pair.value)),
            )
        })
        .collect::<String>();
    format!(r#"<div class="glass-rep-meters">{rows}</div>"#)
}

fn pipeline(stages: &[PipelineStage]) -> String {
    if stages.is_empty() {
        return r#"<div class="glass-rep-empty">No pipeline stages.</div>"#.to_string();
    }
    let body = stages
        .iter()
        .map(|stage| {
            let state = stage.state.as_str();
            let note = stage
                .note
                .as_deref()
                .map(|note| {
                    format!(
                        r#"<span class="glass-rep-stage-note">{}</span>"#,
                        html_escape(note)
                    )
                })
                .unwrap_or_default();
            format!(
                r#"<div class="glass-rep-stage is-{state}"><span class="glass-rep-stage-head"><span class="glass-rep-stage-glyph">{icon}</span><span class="glass-rep-stage-label">{label}</span></span>{note}</div>"#,
                icon = icon_svg(stage.state.icon()),
                label = html_escape(&stage.label),
            )
        })
        .collect::<String>();
    format!(r#"<div class="glass-rep-pipeline">{body}</div>"#)
}

fn trail(events: &[TrailEvent]) -> String {
    if events.is_empty() {
        return r#"<div class="glass-rep-empty">No trail events.</div>"#.to_string();
    }
    let rows = events
        .iter()
        .map(|event| {
            let kind = event.kind.as_deref().unwrap_or("note");
            let agent = event
                .agent
                .as_deref()
                .map(|agent| {
                    format!(
                        r#"<span class="glass-rep-trail-agent">{}</span>"#,
                        html_escape(agent)
                    )
                })
                .unwrap_or_default();
            let body = format!(
                r#"<span class="glass-rep-trail-time">{time}</span><span class="glass-rep-trail-glyph">{icon}</span><span class="glass-rep-trail-body"><span class="ae-chip ae-cat-5">{kind}</span>{agent}<span class="glass-rep-trail-title">{title}</span></span>"#,
                time = html_escape(&event.time),
                icon = icon_svg(kind),
                kind = html_escape(kind),
                title = html_escape(&event.title),
            );
            if let Some(href) = event.href.as_deref() {
                format!(
                    r#"<a class="glass-rep-trail-row" href="{href}">{body}</a>"#,
                    href = html_attr_escape(href),
                )
            } else {
                format!(r#"<div class="glass-rep-trail-row">{body}</div>"#)
            }
        })
        .collect::<String>();
    format!(r#"<div class="glass-rep-trail">{rows}</div>"#)
}

fn callouts(lines: &[StatusLine]) -> String {
    if lines.is_empty() {
        return r#"<div class="glass-rep-empty">No callouts.</div>"#.to_string();
    }
    let body = lines
        .iter()
        .map(|line| {
            let status = line.status.as_deref().unwrap_or("note");
            let inner = format!(
                r#"{icon}<span class="ae-status-label">{text}</span>"#,
                icon = icon_svg(status),
                text = html_escape(&line.text),
            );
            if let Some(href) = line.href.as_deref() {
                format!(
                    r#"<a class="ae-status glass-rep-callout is-{status}" href="{href}">{inner}</a>"#,
                    status = html_attr_escape(status),
                    href = html_attr_escape(href),
                )
            } else {
                format!(
                    r#"<span class="ae-status glass-rep-callout is-{status}">{inner}</span>"#,
                    status = html_attr_escape(status),
                )
            }
        })
        .collect::<String>();
    format!(r#"<div class="glass-rep-callouts">{body}</div>"#)
}

fn evidence_chips(links: &[EvidenceLink]) -> String {
    if links.is_empty() {
        return r#"<p class="glass-rep-evidence"><span class="glass-rep-evidence-k">evidence</span><span class="ae-dim">none</span></p>"#.to_string();
    }
    let chips = links
        .iter()
        .map(|link| {
            format!(
                r#"<a class="ae-tag" href="{href}">{label}</a>"#,
                href = html_attr_escape(&link.href),
                label = html_escape(&link.label),
            )
        })
        .collect::<String>();
    format!(
        r#"<p class="glass-rep-evidence"><span class="glass-rep-evidence-k">evidence</span>{chips}</p>"#
    )
}

fn diff_exhibit(file: &str, lines: &[DiffLine]) -> String {
    let rows = lines
        .iter()
        .map(|line| {
            let sign = match line.state {
                DiffState::Add => "+",
                DiffState::Del => "-",
                DiffState::Ctx => " ",
            };
            let cls = match line.state {
                DiffState::Add => " is-add",
                DiffState::Del => " is-del",
                DiffState::Ctx => "",
            };
            format!(
                r#"<div class="glass-rep-diff-line"><span class="glass-rep-gutter{cls}">{sign}</span><span>{text}</span></div>"#,
                text = html_escape(&line.text),
            )
        })
        .collect::<String>();
    format!(
        r#"<figure class="glass-rep-exhibit"><figcaption>diff - {file}</figcaption><div class="glass-rep-code glass-rep-scroll">{rows}</div></figure>"#,
        file = html_escape(file),
    )
}

fn terminal_exhibit(lines: &[String]) -> String {
    let rows = lines
        .iter()
        .map(|line| {
            let ok = line.starts_with("OK ") || line.starts_with("ok ") || line.starts_with("✔");
            let text = line
                .strip_prefix("OK ")
                .or_else(|| line.strip_prefix("ok "))
                .or_else(|| line.strip_prefix("✔"))
                .unwrap_or(line)
                .trim_start();
            let mark = if ok {
                r#"<span class="glass-rep-mark is-ok">ok</span>"#
            } else {
                r#"<span class="glass-rep-mark is-prompt">&gt;</span>"#
            };
            format!(
                r#"<div class="glass-rep-terminal-line">{mark}<span>{}</span></div>"#,
                html_escape(text),
            )
        })
        .collect::<String>();
    format!(
        r#"<figure class="glass-rep-exhibit"><figcaption>terminal</figcaption><div class="glass-rep-code glass-rep-scroll">{rows}</div></figure>"#
    )
}

fn pull_quote(text: &str, by: Option<&str>) -> String {
    let by = by
        .map(|by| format!(r#"<span class="ae-pull-by">{}</span>"#, html_escape(by)))
        .unwrap_or_default();
    format!(
        r#"<blockquote class="ae-pull glass-rep-pull">{text}{by}</blockquote>"#,
        text = html_escape(text),
    )
}

fn badge_row(badges: &[Badge]) -> String {
    let body = badges
        .iter()
        .map(|badge| {
            let status = badge.status.as_deref().unwrap_or("note");
            let value = badge
                .value
                .as_deref()
                .map(|value| {
                    format!(
                        r#"<span class="ae-num glass-rep-badge-value">{}</span>"#,
                        html_escape(value)
                    )
                })
                .unwrap_or_default();
            format!(
                r#"<span class="ae-chip glass-rep-badge is-{status}">{value}{label}</span>"#,
                status = html_attr_escape(status),
                label = html_escape(&badge.label),
            )
        })
        .collect::<String>();
    format!(r#"<div class="glass-rep-badge-row">{body}</div>"#)
}

fn icon_row(rows: &[IconRowItem]) -> String {
    let body = rows
        .iter()
        .map(|row| {
            let meta = row
                .meta
                .as_deref()
                .map(|meta| format!(r#"<span class="ae-dim">{}</span>"#, html_escape(meta)))
                .unwrap_or_default();
            format!(
                r#"<span class="ae-icon-row glass-rep-icon-row-item"><span class="ae-list-icon">{icon}</span><span class="ae-icon-row-main"><span>{text}</span>{meta}</span></span>"#,
                icon = icon_svg(row.icon.as_deref().unwrap_or("note")),
                text = html_escape(&row.text),
            )
        })
        .collect::<String>();
    format!(r#"<div class="glass-rep-icon-row">{body}</div>"#)
}

fn prose(text: &str) -> String {
    format!(r#"<p class="glass-rep-prose">{}</p>"#, html_escape(text))
}

fn fig_caption(text: &str) -> String {
    format!(r#"<p class="glass-rep-caption">{}</p>"#, html_escape(text))
}

fn compact_number(value: f64) -> String {
    if (value.fract()).abs() < f64::EPSILON {
        format!("{value:.0}")
    } else {
        format!("{value:.1}")
    }
}

fn spark_points(series: &[SeriesPoint]) -> String {
    let min = series
        .iter()
        .map(|point| point.value)
        .fold(f64::INFINITY, f64::min);
    let max = series
        .iter()
        .map(|point| point.value)
        .fold(f64::NEG_INFINITY, f64::max);
    let span = if (max - min).abs() < f64::EPSILON {
        1.0
    } else {
        max - min
    };
    let denom = (series.len().saturating_sub(1)).max(1) as f64;
    series
        .iter()
        .enumerate()
        .map(|(index, point)| {
            let x = index as f64 / denom * 100.0;
            let y = 21.0 - ((point.value - min) / span) * 18.0;
            format!("{x:.2},{y:.2}")
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn series_aria_label(series: &[SeriesPoint]) -> String {
    series
        .iter()
        .map(|point| format!("{} {}", point.label, compact_number(point.value)))
        .collect::<Vec<_>>()
        .join(", ")
}

fn default_peak_label(series: &[SeriesPoint]) -> String {
    series
        .iter()
        .max_by(|left, right| {
            left.value
                .partial_cmp(&right.value)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|point| format!("peak {} - {}", compact_number(point.value), point.label))
        .unwrap_or_else(|| "peak n/a".to_string())
}

impl PipelineState {
    fn as_str(self) -> &'static str {
        match self {
            Self::Done => "done",
            Self::Active => "active",
            Self::Blocked => "blocked",
            Self::Pending => "pending",
        }
    }

    fn icon(self) -> &'static str {
        match self {
            Self::Done => "ok",
            Self::Active => "active",
            Self::Blocked => "warn",
            Self::Pending => "pending",
        }
    }
}

fn icon_svg(kind: &str) -> &'static str {
    match kind {
        "ok" | "done" | "shipped" | "receipt" => {
            r#"<svg class="ae-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><circle cx="12" cy="12" r="9"></circle><path d="m8.5 12 2.5 2.5 4.5-4.5"></path></svg>"#
        }
        "warn" | "blocked" => {
            r#"<svg class="ae-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M12 3 22 20H2L12 3Z"></path><path d="M12 9v5"></path><path d="M12 17h.01"></path></svg>"#
        }
        "question" => {
            r#"<svg class="ae-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><circle cx="12" cy="12" r="9"></circle><path d="M9.5 9a2.7 2.7 0 0 1 5 1.4c0 1.8-2.5 2-2.5 3.6"></path><path d="M12 17h.01"></path></svg>"#
        }
        "active" | "report" => {
            r#"<svg class="ae-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M4 19V5"></path><path d="M8 17V9"></path><path d="M12 20V4"></path><path d="M16 16v-6"></path><path d="M20 18V7"></path></svg>"#
        }
        "pending" | "note" | "digest" => {
            r#"<svg class="ae-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M12 20h9"></path><path d="M16.5 3.5a2.1 2.1 0 0 1 3 3L8 18l-4 1 1-4z"></path></svg>"#
        }
        "release" => {
            r#"<svg class="ae-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M3 12V6a1 1 0 0 1 1-1h6l9 9-7 7-9-9z"></path><path d="M7.5 8.5h.01"></path></svg>"#
        }
        _ => {
            r#"<svg class="ae-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><circle cx="12" cy="12" r="9"></circle><path d="M12 8v4"></path><path d="M12 16h.01"></path></svg>"#
        }
    }
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

fn html_attr_escape(raw: &str) -> String {
    html_escape(raw)
}

pub(crate) const STYLE: &str = r#"
.glass-rep-hero { display: grid; gap: var(--ae-space-4); padding-bottom: var(--ae-space-5); border-bottom: 1px solid var(--ae-line); }
.glass-rep-hero h2 { margin: 0; font-size: clamp(1.5rem, 2.5vw, 2.4rem); line-height: 1.05; letter-spacing: 0; }
.glass-rep-hero .glass-rep-stat-band { margin: 0; }
.glass-rep-hero-trend { display: grid; grid-template-columns: auto minmax(0, 1fr) auto; align-items: center; gap: var(--ae-space-3); }
.glass-rep-hero-spark { height: 3.4em; }
.glass-rep-stat-band { margin: var(--ae-space-4) 0; }
.glass-rep-stat.is-warn .ae-num { color: var(--ae-warn); }
.glass-rep-figure { margin: 1.4em 0; min-width: 0; }
.glass-rep-sparkline { width: 100%; height: 2.8em; color: var(--ae-accent); }
.glass-rep-bars { display: flex; align-items: flex-end; gap: var(--ae-space-2); height: 7.5em; margin: 1.2em 0; }
.glass-rep-bar-col { flex: 1 1 0; display: flex; flex-direction: column; align-items: center; gap: 0.4em; min-width: 0; height: 100%; }
.glass-rep-bar-track { flex: 1 1 auto; width: 100%; display: flex; align-items: flex-end; }
.glass-rep-bar-fill { width: 100%; min-height: 1px; background: var(--ae-ink-muted); }
.glass-rep-bar-fill.is-peak { background: var(--ae-accent); }
.glass-rep-bar-label { max-width: 100%; font-family: var(--ae-font-mono); font-size: 11px; letter-spacing: 0; color: var(--ae-ink-faint); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.glass-rep-meters { display: grid; gap: var(--ae-space-3); margin: 1.1em 0; }
.glass-rep-meter-row { display: grid; grid-template-columns: minmax(4.5em, 8em) minmax(0, 1fr) auto; align-items: center; gap: var(--ae-space-4); }
.glass-rep-meter-label { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.glass-rep-meter-value { min-width: 1.6em; text-align: right; }
.glass-rep-pipeline { display: grid; grid-auto-flow: column; grid-auto-columns: minmax(0, 1fr); border: 1px solid var(--ae-line); margin: 1.4em 0; }
.glass-rep-stage { display: flex; flex-direction: column; gap: 0.35em; min-width: 0; padding: 0.7em 0.85em; border-left: 1px solid var(--ae-line); }
.glass-rep-stage:first-child { border-left: 0; }
.glass-rep-stage-head { display: flex; align-items: center; gap: 0.4em; min-width: 0; }
.glass-rep-stage-glyph { display: inline-flex; color: var(--ae-ink-muted); }
.glass-rep-stage.is-blocked .glass-rep-stage-glyph { color: var(--ae-warn); }
.glass-rep-stage-glyph .ae-icon { width: 1em; height: 1em; align-self: center; }
.glass-rep-stage-label { min-width: 0; font-weight: var(--ae-w-medium); overflow-wrap: anywhere; }
.glass-rep-stage-note { font-family: var(--ae-font-mono); font-size: 11px; letter-spacing: 0; color: var(--ae-ink-faint); }
.glass-rep-trail { border-top: 1px solid var(--ae-line); margin: 1.3em 0; }
.glass-rep-trail-row { display: grid; grid-template-columns: 4.4em 1.2em minmax(0, 1fr); align-items: baseline; gap: var(--ae-space-3); padding: 0.5em 0; border-bottom: 1px solid var(--ae-line); color: inherit; text-decoration: none; }
.glass-rep-trail-time { font-family: var(--ae-font-mono); font-size: 11px; letter-spacing: 0; color: var(--ae-ink-faint); }
.glass-rep-trail-glyph { display: inline-flex; align-self: center; color: var(--ae-ink-muted); }
.glass-rep-trail-glyph .ae-icon { width: 0.95em; height: 0.95em; align-self: center; }
.glass-rep-trail-body { min-width: 0; display: flex; flex-wrap: wrap; align-items: baseline; gap: 0.5em; }
.glass-rep-trail-agent { font-family: var(--ae-font-mono); font-size: 11px; letter-spacing: 0; color: var(--ae-ink-muted); }
.glass-rep-trail-title { font-size: 13px; color: var(--ae-ink); min-width: 0; }
.glass-rep-callouts { display: grid; gap: var(--ae-space-3); margin: 1.3em 0; }
.glass-rep-callout { align-items: baseline; color: inherit; text-decoration: none; }
.glass-rep-callout .ae-icon { align-self: center; }
.glass-rep-callout.is-warn .ae-icon, .glass-rep-callout.is-blocked .ae-icon { color: var(--ae-warn); }
.glass-rep-evidence { display: flex; flex-wrap: wrap; align-items: baseline; gap: 0.5em; margin: 0.7em 0 1.1em; }
.glass-rep-evidence-k { font-family: var(--ae-font-mono); font-size: 11px; letter-spacing: 0.14em; color: var(--ae-ink-faint); }
.glass-rep-exhibit { margin: 1.5em 0; min-width: 0; }
.glass-rep-scroll { overflow-x: auto; }
.glass-rep-code { background: var(--ae-wash); font-family: var(--ae-font-mono); font-size: 13px; line-height: 1.7; padding: 1em 1.2em; color: var(--ae-ink); }
.glass-rep-diff-line { display: grid; grid-template-columns: 1.3em 1fr; gap: 0.7em; white-space: pre; }
.glass-rep-gutter { color: var(--ae-ink-faint); user-select: none; text-align: center; }
.glass-rep-gutter.is-add { color: var(--ae-ok); }
.glass-rep-gutter.is-del { color: var(--ae-err); }
.glass-rep-terminal-line { display: flex; gap: 0.6em; white-space: pre-wrap; }
.glass-rep-mark { flex: none; user-select: none; font-size: 11px; }
.glass-rep-mark.is-ok { color: var(--ae-ok); }
.glass-rep-mark.is-prompt { color: var(--ae-ink-faint); }
.glass-rep-pull { margin: 1.4em 0; }
.glass-rep-badge-row { display: flex; flex-wrap: wrap; align-items: center; gap: 0.5em; margin: 1em 0; }
.glass-rep-badge { display: inline-flex; gap: 0.35em; align-items: baseline; }
.glass-rep-badge.is-warn { border-color: var(--ae-warn); }
.glass-rep-icon-row { display: grid; gap: var(--ae-space-3); margin: 1em 0; }
.glass-rep-icon-row-item .ae-list-icon { align-self: center; color: var(--ae-ink-muted); }
.glass-rep-prose { max-width: 62em; }
.glass-rep-caption { margin-top: 1.6em; padding-top: 1em; border-top: 1px solid var(--ae-line); font-family: var(--ae-font-mono); font-size: 11px; letter-spacing: 0.06em; color: var(--ae-ink-faint); }
.glass-rep-empty { color: var(--ae-ink-muted); font-size: 13px; }
@media (max-width: 48rem) {
  .glass-rep-hero-trend { grid-template-columns: minmax(0, 1fr); align-items: start; }
  .glass-rep-pipeline { grid-auto-flow: row; }
  .glass-rep-stage { flex-direction: row; align-items: baseline; gap: 0.6em; border-left: 0; border-top: 1px solid var(--ae-line); }
  .glass-rep-stage:first-child { border-top: 0; }
  .glass-rep-stage-note { margin-left: auto; }
  .glass-rep-meter-row { grid-template-columns: minmax(0, 1fr); gap: 0.35em; }
}
"#;

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_components() -> Vec<ReportComponent> {
        vec![
            ReportComponent::Hero {
                kicker: "FLEET DIGEST - PAST 24H - BRIEF".to_string(),
                headline: "The fleet moved.".to_string(),
                figures: vec![
                    Figure {
                        value: "12".to_string(),
                        label: "completed".to_string(),
                        warn: false,
                    },
                    Figure {
                        value: "1".to_string(),
                        label: "blocked".to_string(),
                        warn: true,
                    },
                ],
                trend: vec![
                    SeriesPoint {
                        label: "10".to_string(),
                        value: 2.0,
                    },
                    SeriesPoint {
                        label: "11".to_string(),
                        value: 4.0,
                    },
                ],
                peak_label: Some("peak 4 - 11".to_string()),
            },
            ReportComponent::StatBand {
                figures: vec![
                    Figure {
                        value: "12".to_string(),
                        label: "completed".to_string(),
                        warn: false,
                    },
                    Figure {
                        value: "1".to_string(),
                        label: "blocked".to_string(),
                        warn: true,
                    },
                ],
            },
            ReportComponent::Spark {
                series: vec![
                    SeriesPoint {
                        label: "10".to_string(),
                        value: 2.0,
                    },
                    SeriesPoint {
                        label: "11".to_string(),
                        value: 4.0,
                    },
                ],
            },
            ReportComponent::Bars {
                series: vec![
                    SeriesPoint {
                        label: "10".to_string(),
                        value: 2.0,
                    },
                    SeriesPoint {
                        label: "11".to_string(),
                        value: 4.0,
                    },
                ],
            },
            ReportComponent::Meters {
                pairs: vec![
                    MeterPair {
                        label: "glass".to_string(),
                        value: 9.0,
                    },
                    MeterPair {
                        label: "powder".to_string(),
                        value: 3.0,
                    },
                ],
            },
            ReportComponent::Pipeline {
                stages: vec![
                    PipelineStage {
                        label: "spec".to_string(),
                        state: PipelineState::Done,
                        note: Some("ratified".to_string()),
                    },
                    PipelineStage {
                        label: "live-fire".to_string(),
                        state: PipelineState::Blocked,
                        note: Some("key".to_string()),
                    },
                ],
            },
            ReportComponent::Trail {
                events: vec![TrailEvent {
                    time: "18:24".to_string(),
                    kind: Some("receipt".to_string()),
                    agent: Some("deploy".to_string()),
                    title: "healthy".to_string(),
                    href: Some("/session/s/p/p".to_string()),
                }],
            },
            ReportComponent::Callouts {
                lines: vec![StatusLine {
                    status: Some("warn".to_string()),
                    text: "needs a key".to_string(),
                    href: None,
                }],
            },
            ReportComponent::EvidenceChips {
                links: vec![EvidenceLink {
                    label: "card".to_string(),
                    href: "/board#card".to_string(),
                }],
            },
            ReportComponent::DiffExhibit {
                file: "src/lib.rs".to_string(),
                lines: vec![
                    DiffLine {
                        state: DiffState::Ctx,
                        text: "fn main()".to_string(),
                    },
                    DiffLine {
                        state: DiffState::Del,
                        text: "old".to_string(),
                    },
                    DiffLine {
                        state: DiffState::Add,
                        text: "new".to_string(),
                    },
                ],
            },
            ReportComponent::TerminalExhibit {
                lines: vec!["cargo test".to_string(), "OK passed".to_string()],
            },
            ReportComponent::PullQuote {
                text: "cached, not curated".to_string(),
                by: Some("DESIGN.md".to_string()),
            },
            ReportComponent::BadgeRow {
                badges: vec![Badge {
                    label: "risk".to_string(),
                    value: Some("1".to_string()),
                    status: Some("warn".to_string()),
                }],
            },
            ReportComponent::IconRow {
                rows: vec![IconRowItem {
                    icon: Some("ok".to_string()),
                    text: "gate passed".to_string(),
                    meta: Some("check.sh".to_string()),
                }],
            },
            ReportComponent::Prose {
                text: "The fleet moved.".to_string(),
            },
            ReportComponent::FigCaption {
                text: "sources: wire, powder".to_string(),
            },
        ]
    }

    #[test]
    fn component_list_renders_every_kind_snapshot() {
        let html = render_components(&sample_components());
        assert!(html.contains("glass-rep-hero"));
        assert!(html.contains("glass-rep-stat-band"));
        assert!(html.contains("glass-rep-sparkline"));
        assert!(html.contains("glass-rep-bars"));
        assert!(html.contains("glass-rep-meters"));
        assert!(html.contains("glass-rep-pipeline"));
        assert!(html.contains("glass-rep-trail"));
        assert!(html.contains("glass-rep-callouts"));
        assert!(html.contains("glass-rep-evidence"));
        assert!(html.contains("diff - src/lib.rs"));
        assert!(html.contains("glass-rep-terminal-line"));
        assert!(html.contains("ae-pull glass-rep-pull"));
        assert!(html.contains("glass-rep-badge-row"));
        assert!(html.contains("glass-rep-icon-row"));
        assert!(html.contains("glass-rep-prose"));
        assert!(html.contains("glass-rep-caption"));
    }

    #[test]
    fn serde_component_list_round_trips_from_generating_models() {
        let raw = serde_json::json!([
            {"kind":"hero","kicker":"FLEET DIGEST - PAST 24H - BRIEF","headline":"A digest.","figures":[{"value":"2","label":"done"}],"trend":[{"label":"10","value":2}],"peak_label":"peak 2 - 10"},
            {"kind":"stat_band","figures":[{"value":"2","label":"done"}]},
            {"kind":"prose","text":"No wall of prose."}
        ]);
        let components: Vec<ReportComponent> =
            serde_json::from_value(raw).expect("component list shape");
        assert_eq!(components.len(), 3);
        assert!(render_components(&components).contains("glass-rep-hero"));
        assert!(render_components(&components).contains("No wall of prose."));
    }

    #[test]
    fn hostile_diff_and_terminal_content_is_escaped() {
        let html = render_components(&[
            ReportComponent::DiffExhibit {
                file: "<script>alert(1)</script>".to_string(),
                lines: vec![DiffLine {
                    state: DiffState::Add,
                    text: r#"<img src=x onerror=alert(1)> "quoted""#.to_string(),
                }],
            },
            ReportComponent::TerminalExhibit {
                lines: vec![r#"<script>alert("x")</script>"#.to_string()],
            },
        ]);
        assert!(!html.contains("<script>"));
        assert!(!html.contains("<img "));
        assert!(html.contains("&lt;script&gt;alert(&quot;x&quot;)&lt;/script&gt;"));
        assert!(html.contains("&lt;img src=x onerror=alert(1)&gt;"));
    }
}
