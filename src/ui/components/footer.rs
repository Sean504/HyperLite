/// Single-row status bar at the bottom.

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use crate::app::App;

pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let bg_style = Style::default().bg(app.theme.bg_element);

    // ── Left side: cwd + model ───────────────────────────────────────────────
    let cwd = std::env::current_dir()
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
        .unwrap_or_else(|| "~".to_string());

    let model_name = app.current_model_name();
    let provider_name = app.current_backend_name();

    let left = Line::from(vec![
        Span::styled(" 󰉋 ", Style::default().fg(app.theme.accent)),
        Span::styled(cwd, Style::default().fg(app.theme.text_muted)),
        Span::styled("  ", Style::default()),
        Span::styled(" ", Style::default().fg(app.theme.primary)),
        Span::styled(model_name, Style::default().fg(app.theme.text).add_modifier(Modifier::BOLD)),
        Span::styled(format!(" ({})", provider_name), Style::default().fg(app.theme.text_muted)),
    ]);

    // ── Right side: key hints ────────────────────────────────────────────────
    let hints = build_hints(app);
    let right = Line::from(hints);

    // Measure right side width
    let right_width = right.spans.iter().map(|s| s.content.len() as u16).sum::<u16>();
    let left_width  = area.width.saturating_sub(right_width);

    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(left_width),
            Constraint::Length(right_width),
        ])
        .split(area);

    let left_para  = Paragraph::new(left).style(bg_style);
    let right_para = Paragraph::new(right).style(bg_style);

    frame.render_widget(left_para,  chunks[0]);
    frame.render_widget(right_para, chunks[1]);
}

fn build_hints(app: &App) -> Vec<Span<'static>> {
    let mut spans = vec![];

    let kb = |key: &'static str, desc: &'static str| -> Vec<Span<'static>> {
        vec![
            Span::styled(format!(" {} ", key), Style::default().fg(app.theme.bg_menu).bg(app.theme.text_muted)),
            Span::styled(format!(" {} ", desc), Style::default().fg(app.theme.text_muted)),
        ]
    };

    if app.is_streaming() {
        spans.extend(kb("Ctrl+C", "Stop"));
    } else {
        spans.extend(kb("?", "help"));
        spans.extend(kb("Ctrl+K", "palette"));
        spans.extend(kb("Ctrl+N", "new"));
        spans.extend(kb("Ctrl+S", "sessions"));
    }

    // Token count if available
    if let Some(tokens) = app.last_token_count {
        spans.push(Span::styled(
            format!(" {}t ", tokens),
            Style::default().fg(app.theme.text_dim),
        ));
    }

    spans
}
