/// 42-col right panel: session list + hardware info + model info.

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

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(8),
            Constraint::Length(3),
            Constraint::Length(7),
            Constraint::Length(8),
        ])
        .split(inner);

    render_sessions(frame, chunks[0], app);
    render_folder(frame, chunks[1], app);
    render_hardware(frame, chunks[2], app);
    render_model_info(frame, chunks[3], app);
}

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
    frame.render_widget(ratatui::widgets::Paragraph::new(line), inner);
}

fn render_hardware(frame: &mut Frame, area: Rect, app: &App) {
    let title = Line::from(vec![
        Span::styled(" Hardware ", Style::default().fg(app.theme.secondary).add_modifier(Modifier::BOLD)),
    ]);

    let block = Block::default()
        .title(title)
        .borders(Borders::BOTTOM)
        .border_style(Style::default().fg(app.theme.border));
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

fn render_model_info(frame: &mut Frame, area: Rect, app: &App) {
    let title = Line::from(vec![
        Span::styled(" Model ", Style::default().fg(app.theme.secondary).add_modifier(Modifier::BOLD)),
    ]);

    let block = Block::default()
        .title(title)
        .borders(Borders::NONE);
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

    frame.render_widget(Paragraph::new(lines), inner);
}

fn truncate(s: &str, max: usize) -> String {
    if max == 0 { return String::new(); }
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let cut: String = s.chars().take(max.saturating_sub(1)).collect();
        format!("{}…", cut)
    }
}
