/// Single-row status bar at the bottom.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use crate::app::App;

pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let bg_style = Style::default().bg(app.theme.bg_element);

    // ── Key hints only ───────────────────────────────────────────────────────
    let hints = build_hints(app);
    frame.render_widget(
        Paragraph::new(Line::from(hints)).style(bg_style),
        area,
    );
}

fn build_hints(app: &App) -> Vec<Span<'static>> {
    let mut spans = vec![];

    let kb = |key: &'static str, desc: &'static str| -> Vec<Span<'static>> {
        vec![
            Span::styled(format!(" {} ", key), Style::default().fg(app.theme.bg_menu).bg(app.theme.text_muted)),
            Span::styled(format!(" {} ", desc), Style::default().fg(app.theme.text_muted)),
        ]
    };

    if app.pending_diff.is_some() {
        spans.extend(kb("Enter", "apply diff"));
        spans.extend(kb("Esc", "discard"));
        return spans;
    }

    if app.is_streaming() {
        spans.extend(kb("Ctrl+C", "Stop"));
    } else {
        spans.extend(kb("?", "help"));
        spans.extend(kb("Ctrl+K", "palette"));
        spans.extend(kb("Ctrl+N", "new"));
        spans.extend(kb("Ctrl+S", "sessions"));
        spans.extend(kb("Ctrl+\\", "sidebar"));
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
