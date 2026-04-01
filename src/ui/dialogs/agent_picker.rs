/// Agent picker dialog — switch between General, Build, Plan, and custom agents.
/// Also contains the agent editor form for creating/editing custom agents.

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph};
use crate::app::{ActiveDialog, App};
use crate::tools::BUILTIN_AGENTS;
use super::centered_rect;

pub fn render(frame: &mut Frame, area: Rect, app: &mut App) {
    let dialog = centered_rect(56, 22, area);
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

    // Build list: built-ins first, then custom agents, then "New Agent…"
    let mut items: Vec<ListItem> = Vec::new();
    let mut agent_ids: Vec<String> = Vec::new();

    for agent in BUILTIN_AGENTS {
        let active = app.current_agent == agent.id;
        let marker = if active { "● " } else { "  " };
        let style = if active {
            Style::default().fg(app.theme.accent).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(app.theme.fg)
        };
        items.push(ListItem::new(Line::from(vec![
            Span::styled(format!("{}{}", marker, agent.name), style),
            Span::styled(
                format!("  — {}", agent.description),
                Style::default().fg(app.theme.fg_dim),
            ),
        ])));
        agent_ids.push(agent.id.to_string());
    }

    for agent in &app.custom_agents {
        let active = app.current_agent == agent.id;
        let marker = if active { "● " } else { "  " };
        let style = if active {
            Style::default().fg(app.theme.accent).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(app.theme.fg)
        };
        let desc = agent.description.as_deref().unwrap_or("Custom agent");
        items.push(ListItem::new(Line::from(vec![
            Span::styled(format!("{}{}", marker, agent.name), style),
            Span::styled(format!("  — {}", desc), Style::default().fg(app.theme.fg_dim)),
        ])));
        agent_ids.push(agent.id.clone());
    }

    // "New Agent…" entry
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
        .highlight_style(Style::default().bg(app.theme.selection).fg(app.theme.fg).add_modifier(Modifier::BOLD))
        .highlight_symbol("▶ ");

    frame.render_stateful_widget(list, chunks[0], &mut state);

    let hint = Paragraph::new(Line::from(vec![
        Span::styled(" Enter", Style::default().fg(app.theme.accent)),
        Span::styled(" select  ", Style::default().fg(app.theme.fg_dim)),
        Span::styled("d", Style::default().fg(app.theme.accent)),
        Span::styled(" delete custom  ", Style::default().fg(app.theme.fg_dim)),
        Span::styled("Esc", Style::default().fg(app.theme.accent)),
        Span::styled(" close", Style::default().fg(app.theme.fg_dim)),
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
            Constraint::Length(2), // Name field
            Constraint::Length(2), // Description field
            Constraint::Min(6),    // System prompt textarea
            Constraint::Length(1), // Hint
        ])
        .split(inner);

    let field_style = |idx: usize| {
        if app.agent_editor_field == idx {
            Style::default().fg(app.theme.accent)
        } else {
            Style::default().fg(app.theme.fg_dim)
        }
    };

    // Name
    let name_block = Block::default()
        .borders(Borders::BOTTOM)
        .border_style(field_style(0))
        .title(Span::styled("Name", field_style(0)));
    let name_text = Paragraph::new(app.agent_editor_name.as_str())
        .style(Style::default().fg(app.theme.fg))
        .block(name_block);
    frame.render_widget(name_text, chunks[0]);

    // Description
    let desc_block = Block::default()
        .borders(Borders::BOTTOM)
        .border_style(field_style(1))
        .title(Span::styled("Description", field_style(1)));
    let desc_text = Paragraph::new(app.agent_editor_desc.as_str())
        .style(Style::default().fg(app.theme.fg))
        .block(desc_block);
    frame.render_widget(desc_text, chunks[1]);

    // System prompt textarea
    let sys_block = Block::default()
        .borders(Borders::ALL)
        .border_style(field_style(2))
        .title(Span::styled("System Prompt", field_style(2)));
    let sys_inner = sys_block.inner(chunks[2]);
    frame.render_widget(sys_block, chunks[2]);
    frame.render_widget(app.agent_editor_system.widget(), sys_inner);

    // Hint
    let hint = Paragraph::new(Line::from(vec![
        Span::styled(" Tab", Style::default().fg(app.theme.accent)),
        Span::styled(" next field  ", Style::default().fg(app.theme.fg_dim)),
        Span::styled("Ctrl+S", Style::default().fg(app.theme.accent)),
        Span::styled(" save  ", Style::default().fg(app.theme.fg_dim)),
        Span::styled("Esc", Style::default().fg(app.theme.accent)),
        Span::styled(" cancel", Style::default().fg(app.theme.fg_dim)),
    ]));
    frame.render_widget(hint, chunks[3]);
}
