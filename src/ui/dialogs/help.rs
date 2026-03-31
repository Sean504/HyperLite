/// Help dialog: keybinding reference.

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use crate::app::App;
use super::centered_rect;

pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let dialog = centered_rect(64, 32, area);
    frame.render_widget(Clear, dialog);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(app.theme.primary))
        .title(Line::from(vec![
            Span::styled(" Help ", Style::default().fg(app.theme.primary).add_modifier(Modifier::BOLD)),
        ]))
        .style(Style::default().bg(app.theme.bg_panel));

    let inner = block.inner(dialog);
    frame.render_widget(block, dialog);

    let sections = app.keybinds.help_sections();
    let n = sections.len();

    // Two columns
    let half = n.div_ceil(2);
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(inner);

    for (col_idx, chunk) in cols.iter().enumerate() {
        let mut lines: Vec<Line<'static>> = vec![];
        let start = col_idx * half;
        let end   = (start + half).min(n);

        for section_idx in start..end {
            let (section, items) = &sections[section_idx];
            lines.push(Line::from(vec![
                Span::styled(
                    section.to_string(),
                    Style::default().fg(app.theme.accent).add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
                ),
            ]));
            for (key, desc) in items {
                lines.push(Line::from(vec![
                    Span::styled(
                        format!("  {:>12}  ", key),
                        Style::default().fg(app.theme.primary).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(desc.clone(), Style::default().fg(app.theme.text)),
                ]));
            }
            lines.push(Line::default());
        }

        frame.render_widget(Paragraph::new(lines), *chunk);
    }
}
