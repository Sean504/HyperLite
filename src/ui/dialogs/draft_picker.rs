/// Draft picker dialog — browse and restore stashed input drafts.

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph};
use crate::app::App;
use super::centered_rect;

pub fn render(frame: &mut Frame, area: Rect, app: &mut App) {
    let dialog = centered_rect(60, 20, area);
    frame.render_widget(Clear, dialog);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(app.theme.accent))
        .title(Line::from(Span::styled(
            " ✎ Stashed Drafts ",
            Style::default().fg(app.theme.accent).add_modifier(Modifier::BOLD),
        )))
        .style(Style::default().bg(app.theme.bg_panel));

    let inner = block.inner(dialog);
    frame.render_widget(block, dialog);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);

    let items: Vec<ListItem> = if app.drafts.is_empty() {
        vec![ListItem::new(Span::styled(
            "  No stashed drafts yet.  Press Ctrl+D to stash current input.",
            Style::default().fg(app.theme.fg_dim),
        ))]
    } else {
        app.drafts.iter().map(|d| {
            let preview = d.content.lines().next().unwrap_or("").chars().take(60).collect::<String>();
            ListItem::new(Line::from(vec![
                Span::styled(format!("  {} ", d.label), Style::default().fg(app.theme.accent)),
                Span::styled(preview, Style::default().fg(app.theme.fg_dim)),
            ]))
        }).collect()
    };

    let mut state = ListState::default();
    if !app.drafts.is_empty() {
        state.select(Some(app.dialog_selected_idx.min(app.drafts.len().saturating_sub(1))));
    }

    let list = List::new(items)
        .highlight_style(Style::default().bg(app.theme.selection).fg(app.theme.fg).add_modifier(Modifier::BOLD))
        .highlight_symbol("▶ ");

    frame.render_stateful_widget(list, chunks[0], &mut state);

    let hint = Paragraph::new(Line::from(vec![
        Span::styled(" Enter", Style::default().fg(app.theme.accent)),
        Span::styled(" restore  ", Style::default().fg(app.theme.fg_dim)),
        Span::styled("d", Style::default().fg(app.theme.accent)),
        Span::styled(" delete  ", Style::default().fg(app.theme.fg_dim)),
        Span::styled("Esc", Style::default().fg(app.theme.accent)),
        Span::styled(" close", Style::default().fg(app.theme.fg_dim)),
    ]));
    frame.render_widget(hint, chunks[1]);
}
