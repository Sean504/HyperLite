/// Permission prompt — replaces the input area when a tool requests approval.
/// Restyled to match the reviewer: thin rounded frame, command shown in a
/// tinted card, decision rendered as chip buttons.

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};
use crate::app::App;

pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let Some(req) = &app.pending_permission else { return };
    let theme = &app.theme;

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.warning))
        .title(Line::from(vec![
            Span::styled(" ⚠ Permission ", Style::default().fg(theme.bg).bg(theme.warning).add_modifier(Modifier::BOLD)),
            Span::styled(format!(" {} ", req.tool), Style::default().fg(theme.warning).add_modifier(Modifier::BOLD)),
        ]))
        .style(Style::default().bg(theme.bg_panel));

    let inner = block.inner(area);
    frame.render_widget(Clear, area);
    frame.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(inner);

    // Action summary line
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(" wants to ", Style::default().fg(theme.text_muted)),
            Span::styled(req.detail.clone(), Style::default().fg(theme.text)),
        ])),
        chunks[0],
    );

    // Command / diff preview in a tinted card
    let preview = req.diff.as_deref().unwrap_or(req.detail.as_str());
    if !preview.is_empty() {
        frame.render_widget(
            Block::default().style(Style::default().bg(theme.bg_element)),
            chunks[1],
        );
        let card_inner = Rect {
            x: chunks[1].x + 1, y: chunks[1].y,
            width: chunks[1].width.saturating_sub(2), height: chunks[1].height,
        };
        let lines: Vec<Line<'static>> = preview.lines().take(card_inner.height as usize).map(|l| {
            Line::from(vec![
                Span::styled("│ ", Style::default().fg(theme.warning)),
                Span::styled(l.to_string(), Style::default().fg(theme.accent)),
            ])
        }).collect();
        frame.render_widget(Paragraph::new(lines), card_inner);
    }

    // Chip buttons
    let chip = |key: &'static str, label: &'static str, color: ratatui::style::Color| -> Vec<Span<'static>> {
        vec![
            Span::styled(format!(" {} ", key), Style::default().fg(theme.bg).bg(color).add_modifier(Modifier::BOLD)),
            Span::styled(format!(" {}   ", label), Style::default().fg(color)),
        ]
    };
    let mut btns = vec![Span::styled(" ", Style::default())];
    btns.extend(chip("y", "allow once", theme.success));
    btns.extend(chip("a", "allow always", theme.primary));
    btns.extend(chip("n", "deny", theme.error));
    frame.render_widget(Paragraph::new(Line::from(btns)), chunks[2]);
}
