/// Render a tool call part as a syslog-style trace line, with optional
/// expanded input/output detail.

use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use crate::session::message::{ToolPart, ToolState};
use crate::ui::theme::Theme;

fn fmt_ts_log(ts: i64) -> String {
    chrono::DateTime::from_timestamp(ts, 0)
        .map(|dt| dt.with_timezone(&chrono::Local).format("%H:%M:%S").to_string())
        .unwrap_or_default()
}

/// `[14:32:07] ▸ shell · cargo build` trace header for a tool part.
fn trace_header(
    ts: &str,
    marker: &str,
    marker_style: Style,
    part: &ToolPart,
    title_style: Style,
    theme: &Theme,
) -> Line<'static> {
    // display_title() is "icon arg" — strip the icon, the marker replaces it
    let arg = part.display_title();
    let arg = arg.splitn(2, ' ').nth(1).unwrap_or("").trim().to_string();

    let mut spans = vec![
        Span::styled(format!(" [{}] ", ts), Style::default().fg(theme.text_dim)),
        Span::styled(format!("{} ", marker), marker_style),
        Span::styled(part.name.clone(), title_style.add_modifier(Modifier::BOLD)),
    ];
    if !arg.is_empty() {
        spans.push(Span::styled(" · ", Style::default().fg(theme.text_dim)));
        let preview: String = arg.chars().take(60).collect();
        spans.push(Span::styled(preview, title_style));
    }
    Line::from(spans)
}

/// Returns lines for a single ToolPart. `created_at` is the parent message
/// timestamp, used for the trace-log prefix.
pub fn render_tool_part(part: &ToolPart, theme: &Theme, expanded: bool, created_at: i64) -> Vec<Line<'static>> {
    let ts = fmt_ts_log(created_at);

    match &part.state {
        ToolState::Pending | ToolState::AwaitingPermission => {
            vec![trace_header(
                &ts, "▸",
                Style::default().fg(theme.text_muted),
                part,
                Style::default().fg(theme.text_muted).add_modifier(Modifier::ITALIC),
                theme,
            )]
        }

        ToolState::Running => {
            let mut lines = vec![trace_header(
                &ts, "▸",
                Style::default().fg(theme.warning).add_modifier(Modifier::BOLD),
                part,
                Style::default().fg(theme.warning),
                theme,
            )];
            if expanded {
                let input_str = serde_json::to_string_pretty(&part.input).unwrap_or_default();
                for l in input_str.lines().take(6) {
                    lines.push(Line::from(vec![
                        Span::styled("    │ ", Style::default().fg(theme.border)),
                        Span::styled(l.to_string(), Style::default().fg(theme.text_dim)),
                    ]));
                }
            }
            lines
        }

        ToolState::Complete => {
            let mut lines = vec![trace_header(
                &ts, "✓",
                Style::default().fg(theme.success),
                part,
                Style::default().fg(theme.text),
                theme,
            )];

            if expanded {
                // Input
                let input_str = serde_json::to_string_pretty(&part.input).unwrap_or_default();
                if !input_str.is_empty() && input_str != "null" {
                    lines.push(Line::from(vec![
                        Span::styled("  input  ", Style::default().fg(theme.text_dim)),
                    ]));
                    for l in input_str.lines().take(6) {
                        lines.push(Line::from(vec![
                            Span::styled("    │ ", Style::default().fg(theme.border)),
                            Span::styled(l.to_string(), Style::default().fg(theme.text_dim)),
                        ]));
                    }
                }
                // Output
                if let Some(out) = &part.output {
                    lines.push(Line::from(vec![
                        Span::styled("  output ", Style::default().fg(theme.text_dim)),
                    ]));
                    for l in out.lines().take(8) {
                        lines.push(Line::from(vec![
                            Span::styled("    │ ", Style::default().fg(theme.border)),
                            Span::styled(l.to_string(), Style::default().fg(theme.text_muted)),
                        ]));
                    }
                    if out.lines().count() > 8 {
                        lines.push(Line::from(vec![
                            Span::styled("    └ …", Style::default().fg(theme.text_dim)),
                        ]));
                    }
                }
            }
            lines
        }

        ToolState::Error => {
            let err = part.error.as_deref().unwrap_or("unknown error");
            let mut lines = vec![trace_header(
                &ts, "✗",
                Style::default().fg(theme.error).add_modifier(Modifier::BOLD),
                part,
                Style::default().fg(theme.error),
                theme,
            )];
            lines.push(Line::from(vec![
                Span::styled("    │ ", Style::default().fg(theme.border)),
                Span::styled(err.lines().next().unwrap_or(err).to_string(), Style::default().fg(theme.text_muted)),
            ]));
            lines
        }

        ToolState::Denied => {
            vec![trace_header(
                &ts, "⊘",
                Style::default().fg(theme.text_dim),
                part,
                Style::default().fg(theme.text_dim),
                theme,
            )]
        }
    }
}
