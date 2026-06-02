/// Help dialog — tabbed reference: Shortcuts / Indexing & Memory / Agents / Tools

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use crate::app::App;
use super::centered_rect;

const TABS: &[&str] = &["Shortcuts", "Indexing & Memory", "Agents", "Tools"];

pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let dialog = centered_rect(84, 40, area);
    frame.render_widget(Clear, dialog);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(app.theme.primary))
        .title(Line::from(vec![
            Span::styled(" Help ", Style::default().fg(app.theme.primary).add_modifier(Modifier::BOLD)),
            Span::styled(" Tab/←/→ to switch  ·  Esc to close ", Style::default().fg(app.theme.text_dim)),
        ]))
        .style(Style::default().bg(app.theme.bg_panel));

    let inner = block.inner(dialog);
    frame.render_widget(block, dialog);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1)])
        .split(inner);

    // Tab bar
    let tab_line: Vec<Span> = TABS.iter().enumerate().flat_map(|(i, name)| {
        let active = i == app.help_tab;
        let style = if active {
            Style::default().fg(app.theme.accent).add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
        } else {
            Style::default().fg(app.theme.text_muted)
        };
        vec![
            Span::styled(format!(" {} ", name), style),
            Span::styled(" │ ", Style::default().fg(app.theme.border)),
        ]
    }).collect();
    frame.render_widget(Paragraph::new(Line::from(tab_line)), chunks[0]);

    match app.help_tab {
        0 => render_shortcuts(frame, chunks[1], app),
        1 => render_indexing(frame, chunks[1], app),
        2 => render_agents(frame, chunks[1], app),
        3 => render_tools(frame, chunks[1], app),
        _ => {}
    }
}

// ── Shortcuts ─────────────────────────────────────────────────────────────────

fn render_shortcuts(frame: &mut Frame, area: Rect, app: &App) {
    let sections = app.keybinds.help_sections();
    let n = sections.len();
    let half = n.div_ceil(2);

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

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
                    Span::styled(format!("  {:>12}  ", key), Style::default().fg(app.theme.primary).add_modifier(Modifier::BOLD)),
                    Span::styled(desc.clone(), Style::default().fg(app.theme.text)),
                ]));
            }
            lines.push(Line::default());
        }

        frame.render_widget(Paragraph::new(lines), *chunk);
    }
}

// ── Indexing & Memory ─────────────────────────────────────────────────────────

fn render_indexing(frame: &mut Frame, area: Rect, app: &App) {
    let h = Style::default().fg(app.theme.accent).add_modifier(Modifier::BOLD | Modifier::UNDERLINED);
    let t = Style::default().fg(app.theme.text);
    let k = Style::default().fg(app.theme.primary).add_modifier(Modifier::BOLD);
    let d = Style::default().fg(app.theme.text_muted);

    let lines: Vec<Line<'static>> = vec![
        Line::from(vec![Span::styled("Folder Indexing (RAG)", h)]),
        Line::default(),
        Line::from(vec![Span::styled("  What it does:", k)]),
        Line::from(vec![Span::styled("  Scans your open folder and builds a semantic search index. The AI can then", d)]),
        Line::from(vec![Span::styled("  search your codebase by meaning rather than exact keywords.", d)]),
        Line::default(),
        Line::from(vec![Span::styled("  How to use:", k)]),
        Line::from(vec![Span::styled("  Ctrl+K → Agent tab → Index Folder   Build an index for the current folder", t)]),
        Line::from(vec![Span::styled("  Ctrl+K → Agent tab → Search Index   Manually search the index", t)]),
        Line::from(vec![Span::styled("  Ctrl+K → Agent tab → Clear Index    Delete the index for this folder", t)]),
        Line::from(vec![Span::styled("  Ctrl+K → Agent tab → List Indexes   Show all indexed folders", t)]),
        Line::default(),
        Line::from(vec![Span::styled("  The AI automatically searches your index during coding tasks.", d)]),
        Line::from(vec![Span::styled("  The sidebar shows ◆ indexed when an index exists for the current folder.", d)]),
        Line::default(),
        Line::default(),
        Line::from(vec![Span::styled("Memory", h)]),
        Line::default(),
        Line::from(vec![Span::styled("  What it does:", k)]),
        Line::from(vec![Span::styled("  Stores facts that persist across all sessions. The AI recalls them", d)]),
        Line::from(vec![Span::styled("  automatically when relevant.", d)]),
        Line::default(),
        Line::from(vec![Span::styled("  How to use:", k)]),
        Line::from(vec![Span::styled("  Ctrl+K → Agent tab → Save Memory    Save a fact (e.g. \"I prefer TypeScript\")", t)]),
        Line::from(vec![Span::styled("  Ctrl+K → Agent tab → View Memory    List all stored memories", t)]),
        Line::from(vec![Span::styled("  Ctrl+K → Agent tab → Clear Memory   Delete all memories", t)]),
        Line::default(),
        Line::from(vec![Span::styled("  The sidebar shows ◆ N memories when memories exist.", d)]),
        Line::from(vec![Span::styled("  You can also just tell the AI \"remember X\" and it will save it.", d)]),
    ];

    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
}

// ── Agents ────────────────────────────────────────────────────────────────────

fn render_agents(frame: &mut Frame, area: Rect, app: &App) {
    let h = Style::default().fg(app.theme.accent).add_modifier(Modifier::BOLD | Modifier::UNDERLINED);
    let t = Style::default().fg(app.theme.text);
    let k = Style::default().fg(app.theme.primary).add_modifier(Modifier::BOLD);
    let d = Style::default().fg(app.theme.text_muted);

    let lines: Vec<Line<'static>> = vec![
        Line::from(vec![Span::styled("Built-in Agents", h)]),
        Line::default(),
        Line::from(vec![Span::styled("  General   ", k), Span::styled("Conversational assistant with full tool access.", t)]),
        Line::from(vec![Span::styled("  Build     ", k), Span::styled("Expert coding agent — reads, writes, builds, runs shell commands.", t)]),
        Line::from(vec![Span::styled("  Plan      ", k), Span::styled("Read-only analysis agent. Explores code but won't write or run commands.", t)]),
        Line::default(),
        Line::from(vec![Span::styled("  Switch agents:  Ctrl+A  or  Ctrl+K → Agent tab → Switch Agent", d)]),
        Line::default(),
        Line::default(),
        Line::from(vec![Span::styled("Custom Agents", h)]),
        Line::default(),
        Line::from(vec![Span::styled("  Create your own agents with a custom name, description, and system prompt.", d)]),
        Line::from(vec![Span::styled("  You can also restrict which tools the agent is allowed to use.", d)]),
        Line::default(),
        Line::from(vec![Span::styled("  How to create:", k)]),
        Line::from(vec![Span::styled("  1. Ctrl+A → New Agent", t)]),
        Line::from(vec![Span::styled("  2. Enter a name, short description, and system prompt", t)]),
        Line::from(vec![Span::styled("  3. Optionally restrict tools (comma-separated list, e.g. read_file,shell)", t)]),
        Line::from(vec![Span::styled("  4. Save — the agent is now available in Ctrl+A", t)]),
        Line::default(),
        Line::from(vec![Span::styled("  System prompt tips:", k)]),
        Line::from(vec![Span::styled("  — Be specific about the agent's role and constraints", d)]),
        Line::from(vec![Span::styled("  — Mention preferred output format, tone, or domain expertise", d)]),
        Line::from(vec![Span::styled("  — Use \"You MUST\" / \"You MUST NOT\" for hard constraints", d)]),
        Line::default(),
        Line::from(vec![Span::styled("  Example system prompt:", k)]),
        Line::from(vec![Span::styled("  \"You are a security auditor. Only read files — never write or execute.", d)]),
        Line::from(vec![Span::styled("   Report findings as a numbered list with severity ratings.\"", d)]),
    ];

    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
}

// ── Tools ─────────────────────────────────────────────────────────────────────

fn render_tools(frame: &mut Frame, area: Rect, app: &App) {
    let h = Style::default().fg(app.theme.accent).add_modifier(Modifier::BOLD | Modifier::UNDERLINED);
    let t = Style::default().fg(app.theme.text);
    let k = Style::default().fg(app.theme.primary).add_modifier(Modifier::BOLD);
    let d = Style::default().fg(app.theme.text_muted);

    let lines: Vec<Line<'static>> = vec![
        Line::from(vec![Span::styled("Available Tools", h)]),
        Line::default(),
        Line::from(vec![Span::styled("  read_file / batch_read  ", k), Span::styled("Read one or many files at once", t)]),
        Line::from(vec![Span::styled("  write_file              ", k), Span::styled("Create or overwrite a file (triggers diff approval)", t)]),
        Line::from(vec![Span::styled("  edit_file               ", k), Span::styled("Search & replace within a file (triggers diff approval)", t)]),
        Line::from(vec![Span::styled("  shell                   ", k), Span::styled("Run a shell command — requires permission", t)]),
        Line::from(vec![Span::styled("  grep / glob             ", k), Span::styled("Search file contents or find files by pattern", t)]),
        Line::from(vec![Span::styled("  tree / list_dir         ", k), Span::styled("Explore directory structure", t)]),
        Line::from(vec![Span::styled("  search                  ", k), Span::styled("Web search via DuckDuckGo", t)]),
        Line::from(vec![Span::styled("  http_fetch              ", k), Span::styled("Fetch a URL and return its content", t)]),
        Line::from(vec![Span::styled("  git_status/log/diff     ", k), Span::styled("Git operations on the current folder", t)]),
        Line::from(vec![Span::styled("  index_dir / search_index", k), Span::styled("RAG — index and search your codebase semantically", t)]),
        Line::default(),
        Line::default(),
        Line::from(vec![Span::styled("Diff Approval", h)]),
        Line::default(),
        Line::from(vec![Span::styled("  write_file and edit_file show a visual diff before applying changes.", d)]),
        Line::from(vec![Span::styled("  Press Y to approve, N to reject. The AI continues after your decision.", d)]),
        Line::default(),
        Line::default(),
        Line::from(vec![Span::styled("Sandbox Mode", h)]),
        Line::default(),
        Line::from(vec![Span::styled("  When sandbox is enabled, shell commands run inside bubblewrap isolation.", d)]),
        Line::from(vec![Span::styled("  Filesystem changes don't escape the sandbox — safe for untrusted prompts.", d)]),
        Line::from(vec![Span::styled("  Ctrl+K → Agent tab → Enable Sandbox / Disable Sandbox", t)]),
        Line::default(),
        Line::default(),
        Line::from(vec![Span::styled("Tool Permissions", h)]),
        Line::default(),
        Line::from(vec![Span::styled("  Tools marked requires_permission=true prompt before running.", d)]),
        Line::from(vec![Span::styled("  shell, write_file, edit_file, delete_file all require approval.", d)]),
        Line::from(vec![Span::styled("  You can approve once or reject — the AI adapts to your decision.", d)]),
    ];

    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
}
