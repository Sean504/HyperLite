/// Command palette (Ctrl+K) — tabbed layout.

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph};
use crate::app::App;
use super::centered_rect;

pub const TABS: &[&str] = &["Sessions", "Display", "Actions", "Options"];

#[derive(Clone)]
pub struct Command {
    pub label:    &'static str,
    pub desc:     &'static str,
    pub shortcut: &'static str,
}

pub fn commands_for_tab(tab: usize) -> Vec<Command> {
    match tab {
        // ── Sessions / Model / Folder ─────────────────────────────────────────
        0 => vec![
            Command { label: "New Session",      desc: "Start a blank session",          shortcut: "Ctrl+N" },
            Command { label: "Switch Session",   desc: "Browse session history",         shortcut: "Ctrl+S" },
            Command { label: "Fork Session",     desc: "Clone current session",          shortcut: "" },
            Command { label: "Delete Session",   desc: "Remove current session",         shortcut: "Ctrl+W" },
            Command { label: "Rename Session",   desc: "Rename current session",         shortcut: "Ctrl+R" },
            Command { label: "Compact Session",  desc: "Summarize and compress history", shortcut: "" },
            Command { label: "Switch Model",     desc: "Choose a local model",           shortcut: "Ctrl+M" },
            Command { label: "Cycle Model Next", desc: "Next model in list",             shortcut: "Alt+M" },
            Command { label: "Switch Agent",     desc: "Change agent mode (General/Build/Plan/Custom)", shortcut: "Ctrl+A" },
            Command { label: "Stash Draft",      desc: "Save input to draft stash",      shortcut: "Ctrl+D" },
            Command { label: "Pop Draft",        desc: "Restore a stashed draft",        shortcut: "Ctrl+Shift+D" },
            Command { label: "Open Folder",      desc: "Open a repo or directory",       shortcut: "Ctrl+O" },
        ],
        // ── Display / Settings ────────────────────────────────────────────────
        1 => vec![
            Command { label: "Pick Theme",           desc: "Switch color theme",             shortcut: "" },
            Command { label: "Toggle Sidebar",       desc: "Show/hide sidebar panel",        shortcut: "Ctrl+\\" },
            Command { label: "Toggle Thinking",      desc: "Show/hide reasoning blocks",     shortcut: "Ctrl+T" },
            Command { label: "Toggle Tool Details",  desc: "Expand/collapse tool output",    shortcut: "Ctrl+H" },
            Command { label: "Toggle Conceal",       desc: "Hide/show code blocks",          shortcut: "Ctrl+/" },
        ],
        // ── Actions ───────────────────────────────────────────────────────────
        2 => vec![
            Command { label: "Open in Editor",       desc: "Edit input in $EDITOR",          shortcut: "Ctrl+E" },
            Command { label: "Copy Last Response",   desc: "Copy assistant message",         shortcut: "Ctrl+C" },
            Command { label: "Undo Last Message",    desc: "Remove last exchange",           shortcut: "Ctrl+Z" },
        ],
        // ── Options ───────────────────────────────────────────────────────────
        3 => vec![
            Command { label: "Help",  desc: "Show keybinding reference", shortcut: "?" },
            Command { label: "Quit",  desc: "Exit HyperLite",            shortcut: "Ctrl+X" },
        ],
        _ => vec![],
    }
}

/// Flat list of all commands across all tabs — used for confirm_dialog label matching.
pub fn all_commands() -> Vec<Command> {
    (0..TABS.len()).flat_map(commands_for_tab).collect()
}

pub fn render(frame: &mut Frame, area: Rect, app: &mut App) {
    let dialog = centered_rect(92, 28, area);
    frame.render_widget(Clear, dialog);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(app.theme.accent))
        .title(Line::from(vec![
            Span::styled(" ⌘ Command Palette ", Style::default().fg(app.theme.accent).add_modifier(Modifier::BOLD)),
        ]))
        .style(Style::default().bg(app.theme.bg_panel));

    let inner = block.inner(dialog);
    frame.render_widget(block, dialog);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // tabs
            Constraint::Length(3), // search
            Constraint::Min(1),    // list
            Constraint::Length(1), // hint
        ])
        .split(inner);

    // ── Tab bar ───────────────────────────────────────────────────────────────
    let tab_line: Vec<Span> = TABS.iter().enumerate().flat_map(|(i, name)| {
        let active = i == app.command_palette_tab;
        let style = if active {
            Style::default().fg(app.theme.bg).bg(app.theme.primary).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(app.theme.text_muted)
        };
        let sep = if i + 1 < TABS.len() {
            Span::styled("  ", Style::default())
        } else {
            Span::raw("")
        };
        vec![Span::styled(format!(" {} ", name), style), sep]
    }).collect();
    frame.render_widget(Paragraph::new(Line::from(tab_line)), chunks[0]);

    // ── Search input ──────────────────────────────────────────────────────────
    let search_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(app.theme.border_hi))
        .style(Style::default().bg(app.theme.bg_element));
    let search_inner = search_block.inner(chunks[1]);
    frame.render_widget(search_block, chunks[1]);

    let query = &app.dialog_search_query;
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(" > ", Style::default().fg(app.theme.primary)),
            Span::styled(query.clone(), Style::default().fg(app.theme.text)),
            Span::styled("█", Style::default().fg(app.theme.accent)),
        ])),
        search_inner,
    );

    // ── Filtered command list for the active tab ──────────────────────────────
    let commands = commands_for_tab(app.command_palette_tab);
    let filtered: Vec<&Command> = commands.iter().filter(|c| {
        query.is_empty() ||
        c.label.to_lowercase().contains(&query.to_lowercase()) ||
        c.desc.to_lowercase().contains(&query.to_lowercase())
    }).collect();

    // Layout: highlight(2) + indent(2) + label(22) + desc(fills) + shortcut(14)
    // highlight_symbol "► " takes 2 chars that must be subtracted from usable width
    let inner_w      = dialog.width.saturating_sub(2) as usize; // strip borders
    let label_col    = 22usize;
    let shortcut_col = 14usize; // "  " + up to 12-char shortcut
    let highlight    = 2usize;  // "► " prefix on selected row (spaces on others)
    let desc_max     = inner_w.saturating_sub(highlight + 2 + label_col + shortcut_col);

    let items: Vec<ListItem> = filtered.iter().map(|c| {
        let desc_text = if c.desc.len() > desc_max {
            format!("{}…", &c.desc[..desc_max.saturating_sub(1)])
        } else {
            c.desc.to_string()
        };
        ListItem::new(Line::from(vec![
            Span::styled(
                format!("  {:<label_col$}", c.label),
                Style::default().fg(app.theme.text).add_modifier(Modifier::BOLD),
            ),
            Span::styled(desc_text, Style::default().fg(app.theme.text_muted)),
            Span::styled(
                format!("  {}", c.shortcut),
                Style::default().fg(app.theme.text_dim),
            ),
        ]))
    }).collect();

    let mut list_state = ListState::default();
    list_state.select(Some(app.dialog_selected_idx.min(filtered.len().saturating_sub(1))));

    let list = List::new(items)
        .highlight_style(Style::default().fg(app.theme.bg).bg(app.theme.primary))
        .highlight_symbol("► ");
    frame.render_stateful_widget(list, chunks[2], &mut list_state);

    // ── Hint ──────────────────────────────────────────────────────────────────
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(" Tab switch tab  ↑↓ navigate  Enter run  Esc close", Style::default().fg(app.theme.text_dim)),
        ])),
        chunks[3],
    );
}
