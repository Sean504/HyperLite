/// Render a single chat message (user or assistant) into ratatui Lines.

use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use crate::session::message::{Message, Part, Role};
use crate::ui::theme::Theme;
use super::tool_call::render_tool_part;

fn fmt_ts(ts: i64) -> String {
    chrono::DateTime::from_timestamp(ts, 0)
        .map(|dt: chrono::DateTime<chrono::Utc>| dt.format("%H:%M").to_string())
        .unwrap_or_default()
}

/// Returns all display lines for a message.
pub fn render_message(msg: &Message, theme: &Theme, width: u16, show_tool_details: bool) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = vec![];

    match msg.role {
        Role::User      => render_user(msg, theme, &mut lines),
        Role::Assistant => render_assistant(msg, theme, width, show_tool_details, &mut lines),
    }

    lines.push(Line::default());
    lines
}

fn render_user(msg: &Message, theme: &Theme, lines: &mut Vec<Line<'static>>) {
    // Check if this is an internal tool result message
    let text = msg.parts.iter().find_map(|p| if let Part::Text(t) = p { Some(t.text.trim()) } else { None }).unwrap_or("");
    if text.starts_with("<tool_result>") {
        render_tool_result_msg(text, theme, lines);
        return;
    }

    let ts = fmt_ts(msg.created_at);
    lines.push(Line::from(vec![
        Span::styled(" You ", Style::default().fg(theme.accent).add_modifier(Modifier::BOLD)),
        Span::styled(ts, Style::default().fg(theme.text_dim)),
    ]));

    for part in &msg.parts {
        if let Part::Text(t) = part {
            for text_line in t.text.lines() {
                lines.push(Line::from(vec![
                    Span::styled(" ┃ ", Style::default().fg(theme.accent)),
                    Span::styled(text_line.to_string(), Style::default().fg(theme.text)),
                ]));
            }
        }
    }
}

fn render_tool_result_msg(text: &str, theme: &Theme, lines: &mut Vec<Line<'static>>) {
    // Parse all <tool_result> blocks in the message
    let mut rest = text;
    while let Some(start) = rest.find("<tool_result>") {
        rest = &rest[start + "<tool_result>".len()..];
        let end = rest.find("</tool_result>").unwrap_or(rest.len());
        let inner = &rest[..end];
        rest = if end < rest.len() { &rest[end + "</tool_result>".len()..] } else { "" };

        let name   = extract_tag(inner, "name").unwrap_or("tool");
        let status = extract_tag(inner, "status").unwrap_or("ok");
        let output = extract_tag(inner, "output").unwrap_or("");

        // ── make_plan: special checklist rendering ─────────────────────────────
        if name == "make_plan" {
            lines.push(Line::from(vec![
                Span::styled(" ◈ ", Style::default().fg(theme.accent)),
                Span::styled("Plan", Style::default().fg(theme.accent).add_modifier(Modifier::BOLD)),
                Span::styled("  ready to execute", Style::default().fg(theme.text_dim)),
            ]));
            for line in output.lines() {
                let trimmed = line.trim();
                if trimmed.is_empty() { continue; }
                // Lines like "1. Step text" get checkbox styling
                if trimmed.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false) {
                    let dot_pos = trimmed.find(". ").unwrap_or(0);
                    let (num, step) = trimmed.split_at(dot_pos.min(trimmed.len()));
                    let step = step.trim_start_matches(". ");
                    lines.push(Line::from(vec![
                        Span::styled("   ", Style::default()),
                        Span::styled("○ ", Style::default().fg(theme.text_dim)),
                        Span::styled(format!("{}. ", num), Style::default().fg(theme.text_dim)),
                        Span::styled(step.to_string(), Style::default().fg(theme.text)),
                    ]));
                } else if trimmed.starts_with("▸") || trimmed.starts_with("steps planned") {
                    // skip decorative lines already shown in header
                } else {
                    lines.push(Line::from(vec![
                        Span::styled(format!("   {}", trimmed), Style::default().fg(theme.text_muted)),
                    ]));
                }
            }
            lines.push(Line::default());
            continue;
        }

        // ── All other tools ────────────────────────────────────────────────────
        let (icon, status_style) = if status == "error" {
            ("✗", Style::default().fg(theme.error))
        } else {
            ("✓", Style::default().fg(theme.success))
        };

        lines.push(Line::from(vec![
            Span::styled(format!(" {} ", icon), status_style),
            Span::styled(name.to_string(), Style::default().fg(theme.text_muted).add_modifier(Modifier::BOLD)),
            Span::styled("  tool result", Style::default().fg(theme.text_dim)),
        ]));

        // Show output lines (capped to keep UI clean)
        let output_lines: Vec<&str> = output.lines().collect();
        let show = output_lines.len().min(6);
        for line in &output_lines[..show] {
            lines.push(Line::from(vec![
                Span::styled("   │ ", Style::default().fg(theme.border)),
                Span::styled(line.to_string(), Style::default().fg(theme.text_dim)),
            ]));
        }
        if output_lines.len() > 6 {
            lines.push(Line::from(vec![
                Span::styled(
                    format!("   └ … {} more lines", output_lines.len() - 6),
                    Style::default().fg(theme.text_dim),
                ),
            ]));
        }
    }
}

/// Split text into (display_text, vec_of_tool_names_called).
/// <tool_call>...</tool_call> blocks are removed from display but their names are returned.
fn extract_tool_calls_for_display(text: &str) -> (String, Vec<String>) {
    let mut display = String::new();
    let mut tools   = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("<tool_call>") {
        display.push_str(&rest[..start]);
        rest = &rest[start + "<tool_call>".len()..];
        if let Some(end) = rest.find("</tool_call>") {
            let inner = &rest[..end];
            rest = &rest[end + "</tool_call>".len()..];
            // Extract tool name
            if let Some(name) = extract_tag(inner, "name") {
                tools.push(name.to_string());
            }
        }
    }
    display.push_str(rest);
    (display.trim_end().to_string(), tools)
}

fn extract_tag<'a>(text: &'a str, tag: &str) -> Option<&'a str> {
    let open  = format!("<{}>", tag);
    let close = format!("</{}>", tag);
    let start = text.find(&open)? + open.len();
    let end   = text[start..].find(&close)?;
    Some(text[start..start + end].trim())
}

fn render_assistant(msg: &Message, theme: &Theme, width: u16, show_tool_details: bool, lines: &mut Vec<Line<'static>>) {
    let model = msg.model.as_deref().unwrap_or("assistant");
    let ts    = fmt_ts(msg.created_at);
    lines.push(Line::from(vec![
        Span::styled(" ", Style::default().fg(theme.primary)),
        Span::styled(model.to_string(), Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)),
        Span::styled(format!("  {}", ts), Style::default().fg(theme.text_dim)),
    ]));

    for part in &msg.parts {
        match part {
            Part::Text(t) => {
                if t.text.is_empty() { continue; }
                // Split tool call blocks from regular text
                let (display_text, called_tools) = extract_tool_calls_for_display(&t.text);
                if !display_text.trim().is_empty() {
                    let rendered = crate::ui::markdown::render(&display_text, theme, width.saturating_sub(2));
                    for md_line in rendered.lines {
                        let mut spans = vec![Span::styled("  ", Style::default())];
                        spans.extend(md_line.spans.into_iter());
                        lines.push(Line::from(spans));
                    }
                }
                for tool_name in called_tools {
                    lines.push(Line::from(vec![
                        Span::styled("  ⚙ ", Style::default().fg(theme.text_dim)),
                        Span::styled(tool_name, Style::default().fg(theme.text_muted).add_modifier(Modifier::BOLD)),
                        Span::styled("  called…", Style::default().fg(theme.text_dim)),
                    ]));
                }
            }

            Part::Reasoning(r) => {
                lines.push(Line::from(vec![
                    Span::styled(" 💭 thinking", Style::default().fg(theme.text_dim).add_modifier(Modifier::ITALIC)),
                ]));
                for r_line in r.text.lines().take(4) {
                    lines.push(Line::from(vec![
                        Span::styled("  ", Style::default()),
                        Span::styled(r_line.to_string(), Style::default().fg(theme.text_dim).add_modifier(Modifier::ITALIC)),
                    ]));
                }
                if r.text.lines().count() > 4 {
                    lines.push(Line::from(vec![
                        Span::styled("  …", Style::default().fg(theme.text_dim)),
                    ]));
                }
            }

            Part::Tool(tp) => {
                for tool_line in render_tool_part(tp, theme, show_tool_details) {
                    lines.push(tool_line);
                }
            }

            Part::File(f) => {
                lines.push(Line::from(vec![
                    Span::styled(format!(" 📎 {}", f.filename), Style::default().fg(theme.accent)),
                ]));
            }
        }
    }
}
