/// Render a tool call part: pending inline badge or expanded block.

use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use crate::session::message::{ToolPart, ToolState};
use crate::ui::theme::Theme;

/// Returns lines for a single ToolPart.
pub fn render_tool_part(part: &ToolPart, theme: &Theme, expanded: bool) -> Vec<Line<'static>> {
    let icon  = part.icon();
    let title = part.display_title();

    match &part.state {
        ToolState::Pending | ToolState::AwaitingPermission => {
            vec![Line::from(vec![
                Span::styled("  ", Style::default()),
                Span::styled(
                    format!("{} {} ", icon, part.pending_text()),
                    Style::default().fg(theme.text_muted).add_modifier(Modifier::ITALIC),
                ),
            ])]
        }

        ToolState::Running => {
            let mut lines = vec![];
            lines.push(Line::from(vec![
                Span::styled(format!(" {} ", icon), Style::default().fg(theme.warning)),
                Span::styled(title.clone(), Style::default().fg(theme.warning).add_modifier(Modifier::BOLD)),
                Span::styled(" …", Style::default().fg(theme.text_muted).add_modifier(Modifier::ITALIC)),
            ]));
            if expanded {
                let input_str = serde_json::to_string_pretty(&part.input).unwrap_or_default();
                for l in input_str.lines().take(6) {
                    lines.push(Line::from(vec![
                        Span::styled("    ", Style::default()),
                        Span::styled(l.to_string(), Style::default().fg(theme.text_dim)),
                    ]));
                }
            }
            lines
        }

        ToolState::Complete => {
            let mut lines = vec![];
            lines.push(Line::from(vec![
                Span::styled(format!(" {} ", icon), Style::default().fg(theme.success)),
                Span::styled(title.clone(), Style::default().fg(theme.text).add_modifier(Modifier::BOLD)),
            ]));

            if expanded {
                // Input
                let input_str = serde_json::to_string_pretty(&part.input).unwrap_or_default();
                if !input_str.is_empty() && input_str != "null" {
                    lines.push(Line::from(vec![
                        Span::styled("  input  ", Style::default().fg(theme.text_dim)),
                    ]));
                    for l in input_str.lines().take(6) {
                        lines.push(Line::from(vec![
                            Span::styled("    ", Style::default()),
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
                            Span::styled("    ", Style::default()),
                            Span::styled(l.to_string(), Style::default().fg(theme.text_muted)),
                        ]));
                    }
                    if out.lines().count() > 8 {
                        lines.push(Line::from(vec![
                            Span::styled("    …", Style::default().fg(theme.text_dim)),
                        ]));
                    }
                }
            }
            lines
        }

        ToolState::Error => {
            let err = part.error.as_deref().unwrap_or("unknown error");
            vec![Line::from(vec![
                Span::styled(format!(" {} ", icon), Style::default().fg(theme.error)),
                Span::styled(title.clone(), Style::default().fg(theme.error)),
                Span::styled(format!(": {}", err), Style::default().fg(theme.text_muted)),
            ])]
        }

        ToolState::Denied => {
            vec![Line::from(vec![
                Span::styled(format!(" {} ", icon), Style::default().fg(theme.text_dim)),
                Span::styled(title.clone(), Style::default().fg(theme.text_dim)),
                Span::styled(" (denied)", Style::default().fg(theme.text_dim)),
            ])]
        }
    }
}
