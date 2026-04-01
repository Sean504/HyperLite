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
    // Sessions: fixed compact block (~¼ of typical terminal height, min 4)
    let session_h  = (inner.height / 4).max(4).min(12);
    // Bottom fixed sections
    let folder_h   = 3_u16;
    let model_h    = 5_u16;
    let hardware_h = 5_u16;
    let bottom_h   = folder_h + model_h + hardware_h;
    // Plan panel gets everything in between
    let plan_h = inner.height
        .saturating_sub(session_h)
        .saturating_sub(bottom_h);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(session_h),
            Constraint::Length(plan_h.max(4)),
            Constraint::Length(folder_h),
            Constraint::Length(model_h),
            Constraint::Length(hardware_h),
        ])
        .split(inner);

    render_sessions(frame, chunks[0], app);
    render_plan_panel(frame, chunks[1], app);
    render_folder(frame, chunks[2], app);
    render_model_info(frame, chunks[3], app);
    render_hardware(frame, chunks[4], app);
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

    let line = Line::from(vec![
        Span::styled(
            truncate(&folder_name, inner.width as usize),
            Style::default().fg(app.theme.accent),
        ),
    ]);
    frame.render_widget(Paragraph::new(line), inner);
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

    let mut lines: Vec<Line<'static>> = vec![];
    let model_name = app.current_model_name();

    lines.push(Line::from(vec![
        Span::styled(
            truncate(&model_name, inner.width as usize),
            Style::default().fg(app.theme.primary).add_modifier(Modifier::BOLD),
        ),
    ]));

    if let Some(family) = crate::models::codex::identify(&model_name) {
        let caps: String = family.capabilities.iter().map(|c| format!("{} ", c.icon())).collect();
        lines.push(Line::from(vec![
            Span::styled(caps, Style::default().fg(app.theme.accent)),
        ]));
        lines.push(Line::from(vec![
            Span::styled(
                truncate(family.description, inner.width as usize),
                Style::default().fg(app.theme.text_muted),
            ),
        ]));
        let ctx_k = family.context_tokens / 1000;
        lines.push(Line::from(vec![
            Span::styled(format!("ctx {}k", ctx_k), Style::default().fg(app.theme.text_dim)),
        ]));
    }

    let backend = app.current_backend_name();
    lines.push(Line::from(vec![
        Span::styled(format!("via {}", backend), Style::default().fg(app.theme.text_dim)),
    ]));

    // Active agent indicator
    let agent_label = crate::tools::get_builtin_agent(&app.current_agent)
        .map(|a| a.name.to_string())
        .or_else(|| app.custom_agents.iter().find(|a| a.id == app.current_agent).map(|a| a.name.clone()))
        .unwrap_or_else(|| app.current_agent.clone());
    let agent_icon = match app.current_agent.as_str() {
        "plan"  => "◎",
        "build" => "⚒",
        _       => "◈",
    };
    lines.push(Line::from(vec![
        Span::styled(
            format!("{} {}", agent_icon, agent_label),
            Style::default().fg(app.theme.accent),
        ),
    ]));

    frame.render_widget(Paragraph::new(lines), inner);
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

    lines.push(Line::from(vec![
        Span::styled(" CPU ", Style::default().fg(app.theme.text_muted)),
        Span::styled(
            truncate(&hw.cpu.name, (inner.width as usize).saturating_sub(6)),
            Style::default().fg(app.theme.text),
        ),
    ]));

    let ram_gb = hw.memory.total_mb / 1024;
    let unified = if hw.memory.is_unified { " unified" } else { "" };
    lines.push(Line::from(vec![
        Span::styled(" RAM ", Style::default().fg(app.theme.text_muted)),
        Span::styled(format!("{} GB{}", ram_gb, unified), Style::default().fg(app.theme.text)),
    ]));

    for gpu in &hw.gpus {
        lines.push(Line::from(vec![
            Span::styled(" GPU ", Style::default().fg(app.theme.text_muted)),
            Span::styled(
                truncate(&format!("{} ({}MB)", gpu.name, gpu.vram_total_mb), (inner.width as usize).saturating_sub(6)),
                Style::default().fg(app.theme.accent),
            ),
        ]));
    }

    let rec = hw.recommendation_line();
    lines.push(Line::from(vec![
        Span::styled(format!(" ⚡ {}", rec), Style::default().fg(app.theme.success)),
    ]));

    frame.render_widget(Paragraph::new(lines), inner);
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
