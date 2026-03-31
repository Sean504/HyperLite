/// Theme picker dialog with live preview swatches.

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph};
use crate::app::App;
use crate::ui::theme;
use super::centered_rect;

pub fn render(frame: &mut Frame, area: Rect, app: &mut App) {
    let dialog = centered_rect(60, 26, area);
    frame.render_widget(Clear, dialog);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(app.theme.primary))
        .title(Line::from(vec![
            Span::styled(" Themes ", Style::default().fg(app.theme.primary).add_modifier(Modifier::BOLD)),
        ]))
        .style(Style::default().bg(app.theme.bg_panel));

    let inner = block.inner(dialog);
    frame.render_widget(block, dialog);

    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(1), Constraint::Length(20)])
        .split(inner);

    let names = theme::all_names();
    let selected_idx = app.dialog_selected_idx.min(names.len().saturating_sub(1));

    let items: Vec<ListItem> = names.iter().enumerate().map(|(i, name)| {
        let is_current = *name == app.config.theme.as_str();
        let marker = if is_current { "✓ " } else { "  " };
        let style = if i == selected_idx {
            Style::default().fg(app.theme.bg).bg(app.theme.primary)
        } else if is_current {
            Style::default().fg(app.theme.success)
        } else {
            Style::default().fg(app.theme.text)
        };
        ListItem::new(format!("{}{}", marker, name)).style(style)
    }).collect();

    let mut list_state = ListState::default();
    list_state.select(Some(selected_idx));

    let list = List::new(items)
        .highlight_style(Style::default().fg(app.theme.bg).bg(app.theme.primary));
    frame.render_stateful_widget(list, chunks[0], &mut list_state);

    // Preview swatches for highlighted theme
    if let Some(name) = names.get(selected_idx) {
        render_preview(frame, chunks[1], name, app);
    }
}

fn render_preview(frame: &mut Frame, area: Rect, theme_name: &str, app: &App) {
    let t = theme::get(theme_name);

    let block = Block::default()
        .borders(Borders::LEFT)
        .border_style(Style::default().fg(app.theme.border))
        .style(Style::default().bg(app.theme.bg_panel));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let swatches: &[(&str, ratatui::style::Color, &str)] = &[
        ("bg",       t.bg,        "Background"),
        ("primary",  t.primary,   "Primary"),
        ("accent",   t.accent,    "Accent"),
        ("sec",      t.secondary, "Secondary"),
        ("success",  t.success,   "Success"),
        ("error",    t.error,     "Error"),
        ("warning",  t.warning,   "Warning"),
        ("text",     t.text,      "Text"),
        ("muted",    t.text_muted,"Muted"),
    ];

    let lines: Vec<Line<'static>> = swatches.iter().map(|(_, color, label)| {
        Line::from(vec![
            Span::styled("███ ", Style::default().fg(*color)),
            Span::styled(label.to_string(), Style::default().fg(app.theme.text_muted)),
        ])
    }).collect();

    frame.render_widget(Paragraph::new(lines), inner);
}
