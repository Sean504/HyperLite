/// Permission prompt — replaces input area when a tool requests approval.

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use crate::app::App;

pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let Some(req) = &app.pending_permission else { return };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(app.theme.warning))
        .title(Line::from(vec![
            Span::styled(" ⚠ Permission Required ", Style::default().fg(app.theme.warning).add_modifier(Modifier::BOLD)),
        ]))
        .style(Style::default().bg(app.theme.bg_panel));

    let inner = block.inner(area);
    frame.render_widget(Clear, area);
    frame.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(inner);

    // Tool + description
    let desc_para = Paragraph::new(vec![
        Line::from(vec![
            Span::styled(format!(" Tool:   {}", req.tool), Style::default().fg(app.theme.text)),
        ]),
        Line::from(vec![
            Span::styled(format!(" Action: {}", req.detail), Style::default().fg(app.theme.text_muted)),
        ]),
    ]);
    frame.render_widget(desc_para, chunks[0]);

    // Diff or command preview
    let preview = req.diff.as_deref().unwrap_or(req.detail.as_str());
    if !preview.is_empty() {
        let cmd_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(app.theme.border))
            .style(Style::default().bg(app.theme.bg_element));
        let cmd_inner = cmd_block.inner(chunks[1]);
        frame.render_widget(cmd_block, chunks[1]);

        let lines: Vec<Line<'static>> = preview.lines().take(6).map(|l| {
            Line::from(vec![
                Span::styled(l.to_string(), Style::default().fg(app.theme.accent)),
            ])
        }).collect();
        frame.render_widget(Paragraph::new(lines), cmd_inner);
    }

    // Buttons hint
    let btn_line = Line::from(vec![
        Span::styled(" [y] Allow once  ", Style::default().fg(app.theme.success).add_modifier(Modifier::BOLD)),
        Span::styled("[a] Allow always  ", Style::default().fg(app.theme.primary)),
        Span::styled("[n] Deny ", Style::default().fg(app.theme.error)),
    ]);
    frame.render_widget(Paragraph::new(btn_line), chunks[2]);
}
