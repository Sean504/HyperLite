/// Session switcher dialog with fuzzy search.

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph};
use crate::app::App;
use super::centered_rect;

pub fn render(frame: &mut Frame, area: Rect, app: &mut App) {
    let dialog = centered_rect(60, 24, area);
    frame.render_widget(Clear, dialog);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(app.theme.primary))
        .title(Line::from(vec![
            Span::styled(" Sessions ", Style::default().fg(app.theme.primary).add_modifier(Modifier::BOLD)),
        ]))
        .style(Style::default().bg(app.theme.bg_panel));

    let inner = block.inner(dialog);
    frame.render_widget(block, dialog);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(inner);

    // Search box
    let search_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(app.theme.border_hi))
        .style(Style::default().bg(app.theme.bg_element));
    let search_inner = search_block.inner(chunks[0]);
    frame.render_widget(search_block, chunks[0]);

    let query = &app.dialog_search_query;
    let search_line = Line::from(vec![
        Span::styled(" 🔍 ", Style::default().fg(app.theme.accent)),
        Span::styled(query.clone(), Style::default().fg(app.theme.text)),
        Span::styled("█", Style::default().fg(app.theme.accent)),
    ]);
    frame.render_widget(Paragraph::new(search_line), search_inner);

    // Filter sessions by query
    let sessions: Vec<&crate::session::message::Session> = app.sessions.iter()
        .filter(|s| {
            query.is_empty() ||
            s.title.to_lowercase().contains(&query.to_lowercase())
        })
        .collect();

    let items: Vec<ListItem> = sessions.iter().map(|s| {
        let is_active = s.id == app.session_id;
        let marker = if is_active { "▶ " } else { "  " };
        let date = chrono::DateTime::from_timestamp(s.updated_at, 0)
            .map(|dt: chrono::DateTime<chrono::Utc>| dt.format("%m/%d %H:%M").to_string())
            .unwrap_or_default();
        let style = if is_active {
            Style::default().fg(app.theme.accent).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(app.theme.text)
        };
        let label = format!("{}{:<40} {}", marker, truncate(&s.title, 40), date);
        ListItem::new(label).style(style)
    }).collect();

    let mut list_state = ListState::default();
    list_state.select(Some(app.dialog_selected_idx.min(sessions.len().saturating_sub(1))));

    let list = List::new(items)
        .highlight_style(Style::default().fg(app.theme.bg).bg(app.theme.primary))
        .highlight_symbol("► ");

    frame.render_stateful_widget(list, chunks[1], &mut list_state);

    // Hint row
    let hint = Paragraph::new(Line::from(vec![
        Span::styled(" ↑↓ navigate  Enter select  Ctrl+W delete  Esc close", Style::default().fg(app.theme.text_dim)),
    ]));
    frame.render_widget(hint, chunks[2]);
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max { return s.to_string(); }
    let cut: String = s.chars().take(max.saturating_sub(1)).collect();
    format!("{}…", cut)
}
