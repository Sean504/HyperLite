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
use ratatui::widgets::Wrap as _Wrap;

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
        ActiveDialog::GitConfirm     => render_git_confirm(frame, area, app),
        ActiveDialog::IndexConfirm   => render_index_confirm(frame, area, app),
        ActiveDialog::RagSearch      => render_rag_search(frame, area, app),
        ActiveDialog::MemoryInput    => render_memory_input(frame, area, app),
        ActiveDialog::BwrapInstall   => render_bwrap_install(frame, area, app),
        ActiveDialog::GitToken            => render_git_token(frame, area, app),
        ActiveDialog::PenTestAuth         => render_pentest_auth(frame, area, app),
        ActiveDialog::PenTestPreflight    => render_pentest_preflight(frame, area, app),
        ActiveDialog::PenTestToolSelector => render_pentest_selector(frame, area, app),
        ActiveDialog::PenTestInstall      => render_pentest_install(frame, area, app),
        ActiveDialog::PenTestSetup        => render_pentest_setup(frame, area, app),
    }
}

fn render_memory_input(frame: &mut Frame, area: Rect, app: &mut App) {
    let dialog = centered_rect(52, 6, area);
    frame.render_widget(Clear, dialog);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(app.theme.accent))
        .title(Line::from(vec![
            Span::styled(" Save to Memory ", Style::default().fg(app.theme.accent)),
        ]));
    let inner = block.inner(dialog);
    frame.render_widget(block, dialog);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);

    frame.render_widget(
        Paragraph::new(vec![
            Line::from(vec![
                Span::styled("  Type a fact for the AI to remember across all sessions.", Style::default().fg(app.theme.text_dim)),
            ]),
            Line::from(vec![
                Span::styled(" > ", Style::default().fg(app.theme.primary)),
                Span::styled(app.dialog_search_query.clone(), Style::default().fg(app.theme.text)),
                Span::styled("█", Style::default().fg(app.theme.accent)),
            ]),
        ]),
        chunks[0],
    );

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("  Enter to save  ·  Esc to cancel", Style::default().fg(app.theme.text_dim)),
        ])),
        chunks[1],
    );
}

fn render_rag_search(frame: &mut Frame, area: Rect, app: &mut App) {
    let dialog = centered_rect(52, 6, area);
    frame.render_widget(Clear, dialog);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(app.theme.accent))
        .title(Line::from(vec![
            Span::styled(" Search Index ", Style::default().fg(app.theme.accent)),
        ]));
    let inner = block.inner(dialog);
    frame.render_widget(block, dialog);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);

    let folder = app.working_dir.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| app.working_dir.to_string_lossy().to_string());

    frame.render_widget(
        Paragraph::new(vec![
            Line::from(vec![
                Span::styled("  Folder: ", Style::default().fg(app.theme.text_dim)),
                Span::styled(folder, Style::default().fg(app.theme.text_dim)),
            ]),
            Line::from(vec![
                Span::styled(" > ", Style::default().fg(app.theme.primary)),
                Span::styled(app.dialog_search_query.clone(), Style::default().fg(app.theme.text)),
                Span::styled("█", Style::default().fg(app.theme.accent)),
            ]),
        ]),
        chunks[0],
    );

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("  Enter to search  ·  Esc to cancel", Style::default().fg(app.theme.text_dim)),
        ])),
        chunks[1],
    );
}

fn render_git_confirm(frame: &mut Frame, area: Rect, app: &mut App) {
    let folder = app.working_dir.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| app.working_dir.to_string_lossy().to_string());

    let branch = app.project_ctx.as_ref()
        .and_then(|c| c.git_branch.clone())
        .unwrap_or_else(|| "main".to_string());

    let dialog = centered_rect(58, 8, area);
    frame.render_widget(Clear, dialog);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(app.theme.primary))
        .title(Line::from(vec![
            Span::styled(" Git repository detected ", Style::default().fg(app.theme.primary)),
        ]));
    let inner = block.inner(dialog);
    frame.render_widget(block, dialog);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);

    frame.render_widget(
        Paragraph::new(vec![
            Line::from(vec![
                Span::styled("  Folder: ", Style::default().fg(app.theme.text_dim)),
                Span::styled(folder, Style::default().fg(app.theme.text).add_modifier(Modifier::BOLD)),
                Span::styled("  (", Style::default().fg(app.theme.text_dim)),
                Span::styled(branch, Style::default().fg(app.theme.accent)),
                Span::styled(")", Style::default().fg(app.theme.text_dim)),
            ]),
            Line::from(vec![
                Span::styled(
                    "  Would you like to enable the Git Agent?",
                    Style::default().fg(app.theme.text),
                ),
            ]),
            Line::from(vec![
                Span::styled(
                    "  Injects branch, status and diff into every prompt.",
                    Style::default().fg(app.theme.text_dim),
                ),
            ]),
            Line::from(vec![
                Span::styled(
                    "  You can always toggle this in the Agent tab of the command palette.",
                    Style::default().fg(app.theme.text_dim),
                ),
            ]),
        ]),
        chunks[0],
    );

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("  Enter / Y  →  enable    ", Style::default().fg(app.theme.success)),
            Span::styled("Esc / N  →  not now", Style::default().fg(app.theme.text_dim)),
        ])),
        chunks[1],
    );
}

fn render_index_confirm(frame: &mut Frame, area: Rect, app: &mut App) {
    let folder = app.working_dir.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| app.working_dir.to_string_lossy().to_string());

    let dialog = centered_rect(52, 7, area);
    frame.render_widget(Clear, dialog);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(app.theme.accent))
        .title(Line::from(vec![
            Span::styled(" Index folder for AI context? ", Style::default().fg(app.theme.accent)),
        ]));
    let inner = block.inner(dialog);
    frame.render_widget(block, dialog);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);

    frame.render_widget(
        Paragraph::new(vec![
            Line::from(vec![
                Span::styled("  Folder: ", Style::default().fg(app.theme.text_dim)),
                Span::styled(folder, Style::default().fg(app.theme.text).add_modifier(Modifier::BOLD)),
            ]),
            Line::from(vec![
                Span::styled(
                    "  Indexes files so the AI can search your code semantically.",
                    Style::default().fg(app.theme.text_dim),
                ),
            ]),
            Line::from(vec![
                Span::styled(
                    "  First run downloads embedding model (~22 MB). Scoped to this folder.",
                    Style::default().fg(app.theme.text_dim),
                ),
            ]),
        ]),
        chunks[0],
    );

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("  Enter / Y  →  index now    ", Style::default().fg(app.theme.success)),
            Span::styled("Esc / N  →  skip", Style::default().fg(app.theme.text_dim)),
        ])),
        chunks[1],
    );
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

fn render_bwrap_install(frame: &mut Frame, area: Rect, app: &App) {
    let purple = app.theme.primary;
    let teal   = app.theme.accent;
    let green  = app.theme.success;
    let orange = app.theme.warning;
    let red    = app.theme.error;
    let dim    = app.theme.text_dim;
    let white  = app.theme.text;
    let bg     = app.theme.bg_panel;

    let dialog = centered_rect(70, 24, area);
    frame.render_widget(Clear, dialog);

    let (title, border_col) = if app.bwrap_sudo_prompt {
        (" Install bubblewrap — sudo password required ", orange)
    } else if app.bwrap_installing {
        (" Installing bubblewrap… ", purple)
    } else if app.bwrap_install_log.iter().any(|l| l.contains('✓')) {
        (" ✓  bubblewrap installed ", green)
    } else {
        (" ✗  Installation failed ", red)
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_col))
        .title(Line::from(vec![Span::styled(title, Style::default().fg(border_col).add_modifier(Modifier::BOLD))]))
        .style(Style::default().bg(bg));
    let inner = block.inner(dialog);
    frame.render_widget(block, dialog);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(3)])
        .split(inner);

    // Log output
    let log_h = chunks[0].height as usize;
    let log_start = app.bwrap_install_log.len().saturating_sub(log_h);
    let log_lines: Vec<Line<'static>> = app.bwrap_install_log[log_start..].iter().map(|l| {
        let col = if l.contains('✓')      { green }
            else if l.contains('✗')       { red   }
            else if l.contains("…")       { teal  }
            else                          { dim   };
        Line::from(vec![Span::styled(format!("  {}", l), Style::default().fg(col))])
    }).collect();
    frame.render_widget(Paragraph::new(log_lines), chunks[0]);

    // Bottom section: password prompt or status
    if app.bwrap_sudo_prompt {
        // Match the first-run sudo prompt style exactly
        let prompt_block = Block::default()
            .borders(Borders::TOP)
            .border_style(Style::default().fg(orange))
            .style(Style::default().bg(bg));
        let prompt_inner = prompt_block.inner(chunks[1]);
        frame.render_widget(prompt_block, chunks[1]);

        let dots = "●".repeat(app.bwrap_sudo_input.len());
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(vec![Span::styled("  Enter your sudo password:", Style::default().fg(white))]),
                Line::from(vec![Span::styled(format!("  {}_", dots), Style::default().fg(teal).add_modifier(Modifier::BOLD))]),
            ]),
            prompt_inner,
        );
    } else if app.bwrap_installing {
        let spinner = ["⠋","⠙","⠹","⠸","⠼","⠴","⠦","⠧","⠇","⠏"][0];
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(format!("  {}  installing…", spinner), Style::default().fg(teal)),
            ])).block(Block::default().borders(Borders::TOP).border_style(Style::default().fg(dim))),
            chunks[1],
        );
    } else {
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("  Esc to close", Style::default().fg(dim)),
            ])).block(Block::default().borders(Borders::TOP).border_style(Style::default().fg(dim))),
            chunks[1],
        );
    }
}

/// Centered rect helper used by all dialogs.
fn render_git_token(frame: &mut Frame, area: Rect, app: &App) {
    let accent  = app.theme.accent;
    let primary = app.theme.primary;
    let success = app.theme.success;
    let text    = app.theme.text;
    let dim     = app.theme.text_dim;
    let bg      = app.theme.bg_panel;

    let host = if app.git_token_host.is_empty() { "github.com" } else { &app.git_token_host };

    let dialog = centered_rect(66, 20, area);
    frame.render_widget(Clear, dialog);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(accent))
        .title(Line::from(vec![
            Span::styled(" Git Authentication Required ", Style::default().fg(accent).add_modifier(Modifier::BOLD)),
        ]))
        .style(Style::default().bg(bg));

    let inner = block.inner(dialog);
    frame.render_widget(block, dialog);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),  // host line
            Constraint::Length(1),  // spacer
            Constraint::Length(1),  // step 1 header
            Constraint::Length(2),  // step 1 path hint
            Constraint::Length(1),  // step 2
            Constraint::Length(1),  // step 3 header
            Constraint::Length(3),  // step 3 scopes
            Constraint::Length(1),  // step 4 header
            Constraint::Length(1),  // spacer
            Constraint::Length(1),  // token input
            Constraint::Length(1),  // spacer
            Constraint::Length(1),  // footer
        ])
        .split(inner);

    // Host
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("  Host: ", Style::default().fg(dim)),
            Span::styled(host.to_string(), Style::default().fg(primary).add_modifier(Modifier::BOLD)),
        ])),
        chunks[0],
    );

    // Step 1
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("  1. Open ", Style::default().fg(text)),
            Span::styled(
                format!("{}/settings/tokens/new", host),
                Style::default().fg(accent).add_modifier(Modifier::BOLD),
            ),
        ])),
        chunks[2],
    );
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(vec![Span::styled(
                "     (Profile photo → Settings → Developer settings",
                Style::default().fg(dim),
            )]),
            Line::from(vec![Span::styled(
                "      → Personal access tokens → Tokens (classic))",
                Style::default().fg(dim),
            )]),
        ]),
        chunks[3],
    );

    // Step 2
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("  2. Set a name like ", Style::default().fg(text)),
            Span::styled("\"HyperLite\"", Style::default().fg(accent)),
            Span::styled(" and pick an expiration", Style::default().fg(text)),
        ])),
        chunks[4],
    );

    // Step 3
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("  3. Check these boxes:", Style::default().fg(text)),
        ])),
        chunks[5],
    );
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(vec![
                Span::styled("     ✓ repo     ", Style::default().fg(success)),
                Span::styled("lets HyperLite push and pull your code", Style::default().fg(dim)),
            ]),
            Line::from(vec![
                Span::styled("     ✓ workflow ", Style::default().fg(success)),
                Span::styled("lets HyperLite update GitHub Actions", Style::default().fg(dim)),
            ]),
            Line::from(vec![
                Span::styled("     ✓ read:org ", Style::default().fg(success)),
                Span::styled("lets HyperLite access organisation repos", Style::default().fg(dim)),
            ]),
        ]),
        chunks[6],
    );

    // Step 4
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("  4. Click ", Style::default().fg(text)),
            Span::styled("\"Generate token\"", Style::default().fg(accent)),
            Span::styled(" — copy it and paste below:", Style::default().fg(text)),
        ])),
        chunks[7],
    );

    // Token input (masked)
    let dots = "●".repeat(app.git_token_input.len());
    let display = if app.git_token_input.is_empty() {
        Span::styled("  paste token here…", Style::default().fg(dim))
    } else {
        Span::styled(format!("  {}_", dots), Style::default().fg(accent).add_modifier(Modifier::BOLD))
    };
    frame.render_widget(
        Paragraph::new(Line::from(vec![display]))
            .block(Block::default().borders(Borders::TOP | Borders::BOTTOM).border_style(Style::default().fg(dim))),
        chunks[9],
    );

    // Footer
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("  Enter to save  ·  Esc to cancel", Style::default().fg(dim)),
        ])),
        chunks[11],
    );
}

// ── PenTest Auth Gate ─────────────────────────────────────────────────────────

fn render_pentest_auth(frame: &mut Frame, area: Rect, app: &App) {
    use crate::pentest::{AuthPhase, AUTH_CONTENT_LINES, AUTH_LINES_PER_TICK};

    // Pen test palette — red accent throughout
    let red    = ratatui::style::Color::Rgb(255, 42,  109);
    let white  = ratatui::style::Color::Rgb(220, 220, 240);
    let dim    = ratatui::style::Color::Rgb(80,  80,  110);
    let bg     = ratatui::style::Color::Rgb(6,   6,   16);
    let teal   = ratatui::style::Color::Rgb(0,   212, 255);

    // Determine border and background color based on flash state
    let (border_col, bg_col) = match &app.pentest_auth_phase {
        AuthPhase::FlashCorrect => (
            ratatui::style::Color::White,
            ratatui::style::Color::Rgb(0, 30, 0),
        ),
        AuthPhase::FlashWrong => (
            ratatui::style::Color::Rgb(255, 0, 0),
            ratatui::style::Color::Rgb(30, 0, 0),
        ),
        AuthPhase::BlackOut => (bg, bg),
        _ => (red, bg),
    };

    // Full-screen clear with background
    frame.render_widget(Clear, area);
    frame.render_widget(
        Block::default().style(Style::default().bg(bg_col)),
        area,
    );

    if app.pentest_auth_phase == AuthPhase::BlackOut {
        return;
    }

    // Outer red header block — full width, 3 lines tall
    let header_area = Rect { x: area.x, y: area.y, width: area.width, height: 3 };
    let header_text = " ⚠  AUTHORIZED ACCESS ONLY  ⚠ ";
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                header_text,
                Style::default()
                    .fg(ratatui::style::Color::White)
                    .bg(border_col)
                    .add_modifier(Modifier::BOLD),
            ),
        ]))
        .alignment(ratatui::layout::Alignment::Center)
        .block(Block::default().style(Style::default().bg(border_col))),
        header_area,
    );

    // Main content box — centered
    let box_w = area.width.min(72);
    let box_h = area.height.saturating_sub(6).min(28);
    let box_area = centered_rect(box_w, box_h, Rect {
        x: area.x, y: area.y + 4,
        width: area.width, height: area.height.saturating_sub(4),
    });

    frame.render_widget(Clear, box_area);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_col))
        .style(Style::default().bg(bg));
    let inner = block.inner(box_area);
    frame.render_widget(block, box_area);

    // Content lines to reveal based on animation tick
    let all_lines: Vec<Line<'static>> = vec![
        Line::from(""),
        Line::from(vec![Span::styled(
            "  HyperLite PenTest Mode provides access to security testing",
            Style::default().fg(white),
        )]),
        Line::from(vec![Span::styled(
            "  tools and automated attack workflows.",
            Style::default().fg(white),
        )]),
        Line::from(""),
        Line::from(vec![Span::styled(
            "  These capabilities exist for one purpose: authorized security",
            Style::default().fg(white),
        )]),
        Line::from(vec![Span::styled(
            "  research and professional penetration testing conducted with",
            Style::default().fg(white),
        )]),
        Line::from(vec![Span::styled(
            "  documented client permission.",
            Style::default().fg(white),
        )]),
        Line::from(""),
        Line::from(vec![Span::styled(
            "  ──────────────────────────────────────────────────────────",
            Style::default().fg(dim),
        )]),
        Line::from(""),
        Line::from(vec![Span::styled(
            "  BY PROCEEDING YOU CONFIRM:",
            Style::default().fg(white).add_modifier(Modifier::BOLD),
        )]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  ◆  ", Style::default().fg(red)),
            Span::styled("You hold written authorization for every target", Style::default().fg(white)),
        ]),
        Line::from(vec![Span::styled(
            "     in your defined scope",
            Style::default().fg(white),
        )]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  ◆  ", Style::default().fg(red)),
            Span::styled("Unauthorized use is illegal under the CFAA and", Style::default().fg(white)),
        ]),
        Line::from(vec![Span::styled(
            "     equivalent laws in your jurisdiction",
            Style::default().fg(white),
        )]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  ◆  ", Style::default().fg(red)),
            Span::styled("HyperLite provides these tools for authorized research", Style::default().fg(white)),
        ]),
        Line::from(vec![Span::styled(
            "     only — we do not condone unauthorized or malicious use",
            Style::default().fg(white),
        )]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  ◆  ", Style::default().fg(red)),
            Span::styled("We make no warranty regarding effectiveness and accept", Style::default().fg(white)),
        ]),
        Line::from(vec![Span::styled(
            "     no responsibility for use or any consequences",
            Style::default().fg(white),
        )]),
    ];

    let lines_to_show = match &app.pentest_auth_phase {
        AuthPhase::Revealing => {
            let n = (app.pentest_auth_tick as usize * AUTH_LINES_PER_TICK as usize)
                .min(all_lines.len());
            &all_lines[..n]
        }
        _ => all_lines.as_slice(),
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(3)])
        .split(inner);

    frame.render_widget(Paragraph::new(lines_to_show.to_vec()), chunks[0]);

    // Input prompt — only shown once content is fully revealed
    if matches!(app.pentest_auth_phase,
        AuthPhase::AwaitInput | AuthPhase::FlashWrong | AuthPhase::FlashCorrect)
    {
        let input_col = match &app.pentest_auth_phase {
            AuthPhase::FlashWrong   => ratatui::style::Color::Rgb(255, 60, 60),
            AuthPhase::FlashCorrect => ratatui::style::Color::Rgb(0, 255, 80),
            _                       => teal,
        };
        let dots = "●".repeat(app.pentest_auth_input.len());
        let placeholder = if app.pentest_auth_input.is_empty() {
            "  Type AUTHORIZED and press Enter…".to_string()
        } else {
            format!("  ▶  {}█", dots)
        };
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(placeholder, Style::default().fg(input_col).add_modifier(Modifier::BOLD)),
            ]))
            .block(Block::default()
                .borders(Borders::TOP)
                .border_style(Style::default().fg(dim))),
            chunks[1],
        );
    }

    // Esc hint
    if area.height > 6 {
        let hint_area = Rect {
            x: area.x, y: area.bottom().saturating_sub(1),
            width: area.width, height: 1,
        };
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("  Esc to cancel", Style::default().fg(dim)),
            ])),
            hint_area,
        );
    }
}

// ── PenTest Pre-flight ────────────────────────────────────────────────────────

fn render_pentest_preflight(frame: &mut Frame, area: Rect, app: &App) {
    use crate::pentest::{ALL_TOOLS, WORKFLOWS, ToolStatus, ToolCategory, compute_availability, WorkflowAvailability};

    let red    = ratatui::style::Color::Rgb(255, 42,  109);
    let cyan   = ratatui::style::Color::Rgb(0,   212, 255);
    let green  = ratatui::style::Color::Rgb(80,  250, 123);
    let yellow = ratatui::style::Color::Rgb(241, 250, 140);
    let white  = ratatui::style::Color::Rgb(220, 220, 240);
    let dim    = ratatui::style::Color::Rgb(80,  80,  110);
    let bg     = ratatui::style::Color::Rgb(6,   6,   16);
    let bg2    = ratatui::style::Color::Rgb(10,  10,  24);

    let dialog = centered_rect(74, (area.height.min(46)).max(20), area);
    frame.render_widget(Clear, dialog);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(red))
        .title(Line::from(vec![
            Span::styled(" PRE-FLIGHT CHECK ", Style::default().fg(red).add_modifier(Modifier::BOLD)),
        ]))
        .style(Style::default().bg(bg));
    let inner = block.inner(dialog);
    frame.render_widget(block, dialog);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // environment info
            Constraint::Length(1),  // spacer
            Constraint::Min(8),     // tool list
            Constraint::Length(1),  // spacer
            Constraint::Min(4),     // workflow availability
            Constraint::Length(1),  // footer
        ])
        .split(inner);

    // ── Environment ───────────────────────────────────────────────────────────
    let (env_text, cap_text) = if let Some(env) = &app.pentest_env {
        let caps = format!(
            "raw_sockets {}  wifi {}  msf_db {}",
            if env.capabilities.raw_sockets { "✓" } else { "✗" },
            if env.capabilities.wifi        { "✓" } else { "✗" },
            if env.capabilities.msf_database { "✓" } else { "✗" },
        );
        (env.env_type.display().to_string(), caps)
    } else {
        ("detecting…".to_string(), String::new())
    };

    frame.render_widget(
        Paragraph::new(vec![
            Line::from(vec![
                Span::styled("  environment  ", Style::default().fg(dim)),
                Span::styled(env_text, Style::default().fg(cyan).add_modifier(Modifier::BOLD)),
            ]),
            Line::from(vec![
                Span::styled("  capabilities ", Style::default().fg(dim)),
                Span::styled(cap_text, Style::default().fg(white)),
            ]),
        ]),
        chunks[0],
    );

    // ── Tools by category ─────────────────────────────────────────────────────
    let categories = [
        ToolCategory::Discovery, ToolCategory::Web, ToolCategory::Injection,
        ToolCategory::Credentials, ToolCategory::Vulnerability, ToolCategory::Exploitation,
        ToolCategory::Osint, ToolCategory::Enumeration, ToolCategory::Network, ToolCategory::Wifi,
    ];

    let mut tool_lines: Vec<Line<'static>> = vec![];
    for cat in &categories {
        let tools_in_cat: Vec<&crate::pentest::ToolDef> = ALL_TOOLS.iter()
            .filter(|t| &t.category == cat)
            .collect();
        if tools_in_cat.is_empty() { continue; }

        let mut items: Vec<Span<'static>> = vec![
            Span::styled(format!("  {:<12}", cat.display()), Style::default().fg(dim)),
        ];

        for def in &tools_in_cat {
            let status = app.pentest_inventory.get(def.name);
            let (sym, col) = match status {
                Some(ToolStatus::Available { .. })    => ("✓", green),
                Some(ToolStatus::Missing)             => ("✗", red),
                Some(ToolStatus::Skipped { .. })      => ("—", dim),
                Some(ToolStatus::InstallFailed { .. }) => ("!", yellow),
                Some(ToolStatus::Checking) | None     => ("·", dim),
            };
            items.push(Span::styled(
                format!("{} {:<14}", sym, def.name),
                Style::default().fg(col),
            ));
        }
        tool_lines.push(Line::from(items));
    }

    frame.render_widget(
        Paragraph::new(tool_lines),
        chunks[2],
    );

    // ── Workflow availability ──────────────────────────────────────────────────
    let mut wf_lines: Vec<Line<'static>> = vec![
        Line::from(vec![Span::styled("  WORKFLOWS", Style::default().fg(dim))]),
    ];

    for wf in WORKFLOWS {
        let avail = compute_availability(wf, &app.pentest_inventory);
        let (label_col, label) = match &avail {
            WorkflowAvailability::Ready        => (green,  "READY      "),
            WorkflowAvailability::Limited {..} => (yellow, "LIMITED    "),
            WorkflowAvailability::Unavailable {..} => (dim, "UNAVAILABLE"),
        };
        let name_col = if avail.is_runnable() { white } else { dim };
        wf_lines.push(Line::from(vec![
            Span::styled("  ", Style::default()),
            Span::styled(label, Style::default().fg(label_col).add_modifier(Modifier::BOLD)),
            Span::styled(format!("  {}", wf.name), Style::default().fg(name_col)),
        ]));
    }

    frame.render_widget(Paragraph::new(wf_lines), chunks[4]);

    // ── Footer ────────────────────────────────────────────────────────────────
    let has_missing = app.pentest_inv_complete
        && !app.pentest_inventory.missing_tools().is_empty();
    let footer_text = if !app.pentest_inv_complete {
        "  checking tools…"
    } else if has_missing {
        "  Enter — install missing tools  ·  Esc to cancel"
    } else {
        "  Enter — continue  ·  Esc to cancel"
    };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(footer_text, Style::default().fg(dim)),
        ])),
        chunks[5],
    );
}

// ── PenTest Engagement Setup ──────────────────────────────────────────────────

fn render_pentest_setup(frame: &mut Frame, area: Rect, app: &App) {
    let cyan   = ratatui::style::Color::Rgb(0,   212, 255);
    let white  = ratatui::style::Color::Rgb(220, 220, 240);
    let dim    = ratatui::style::Color::Rgb(80,  80,  110);
    let yellow = ratatui::style::Color::Rgb(241, 250, 140);
    let bg     = ratatui::style::Color::Rgb(6,   6,   16);

    let dialog = centered_rect(62, 16, area);
    frame.render_widget(Clear, dialog);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(cyan))
        .title(Line::from(vec![
            Span::styled(" NEW ENGAGEMENT ", Style::default().fg(cyan).add_modifier(Modifier::BOLD)),
        ]))
        .style(Style::default().bg(bg));
    let inner = block.inner(dialog);
    frame.render_widget(block, dialog);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2), // field 0: target
            Constraint::Length(1), // spacer
            Constraint::Length(2), // field 1: exclusions
            Constraint::Length(1), // spacer
            Constraint::Length(2), // field 2: depth
            Constraint::Length(1), // spacer
            Constraint::Length(2), // footer
        ])
        .split(inner);

    let field_style = |idx: usize| -> Style {
        if app.pentest_setup_field == idx {
            Style::default().fg(cyan).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(dim)
        }
    };
    let input_style = |idx: usize| -> Style {
        if app.pentest_setup_field == idx {
            Style::default().fg(white)
        } else {
            Style::default().fg(dim)
        }
    };
    let cursor = |idx: usize, s: &str| -> String {
        if app.pentest_setup_field == idx {
            format!("{}_", s)
        } else {
            s.to_string()
        }
    };

    // Field 0: Target
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(vec![Span::styled("  Target  ", field_style(0)),
                Span::styled("IP · CIDR · domain", Style::default().fg(dim))]),
            Line::from(vec![Span::styled("  > ", Style::default().fg(cyan)),
                Span::styled(cursor(0, &app.pentest_setup_target), input_style(0))]),
        ]),
        chunks[0],
    );

    // Field 1: Exclusions
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(vec![Span::styled("  Exclusions  ", field_style(1)),
                Span::styled("comma-separated (optional)", Style::default().fg(dim))]),
            Line::from(vec![Span::styled("  > ", Style::default().fg(cyan)),
                Span::styled(cursor(1, &app.pentest_setup_exclusions), input_style(1))]),
        ]),
        chunks[2],
    );

    // Field 2: Depth selector
    let depths = [
        crate::pentest::Depth::ReconOnly,
        crate::pentest::Depth::SafeActive,
        crate::pentest::Depth::Full,
    ];
    let mut depth_spans = vec![
        Span::styled("  Depth  ", field_style(2)),
        Span::styled("◄ ", if app.pentest_setup_field == 2 { Style::default().fg(cyan) } else { Style::default().fg(dim) }),
    ];
    for d in &depths {
        let active = *d == app.pentest_setup_depth;
        let col = if active { cyan } else { dim };
        let txt = if active { format!("[{}]", d.display()) } else { d.display().to_string() };
        depth_spans.push(Span::styled(format!(" {} ", txt), Style::default().fg(col)));
    }
    depth_spans.push(Span::styled(" ►", if app.pentest_setup_field == 2 { Style::default().fg(cyan) } else { Style::default().fg(dim) }));
    let depth_desc = app.pentest_setup_depth.description();
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(depth_spans),
            Line::from(vec![Span::styled(format!("  {}", depth_desc), Style::default().fg(dim))]),
        ]),
        chunks[4],
    );

    // Footer
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("  Tab/↓ next field  ·  Enter start engagement  ·  Esc back",
                Style::default().fg(dim)),
        ])),
        chunks[6],
    );
}

// ── PenTest Tool Selector ─────────────────────────────────────────────────────

fn render_pentest_selector(frame: &mut Frame, area: Rect, app: &App) {
    use crate::pentest::{tool_def, PackageManager};

    let cyan   = ratatui::style::Color::Rgb(0,   212, 255);
    let green  = ratatui::style::Color::Rgb(80,  250, 123);
    let yellow = ratatui::style::Color::Rgb(241, 250, 140);
    let red    = ratatui::style::Color::Rgb(255, 42,  109);
    let white  = ratatui::style::Color::Rgb(220, 220, 240);
    let dim    = ratatui::style::Color::Rgb(80,  80,  110);
    let bg     = ratatui::style::Color::Rgb(6,   6,   16);
    let mg     = ratatui::style::Color::Rgb(224, 64,  251);

    let selected_count = app.pentest_selector_items.iter().filter(|(_, s)| *s).count();

    let dialog_h = (app.pentest_selector_items.len() as u16 + 14).min(area.height.saturating_sub(2));
    let dialog = centered_rect(68, dialog_h, area);
    frame.render_widget(Clear, dialog);

    let border_col = if app.pentest_golang_confirm      { yellow }
        else if app.pentest_install_sudo_prompt        { mg }
        else                                           { cyan };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_col))
        .title(Line::from(vec![
            Span::styled(" INSTALL TOOLS ", Style::default().fg(border_col).add_modifier(Modifier::BOLD)),
        ]))
        .style(Style::default().bg(bg));
    let inner = block.inner(dialog);
    frame.render_widget(block, dialog);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),  // nav hint
            Constraint::Length(1),  // divider
            Constraint::Min(4),     // tool list
            Constraint::Length(1),  // divider
            Constraint::Length(3),  // footer / sudo prompt
        ])
        .split(inner);

    // Navigation hint
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("  ↑↓ navigate  ", Style::default().fg(dim)),
            Span::styled("Space", Style::default().fg(cyan)),
            Span::styled(" toggle  ", Style::default().fg(dim)),
            Span::styled("a", Style::default().fg(cyan)),
            Span::styled(" all  ", Style::default().fg(dim)),
            Span::styled("n", Style::default().fg(cyan)),
            Span::styled(" none  ", Style::default().fg(dim)),
            Span::styled("Enter", Style::default().fg(cyan)),
            Span::styled(" install", Style::default().fg(dim)),
        ])),
        chunks[0],
    );
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                "  ──────────────────────────────────────────────────────────",
                Style::default().fg(dim),
            )
        ])),
        chunks[1],
    );

    // Tool list — group by category
    let pkg_mgr_label = if let Some(env) = &app.pentest_env {
        match &env.package_manager {
            Some(PackageManager::Apt)    => "apt install",
            Some(PackageManager::Dnf)    => "dnf install",
            Some(PackageManager::Pacman) => "pacman -S",
            Some(PackageManager::Brew)   => "brew install",
            _ => "install",
        }
    } else { "install" };

    let mut lines: Vec<Line<'static>> = vec![];
    for (i, (name, selected)) in app.pentest_selector_items.iter().enumerate() {
        let is_cursor = i == app.pentest_selector_cursor;

        let checkbox = if *selected { "[✓]" } else { "[  ]" };
        let check_col = if *selected { green } else { dim };

        // Install method label
        let method = if let Some(def) = tool_def(name) {
            if def.special.is_some() {
                "go install".to_string()
            } else {
                let pkg = match &app.pentest_env {
                    Some(env) => match &env.package_manager {
                        Some(PackageManager::Apt)    => def.apt.unwrap_or(name),
                        Some(PackageManager::Dnf)    => def.dnf.unwrap_or(name),
                        Some(PackageManager::Pacman) => def.pacman.unwrap_or(name),
                        _ => def.apt.unwrap_or(name),
                    },
                    None => name,
                };
                format!("{} {}", pkg_mgr_label, pkg)
            }
        } else {
            name.clone()
        };

        let cursor_sym = if is_cursor { "►" } else { " " };
        let name_col   = if is_cursor { white } else if *selected { white } else { dim };
        let bg_style   = if is_cursor {
            Style::default().bg(ratatui::style::Color::Rgb(20, 20, 40))
        } else {
            Style::default()
        };

        lines.push(Line::from(vec![
            Span::styled(format!(" {} ", cursor_sym), Style::default().fg(cyan)),
            Span::styled(format!("{} ", checkbox), Style::default().fg(check_col)),
            Span::styled(format!("{:<16}", name), Style::default().fg(name_col).patch(bg_style)),
            Span::styled(method, Style::default().fg(dim)),
        ]));
    }

    let list_h = chunks[2].height as usize;
    let scroll = if app.pentest_selector_cursor + 1 > list_h {
        app.pentest_selector_cursor + 1 - list_h
    } else {
        0
    };
    let visible: Vec<Line<'static>> = lines.into_iter().skip(scroll).collect();
    frame.render_widget(Paragraph::new(visible), chunks[2]);

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                "  ──────────────────────────────────────────────────────────",
                Style::default().fg(dim),
            )
        ])),
        chunks[3],
    );

    // Footer — golang confirm, sudo prompt, or normal hint
    if app.pentest_golang_confirm {
        let tools_str = app.pentest_golang_tools.join(", ");
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(vec![
                    Span::styled("  golang-go required for: ", Style::default().fg(yellow).add_modifier(Modifier::BOLD)),
                    Span::styled(tools_str, Style::default().fg(white)),
                ]),
                Line::from(vec![
                    Span::styled("  Install golang-go (~200 MB)?  ", Style::default().fg(yellow)),
                    Span::styled("[Y]", Style::default().fg(green).add_modifier(Modifier::BOLD)),
                    Span::styled(" yes  ", Style::default().fg(dim)),
                    Span::styled("[N]", Style::default().fg(red)),
                    Span::styled(" skip those tools  ", Style::default().fg(dim)),
                    Span::styled("[Esc]", Style::default().fg(dim)),
                    Span::styled(" back", Style::default().fg(dim)),
                ]),
            ]),
            chunks[4],
        );
    } else if app.pentest_install_sudo_prompt {
        let dots = "●".repeat(app.pentest_install_sudo_input.len());
        let input_text = if app.pentest_install_sudo_input.is_empty() {
            "  enter sudo password…".to_string()
        } else {
            format!("  {}_", dots)
        };
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(vec![
                    Span::styled("  sudo password required  ", Style::default().fg(yellow).add_modifier(Modifier::BOLD)),
                ]),
                Line::from(vec![
                    Span::styled(input_text, Style::default().fg(mg).add_modifier(Modifier::BOLD)),
                ]),
            ]),
            chunks[4],
        );
    } else {
        let hint = if selected_count == 0 {
            format!("  nothing selected")
        } else {
            format!("  {} tool{} selected  ·  Enter to install  ·  Esc back",
                selected_count,
                if selected_count == 1 { "" } else { "s" })
        };
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(hint, Style::default().fg(if selected_count > 0 { cyan } else { dim })),
            ])),
            chunks[4],
        );
    }
}

// ── PenTest Install Progress ──────────────────────────────────────────────────

fn render_pentest_install(frame: &mut Frame, area: Rect, app: &App) {
    let green  = ratatui::style::Color::Rgb(57,  255, 20);
    let red    = ratatui::style::Color::Rgb(255, 42,  109);
    let cyan   = ratatui::style::Color::Rgb(0,   212, 255);
    let yellow = ratatui::style::Color::Rgb(241, 250, 140);
    let dim    = ratatui::style::Color::Rgb(80,  80,  110);
    let white  = ratatui::style::Color::Rgb(220, 220, 240);
    let bg     = ratatui::style::Color::Rgb(6,   6,   16);

    let dialog = centered_rect(72, area.height.min(36).max(14), area);
    frame.render_widget(Clear, dialog);

    let (title, border_col) = if app.pentest_installing {
        (" INSTALLING… ", cyan)
    } else if app.pentest_install_log.iter().any(|l| l.contains('✗')) {
        (" INSTALL COMPLETE — ERRORS ", red)
    } else {
        (" INSTALL COMPLETE ", green)
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_col))
        .title(Line::from(vec![
            Span::styled(title, Style::default().fg(border_col).add_modifier(Modifier::BOLD)),
        ]))
        .style(Style::default().bg(bg));
    let inner = block.inner(dialog);
    frame.render_widget(block, dialog);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);

    // Scroll to bottom — show last N lines
    let log_h = chunks[0].height as usize;
    let start = app.pentest_install_log.len().saturating_sub(log_h);
    let log_lines: Vec<Line<'static>> = app.pentest_install_log[start..].iter().map(|l| {
        let col = if l.starts_with('✓')      { green  }
            else if l.starts_with('✗')       { red    }
            else if l.starts_with('$')       { cyan   }
            else if l.starts_with('⚠')       { yellow }
            else                             { dim    };
        Line::from(vec![Span::styled(
            format!("  {}", l),
            Style::default().fg(col),
        )])
    }).collect();
    frame.render_widget(Paragraph::new(log_lines), chunks[0]);

    // Footer
    let footer = if app.pentest_installing {
        let spin = ["⠋","⠙","⠹","⠸","⠼","⠴","⠦","⠧","⠇","⠏"][
            (app.cursor_blink_tick as usize / 2) % 10
        ];
        Span::styled(format!("  {}  installing…", spin), Style::default().fg(cyan))
    } else {
        Span::styled("  Esc to close", Style::default().fg(dim))
    };
    frame.render_widget(
        Paragraph::new(Line::from(vec![footer]))
            .block(Block::default().borders(Borders::TOP).border_style(Style::default().fg(dim))),
        chunks[1],
    );
}

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
