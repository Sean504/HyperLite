/// Agent picker dialog — switch between General, Build, Plan, and custom agents.
/// Also contains the agent editor form for creating/editing custom agents.

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph};
use crate::app::App;
use crate::tools::BUILTIN_AGENTS;
use super::centered_rect;

pub fn render(frame: &mut Frame, area: Rect, app: &mut App) {
    let dialog = centered_rect(72, 22, area);
    frame.render_widget(Clear, dialog);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(app.theme.accent))
        .title(Line::from(vec![
            Span::styled(" ◈ Switch Agent ", Style::default().fg(app.theme.accent).add_modifier(Modifier::BOLD)),
        ]))
        .style(Style::default().bg(app.theme.bg_panel));

    let inner = block.inner(dialog);
    frame.render_widget(block, dialog);

    // inner width = dialog(72) - 2 borders = 70; "● Name  — " prefix uses ~14 chars
    let inner_w  = dialog.width.saturating_sub(2) as usize;
    let desc_max = inner_w.saturating_sub(14);

    let truncate_desc = |s: &str| -> String {
        if s.len() > desc_max {
            format!("{}…", &s[..desc_max.saturating_sub(1)])
        } else {
            s.to_string()
        }
    };

    let mut items: Vec<ListItem> = Vec::new();

    for agent in BUILTIN_AGENTS {
        let active = app.current_agent == agent.id;
        let marker = if active { "● " } else { "  " };
        let name_style = if active {
            Style::default().fg(app.theme.accent).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(app.theme.text)
        };
        items.push(ListItem::new(Line::from(vec![
            Span::styled(format!("{}{}", marker, agent.name), name_style),
            Span::styled(format!("  — {}", truncate_desc(agent.description)), Style::default().fg(app.theme.text_muted)),
        ])));
    }

    for agent in &app.custom_agents {
        let active = app.current_agent == agent.id;
        let marker = if active { "● " } else { "  " };
        let name_style = if active {
            Style::default().fg(app.theme.accent).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(app.theme.text)
        };
        let desc = agent.description.as_deref().unwrap_or("Custom agent");
        items.push(ListItem::new(Line::from(vec![
            Span::styled(format!("{}{}", marker, agent.name), name_style),
            Span::styled(format!("  — {}", truncate_desc(desc)), Style::default().fg(app.theme.text_muted)),
        ])));
    }

    items.push(ListItem::new(Line::from(vec![
        Span::styled("  + New Agent…", Style::default().fg(app.theme.accent)),
    ])));

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);

    let mut state = ListState::default();
    state.select(Some(app.dialog_selected_idx.min(items.len().saturating_sub(1))));

    let list = List::new(items)
        .highlight_style(Style::default().bg(app.theme.bg_element).fg(app.theme.text).add_modifier(Modifier::BOLD))
        .highlight_symbol("▶ ");

    frame.render_stateful_widget(list, chunks[0], &mut state);

    let hint = Paragraph::new(Line::from(vec![
        Span::styled(" Enter", Style::default().fg(app.theme.accent)),
        Span::styled(" select  ", Style::default().fg(app.theme.text_muted)),
        Span::styled("d", Style::default().fg(app.theme.accent)),
        Span::styled(" delete custom  ", Style::default().fg(app.theme.text_muted)),
        Span::styled("Esc", Style::default().fg(app.theme.accent)),
        Span::styled(" close", Style::default().fg(app.theme.text_muted)),
    ]));
    frame.render_widget(hint, chunks[1]);
}

pub fn render_editor(frame: &mut Frame, area: Rect, app: &mut App) {
    let dialog = centered_rect(60, 26, area);
    frame.render_widget(Clear, dialog);

    let title = if app.agent_editor_id.is_some() { " ◈ Edit Agent " } else { " ◈ New Agent " };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(app.theme.accent))
        .title(Line::from(Span::styled(title, Style::default().fg(app.theme.accent).add_modifier(Modifier::BOLD))))
        .style(Style::default().bg(app.theme.bg_panel));

    let inner = block.inner(dialog);
    frame.render_widget(block, dialog);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Length(2),
            Constraint::Min(6),
            Constraint::Length(1),
        ])
        .split(inner);

    let field_style = |idx: usize| -> Style {
        if app.agent_editor_field == idx {
            Style::default().fg(app.theme.accent)
        } else {
            Style::default().fg(app.theme.text_muted)
        }
    };

    let name_block = Block::default()
        .borders(Borders::BOTTOM)
        .border_style(field_style(0))
        .title(Span::styled("Name", field_style(0)));
    frame.render_widget(
        Paragraph::new(app.agent_editor_name.as_str())
            .style(Style::default().fg(app.theme.text))
            .block(name_block),
        chunks[0],
    );

    let desc_block = Block::default()
        .borders(Borders::BOTTOM)
        .border_style(field_style(1))
        .title(Span::styled("Description", field_style(1)));
    frame.render_widget(
        Paragraph::new(app.agent_editor_desc.as_str())
            .style(Style::default().fg(app.theme.text))
            .block(desc_block),
        chunks[1],
    );

    let sys_block = Block::default()
        .borders(Borders::ALL)
        .border_style(field_style(2))
        .title(Span::styled("System Prompt", field_style(2)));
    let sys_inner = sys_block.inner(chunks[2]);
    frame.render_widget(sys_block, chunks[2]);
    frame.render_widget(&app.agent_editor_system, sys_inner);

    let hint = Paragraph::new(Line::from(vec![
        Span::styled(" Tab", Style::default().fg(app.theme.accent)),
        Span::styled(" next field  ", Style::default().fg(app.theme.text_muted)),
        Span::styled("Ctrl+S", Style::default().fg(app.theme.accent)),
        Span::styled(" save  ", Style::default().fg(app.theme.text_muted)),
        Span::styled("Esc", Style::default().fg(app.theme.accent)),
        Span::styled(" cancel", Style::default().fg(app.theme.text_muted)),
    ]));
    frame.render_widget(hint, chunks[3]);
}
