pub mod session_list;
pub mod model_picker;
pub mod help;
pub mod command;
pub mod theme_picker;
pub mod agent_picker;
pub mod draft_picker;

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph};
use crate::app::{ActiveDialog, App};

pub fn render(frame: &mut Frame, area: Rect, app: &mut App) {
    match &app.active_dialog {
        ActiveDialog::None => {}
        ActiveDialog::SessionList    => session_list::render(frame, area, app),
        ActiveDialog::ModelPicker    => model_picker::render(frame, area, app),
        ActiveDialog::Help           => help::render(frame, area, app),
        ActiveDialog::CommandPalette => command::render(frame, area, app),
        ActiveDialog::ThemePicker    => theme_picker::render(frame, area, app),
        ActiveDialog::FolderInput    => render_folder_input(frame, area, app),
        ActiveDialog::AgentPicker    => agent_picker::render(frame, area, app),
        ActiveDialog::AgentEditor    => agent_picker::render_editor(frame, area, app),
        ActiveDialog::DraftPicker    => draft_picker::render(frame, area, app),
    }
}

fn render_folder_input(frame: &mut Frame, area: Rect, app: &mut App) {
    let dialog = centered_rect(60, 24, area);
    frame.render_widget(Clear, dialog);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(app.theme.accent))
        .title(Line::from(vec![
            Span::styled(
                "  Open Folder / Repo ",
                Style::default().fg(app.theme.accent).add_modifier(Modifier::BOLD),
            ),
        ]))
        .style(Style::default().bg(app.theme.bg_panel));

    let inner = block.inner(dialog);
    frame.render_widget(block, dialog);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // current path
            Constraint::Length(1), // separator space
            Constraint::Min(8),    // directory list
            Constraint::Length(1), // separator space
            Constraint::Length(1), // type path input
            Constraint::Length(1), // hint
        ])
        .split(inner);

    // ── Current path ──────────────────────────────────────────────────────────
    let path_str = app.folder_browser_path.display().to_string();
    let max_path = chunks[0].width as usize;
    let display_path = if path_str.len() > max_path {
        format!("…{}", &path_str[path_str.len().saturating_sub(max_path - 1)..])
    } else {
        path_str.clone()
    };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(" ", Style::default()),
            Span::styled(display_path, Style::default().fg(app.theme.primary).add_modifier(Modifier::BOLD)),
        ])),
        chunks[0],
    );

    // ── Directory list ────────────────────────────────────────────────────────
    let entries = &app.folder_browser_entries;
    let items: Vec<ListItem> = entries.iter().enumerate().map(|(i, name)| {
        let (icon, style) = if i == 0 {
            // "Select this folder"
            ("✓ ", Style::default().fg(app.theme.success).add_modifier(Modifier::BOLD))
        } else if name == ".." {
            ("↑ ", Style::default().fg(app.theme.text_muted))
        } else {
            (" ", Style::default().fg(app.theme.text))
        };
        ListItem::new(Line::from(vec![
            Span::styled(format!("  {}{}", icon, name), style),
        ]))
    }).collect();

    let mut list_state = ListState::default();
    if !app.folder_input_buf.is_empty() {
        list_state.select(None);
    } else {
        list_state.select(Some(app.dialog_selected_idx.min(entries.len().saturating_sub(1))));
    }

    let list_block = Block::default()
        .borders(Borders::TOP | Borders::BOTTOM)
        .border_style(Style::default().fg(app.theme.border));
    let list = List::new(items)
        .block(list_block)
        .highlight_style(Style::default().fg(app.theme.bg).bg(app.theme.accent))
        .highlight_symbol("► ");
    frame.render_stateful_widget(list, chunks[2], &mut list_state);

    // ── Type-a-path input ─────────────────────────────────────────────────────
    let typing = &app.folder_input_buf;
    let input_style = if typing.is_empty() {
        Style::default().fg(app.theme.text_dim)
    } else {
        Style::default().fg(app.theme.text)
    };
    let input_text = if typing.is_empty() {
        " Type a path…".to_string()
    } else {
        format!(" {}", typing)
    };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(input_text, input_style),
            if !typing.is_empty() {
                Span::styled("█", Style::default().fg(app.theme.accent))
            } else {
                Span::raw("")
            },
        ])),
        chunks[4],
    );

    // ── Hint ──────────────────────────────────────────────────────────────────
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                " ↑↓ navigate  Enter open/select  ⌫ go up  Esc cancel",
                Style::default().fg(app.theme.text_dim),
            ),
        ])),
        chunks[5],
    );
}

/// Centered rect helper used by all dialogs.
pub fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let x = area.x + area.width.saturating_sub(width) / 2;
    let y = area.y + area.height.saturating_sub(height) / 2;
    Rect {
        x,
        y,
        width: width.min(area.width),
        height: height.min(area.height),
    }
}
