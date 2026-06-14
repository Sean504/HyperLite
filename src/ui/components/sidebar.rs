/// Right sidebar: sessions (compact) · active plan / tool activity · folder · model · hardware

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};
use crate::app::App;

pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .borders(Borders::LEFT)
        .border_style(Style::default().fg(app.theme.border))
        .style(Style::default().bg(app.theme.bg_panel));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    // ── Heights ────────────────────────────────────────────────────────────────
    // Mascot: pixel robot at the top — only when the terminal is tall enough
    let mascot_h   = if inner.height >= 34 { super::mascot::PANEL_HEIGHT } else { 0 };
    // Sessions: fixed compact block (~¼ of typical terminal height, min 4)
    let session_h  = (inner.height / 4).max(4).min(12);
    // Bottom fixed sections
    let folder_h   = 6_u16;
    let model_h    = 3_u16;
    let hardware_h = 5_u16;
    let bottom_h   = folder_h + model_h + hardware_h;
    // Plan panel gets everything in between
    let plan_h = inner.height
        .saturating_sub(mascot_h)
        .saturating_sub(session_h)
        .saturating_sub(bottom_h);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(mascot_h),
            Constraint::Length(session_h),
            Constraint::Length(plan_h.max(4)),
            Constraint::Length(folder_h),
            Constraint::Length(model_h),
            Constraint::Length(hardware_h),
        ])
        .split(inner);

    if mascot_h > 0 {
        super::mascot::render(frame, chunks[0], app);
    }
    render_sessions(frame, chunks[1], app);
    render_plan_panel(frame, chunks[2], app);
    render_folder(frame, chunks[3], app);
    render_model_info(frame, chunks[4], app);
    render_hardware(frame, chunks[5], app);
}

// ── Sessions (compact) ────────────────────────────────────────────────────────

fn render_sessions(frame: &mut Frame, area: Rect, app: &App) {
    let title = Line::from(vec![
        Span::styled(" Sessions ", Style::default().fg(app.theme.primary).add_modifier(Modifier::BOLD)),
    ]);

    let items: Vec<ListItem> = app.sessions.iter().map(|s| {
        let is_active = s.id == app.session_id;
        let marker = if is_active { "▶ " } else { "  " };
        let style = if is_active {
            Style::default().fg(app.theme.accent).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(app.theme.text_muted)
        };
        let label = format!("{}{}", marker, truncate(&s.title, (area.width as usize).saturating_sub(3)));
        ListItem::new(label).style(style)
    }).collect();

    let block = Block::default()
        .title(title)
        .borders(Borders::BOTTOM)
        .border_style(Style::default().fg(app.theme.border));

    let list = List::new(items).block(block);
    frame.render_widget(list, area);
}

// ── Active Plan / Tool Activity ───────────────────────────────────────────────

fn render_plan_panel(frame: &mut Frame, area: Rect, app: &App) {
    if area.height < 3 { return; }

    // Changes cockpit takes priority: a pending changeset, or this session's
    // applied/skipped history, is the most useful thing to surface here.
    if app.changeset.is_some() || !app.changes_log.is_empty() {
        render_changes(frame, area, app);
        return;
    }

    let has_plan = !app.active_plan.is_empty();

    let title = if has_plan {
        let done  = app.plan_step.min(app.active_plan.len());
        let total = app.active_plan.len();
        Line::from(vec![
            Span::styled(" Plan ", Style::default().fg(app.theme.primary).add_modifier(Modifier::BOLD)),
            Span::styled(
                format!(" {}/{} ", done, total),
                Style::default().fg(app.theme.text_muted),
            ),
        ])
    } else {
        Line::from(vec![
            Span::styled(" Activity ", Style::default().fg(app.theme.secondary).add_modifier(Modifier::BOLD)),
        ])
    };

    let block = Block::default()
        .title(title)
        .borders(Borders::BOTTOM)
        .border_style(Style::default().fg(app.theme.border));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if has_plan {
        render_plan_steps(frame, inner, app);
    } else {
        render_tool_history(frame, inner, app);
    }
}

// ── Changes cockpit ───────────────────────────────────────────────────────────

fn render_changes(frame: &mut Frame, area: Rect, app: &App) {
    use crate::app::ChangeStatus;

    // Title with running totals
    let (added, removed): (usize, usize) = {
        let mut a = app.changes_log.iter().map(|c| c.added).sum::<usize>();
        let mut r = app.changes_log.iter().map(|c| c.removed).sum::<usize>();
        if let Some(cs) = &app.changeset { a += cs.total_added(); r += cs.total_removed(); }
        (a, r)
    };
    let title = Line::from(vec![
        Span::styled(" Changes ", Style::default().fg(app.theme.accent).add_modifier(Modifier::BOLD)),
        Span::styled(format!("+{} ", added), Style::default().fg(app.theme.diff_add_fg)),
        Span::styled(format!("−{} ", removed), Style::default().fg(app.theme.diff_del_fg)),
    ]);
    let block = Block::default()
        .title(title)
        .borders(Borders::BOTTOM)
        .border_style(Style::default().fg(app.theme.border));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let row = |path: &str, added: usize, removed: usize, marker: &str, mstyle: Style, width: u16| -> Line<'static> {
        let name = std::path::Path::new(path).file_name()
            .and_then(|n| n.to_str()).unwrap_or(path);
        let counts = format!(" +{} −{}", added, removed);
        let name_w = (width as usize).saturating_sub(2 + counts.chars().count());
        Line::from(vec![
            Span::styled(marker.to_string(), mstyle),
            Span::styled(format!("{:<width$}", truncate(name, name_w), width = name_w.max(1)),
                Style::default().fg(app.theme.text_muted)),
            Span::styled(format!("+{}", added), Style::default().fg(app.theme.diff_add_fg)),
            Span::styled(format!(" −{}", removed), Style::default().fg(app.theme.diff_del_fg)),
        ])
    };

    let mut lines: Vec<Line<'static>> = Vec::new();
    let max_rows = inner.height as usize;

    // Pending changeset entries first (most actionable)
    if let Some(cs) = &app.changeset {
        for e in &cs.entries {
            if lines.len() >= max_rows { break; }
            let (marker, mstyle) = match e.status {
                ChangeStatus::Pending => ("⧖ ", Style::default().fg(app.theme.warning)),
                ChangeStatus::Applied => ("✓ ", Style::default().fg(app.theme.success)),
                ChangeStatus::Skipped => ("✗ ", Style::default().fg(app.theme.text_dim)),
            };
            lines.push(row(&e.file_path, e.added, e.removed, marker, mstyle, inner.width));
        }
    }

    // Then the session history, most recent first
    for c in app.changes_log.iter().rev() {
        if lines.len() >= max_rows { break; }
        let (marker, mstyle) = match c.status {
            ChangeStatus::Applied => ("✓ ", Style::default().fg(app.theme.success)),
            ChangeStatus::Skipped => ("✗ ", Style::default().fg(app.theme.text_dim)),
            ChangeStatus::Pending => ("⧖ ", Style::default().fg(app.theme.warning)),
        };
        lines.push(row(&c.file_path, c.added, c.removed, marker, mstyle, inner.width));
    }

    if app.changeset.is_some() && !app.review_open {
        if lines.len() < max_rows {
            lines.push(Line::from(vec![
                Span::styled(" Ctrl+R to review", Style::default().fg(app.theme.accent)),
            ]));
        }
    }

    frame.render_widget(Paragraph::new(lines), inner);
}

fn render_plan_steps(frame: &mut Frame, area: Rect, app: &App) {
    let max_rows = area.height as usize;
    let total    = app.active_plan.len();
    let current  = app.plan_step;          // index of step currently executing (0-based)

    // Window the visible steps around the current one
    let visible_start = if current >= max_rows { current + 1 - max_rows } else { 0 };

    let mut lines: Vec<Line<'static>> = Vec::new();

    for (i, step) in app.active_plan.iter().enumerate() {
        if i < visible_start { continue; }
        if lines.len() >= max_rows { break; }

        let is_done    = i < current;
        let is_current = i == current && app.streaming;
        let label      = truncate(step, (area.width as usize).saturating_sub(4));

        let (marker, marker_style, text_style) = if is_done {
            (
                "✓ ",
                Style::default().fg(app.theme.success),
                Style::default().fg(app.theme.text_muted),
            )
        } else if is_current {
            (
                "▶ ",
                Style::default().fg(app.theme.accent).add_modifier(Modifier::BOLD),
                Style::default().fg(app.theme.text).add_modifier(Modifier::BOLD),
            )
        } else {
            (
                "○ ",
                Style::default().fg(app.theme.text_dim),
                Style::default().fg(app.theme.text_dim),
            )
        };

        // Dim the step index for context
        let step_num = format!("{:>2}.", i + 1);
        lines.push(Line::from(vec![
            Span::styled(step_num, Style::default().fg(app.theme.text_dim)),
            Span::styled(marker, marker_style),
            Span::styled(label, text_style),
        ]));

        // If this is the last visible step and more remain, add a hint
        if lines.len() == max_rows && i + 1 < total {
            let remaining = total - i - 1;
            lines.pop();
            lines.push(Line::from(vec![
                Span::styled(
                    format!("   … {} more step{}", remaining, if remaining == 1 { "" } else { "s" }),
                    Style::default().fg(app.theme.text_dim),
                ),
            ]));
        }
    }

    frame.render_widget(Paragraph::new(lines), area);
}

fn render_tool_history(frame: &mut Frame, area: Rect, app: &App) {
    if app.tool_history.is_empty() {
        let hint = Line::from(vec![
            Span::styled(
                " No tools used yet",
                Style::default().fg(app.theme.text_dim),
            ),
        ]);
        frame.render_widget(Paragraph::new(vec![hint]), area);
        return;
    }

    let max_rows = area.height as usize;
    // Show most recent at top
    let history: Vec<&(String, bool)> = app.tool_history.iter().rev().take(max_rows).collect();

    let lines: Vec<Line<'static>> = history.iter().map(|(name, is_err)| {
        let (marker, style) = if *is_err {
            ("✗ ", Style::default().fg(app.theme.error))
        } else {
            ("✓ ", Style::default().fg(app.theme.success))
        };
        let name_s = truncate(name, (area.width as usize).saturating_sub(3));
        Line::from(vec![
            Span::styled(marker, style),
            Span::styled(name_s, Style::default().fg(app.theme.text_muted)),
        ])
    }).collect();

    frame.render_widget(Paragraph::new(lines), area);
}

// ── Folder ────────────────────────────────────────────────────────────────────

fn render_folder(frame: &mut Frame, area: Rect, app: &App) {
    let title = Line::from(vec![
        Span::styled(" Folder ", Style::default().fg(app.theme.secondary).add_modifier(Modifier::BOLD)),
    ]);

    let block = Block::default()
        .title(title)
        .borders(Borders::BOTTOM)
        .border_style(Style::default().fg(app.theme.border));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let folder_name = app.working_dir
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| app.working_dir.display().to_string());

    // Check if this folder has a RAG index
    let has_index = {
        let dir = app.working_dir.to_string_lossy().to_string();
        let conn = app.db.lock().unwrap();
        crate::rag::store::get_index_for_dir(&conn, &dir)
            .ok().flatten().is_some()
    };

    let index_line = if has_index {
        Line::from(vec![Span::styled("◆ indexed", Style::default().fg(app.theme.success))])
    } else if app.project_context_active {
        Line::from(vec![Span::styled("◆ git context", Style::default().fg(app.theme.accent))])
    } else {
        Line::default()
    };

    let mem_count = {
        let conn = app.db.lock().unwrap();
        crate::memory::count(&conn)
    };
    let memory_line = if mem_count > 0 {
        Line::from(vec![Span::styled(
            format!("◆ {} memor{}", mem_count, if mem_count == 1 { "y" } else { "ies" }),
            Style::default().fg(app.theme.primary),
        )])
    } else {
        Line::default()
    };

    let sandbox_line = if app.sandbox_enabled {
        Line::from(vec![Span::styled("⬡ sandbox", Style::default().fg(app.theme.yellow))])
    } else {
        Line::default()
    };

    let lines = vec![
        Line::from(vec![
            Span::styled(
                truncate(&folder_name, inner.width as usize),
                Style::default().fg(app.theme.accent),
            ),
        ]),
        index_line,
        memory_line,
        sandbox_line,
    ];
    frame.render_widget(Paragraph::new(lines), inner);
}

// ── Model Info ────────────────────────────────────────────────────────────────

fn render_model_info(frame: &mut Frame, area: Rect, app: &App) {
    let title = Line::from(vec![
        Span::styled(" Model ", Style::default().fg(app.theme.secondary).add_modifier(Modifier::BOLD)),
    ]);

    let block = Block::default()
        .title(title)
        .borders(Borders::BOTTOM)
        .border_style(Style::default().fg(app.theme.border));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let model_name = app.current_model_name();
    let backend    = app.current_backend_name();
    let combined   = format!("{} ({})", model_name, backend);

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                truncate(&combined, inner.width as usize),
                Style::default().fg(app.theme.primary).add_modifier(Modifier::BOLD),
            ),
        ])),
        inner,
    );
}

// ── Hardware ──────────────────────────────────────────────────────────────────

fn render_hardware(frame: &mut Frame, area: Rect, app: &App) {
    let title = Line::from(vec![
        Span::styled(" Hardware ", Style::default().fg(app.theme.secondary).add_modifier(Modifier::BOLD)),
    ]);

    let block = Block::default()
        .title(title)
        .borders(Borders::NONE);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let hw = &app.hardware;
    let mut lines: Vec<Line<'static>> = vec![];

    // Live CPU gauge
    let cpu_pct = app.live_cpu_pct.clamp(0.0, 100.0);
    lines.push(gauge_line(" CPU ", (cpu_pct / 100.0) as f64, format!("{:>3.0}%", cpu_pct), app, inner.width));

    // Live RAM gauge
    let ram_total = hw.memory.total_mb.max(1);
    let ram_used  = app.live_ram_used_mb.min(ram_total);
    let ram_label = format!("{}/{}G", ram_used / 1024, ram_total / 1024);
    lines.push(gauge_line(" RAM ", ram_used as f64 / ram_total as f64, ram_label, app, inner.width));

    for gpu in &hw.gpus {
        lines.push(Line::from(vec![
            Span::styled(" GPU ", Style::default().fg(app.theme.text_muted)),
            Span::styled(
                truncate(&format!("{} · {}G", gpu.name, gpu.vram_total_mb / 1024), (inner.width as usize).saturating_sub(6)),
                Style::default().fg(app.theme.accent),
            ),
        ]));
    }

    lines.push(Line::from(vec![
        Span::styled(format!(" ⚡ {}", hw.recommendation_short()), Style::default().fg(app.theme.success)),
    ]));

    frame.render_widget(Paragraph::new(lines), inner);
}

/// One labelled block-char gauge row: ` CPU ▰▰▰▱▱▱▱▱ 34%`
fn gauge_line(label: &'static str, ratio: f64, value: String, app: &App, width: u16) -> Line<'static> {
    let ratio = ratio.clamp(0.0, 1.0);
    // label(5) + space + value + space → remainder is the bar
    let bar_w = (width as usize)
        .saturating_sub(label.len() + value.chars().count() + 2)
        .clamp(4, 14);
    let filled = (ratio * bar_w as f64).round() as usize;

    let fill_color = if ratio >= 0.85 {
        app.theme.error
    } else if ratio >= 0.6 {
        app.theme.warning
    } else {
        app.theme.accent
    };

    Line::from(vec![
        Span::styled(label, Style::default().fg(app.theme.text_muted)),
        Span::styled("▰".repeat(filled), Style::default().fg(fill_color)),
        Span::styled("▱".repeat(bar_w - filled), Style::default().fg(app.theme.text_dim)),
        Span::styled(format!(" {}", value), Style::default().fg(app.theme.text)),
    ])
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn truncate(s: &str, max: usize) -> String {
    if max == 0 { return String::new(); }
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let cut: String = s.chars().take(max.saturating_sub(1)).collect();
        format!("{}…", cut)
    }
}
