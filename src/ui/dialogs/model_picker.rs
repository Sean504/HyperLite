/// Model picker dialog: favorites / recents / all backends, hardware-aware.

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Gauge, List, ListItem, ListState, Paragraph, Tabs};
use crate::app::App;
use crate::models::codex;
use super::centered_rect;

const TABS: &[&str] = &["All", "Recommended", "By Backend", "Download"];

pub struct DownloadEntry {
    pub name:    &'static str,
    pub display: &'static str,
    pub desc:    &'static str,
    pub size_gb: f32,
}

pub const DOWNLOADABLE: &[DownloadEntry] = &[
    DownloadEntry { name: "smollm2:1.7b",       display: "SmolLM2 1.7B",        desc: "Tiny but capable. Runs anywhere.",                  size_gb: 1.0  },
    DownloadEntry { name: "phi4-mini",           display: "Phi-4 Mini 3.8B",      desc: "Microsoft's punchy small model. Great reasoning.",   size_gb: 2.5  },
    DownloadEntry { name: "qwen2.5:3b",          display: "Qwen2.5 3B",           desc: "Alibaba multilingual workhorse.",                    size_gb: 2.0  },
    DownloadEntry { name: "llama3.2:3b",         display: "Llama 3.2 3B",         desc: "Meta's small Llama. Solid all-rounder.",             size_gb: 2.0  },
    DownloadEntry { name: "qwen2.5-coder:7b",    display: "Qwen2.5-Coder 7B",     desc: "Best-in-class code model at 7B.",                   size_gb: 4.7  },
    DownloadEntry { name: "mistral:7b",          display: "Mistral 7B",           desc: "Fast and strong general-purpose.",                   size_gb: 4.1  },
    DownloadEntry { name: "llama3.1:8b",         display: "Llama 3.1 8B",         desc: "Meta's flagship 8B. Excellent instruction following.", size_gb: 4.7 },
    DownloadEntry { name: "qwen2.5:14b",         display: "Qwen2.5 14B",          desc: "Great balance of capability and size.",              size_gb: 9.0  },
    DownloadEntry { name: "deepseek-r1:14b",     display: "DeepSeek-R1 14B",      desc: "Chain-of-thought reasoning model.",                  size_gb: 9.0  },
    DownloadEntry { name: "qwen2.5-coder:14b",   display: "Qwen2.5-Coder 14B",    desc: "Top coding model for mid-range GPUs.",               size_gb: 9.0  },
    DownloadEntry { name: "qwen2.5:32b",         display: "Qwen2.5 32B",          desc: "Frontier-level quality for high-end GPUs.",          size_gb: 20.0 },
    DownloadEntry { name: "deepseek-r1:32b",     display: "DeepSeek-R1 32B",      desc: "Best open reasoning model.",                         size_gb: 20.0 },
    DownloadEntry { name: "llama3.3:70b",        display: "Llama 3.3 70B",        desc: "Near-frontier. Needs 40 GB+ VRAM or large RAM.",     size_gb: 43.0 },
];

pub fn render(frame: &mut Frame, area: Rect, app: &mut App) {
    let dialog = centered_rect(70, 28, area);
    frame.render_widget(Clear, dialog);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(app.theme.accent))
        .title(Line::from(vec![
            Span::styled(" Model Picker ", Style::default().fg(app.theme.accent).add_modifier(Modifier::BOLD)),
        ]))
        .style(Style::default().bg(app.theme.bg_panel));

    let inner = block.inner(dialog);
    frame.render_widget(block, dialog);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),   // tabs
            Constraint::Length(3),   // search
            Constraint::Min(1),      // list
            Constraint::Length(3),   // model details
            Constraint::Length(1),   // hints
        ])
        .split(inner);

    // Tabs
    let tab_titles: Vec<Line> = TABS.iter().map(|t| Line::from(*t)).collect();
    let tabs = Tabs::new(tab_titles)
        .select(app.model_picker_tab)
        .style(Style::default().fg(app.theme.text_muted))
        .highlight_style(Style::default().fg(app.theme.primary).add_modifier(Modifier::BOLD))
        .divider("│");
    frame.render_widget(tabs, chunks[0]);

    if app.model_picker_tab == 3 {
        render_download_tab(frame, &chunks, app);
        return;
    }

    // Search
    let search_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(app.theme.border_hi))
        .style(Style::default().bg(app.theme.bg_element));
    let search_inner = search_block.inner(chunks[1]);
    frame.render_widget(search_block, chunks[1]);

    let query = &app.dialog_search_query;
    let search_line = Line::from(vec![
        Span::styled(" 🔍 ", Style::default().fg(app.theme.accent)),
        Span::styled(query.clone(), Style::default().fg(app.theme.text)),
        Span::styled("█", Style::default().fg(app.theme.accent)),
    ]);
    frame.render_widget(Paragraph::new(search_line), search_inner);

    // Model list
    let models = filter_models(app);
    let items: Vec<ListItem> = models.iter().map(|m| {
        let is_active = &m.id == &app.current_model;
        let marker    = if is_active { "▶ " } else { "  " };
        let format_tag = format!("[{}]", format!("{:?}", m.format).to_uppercase());
        let style = if is_active {
            Style::default().fg(app.theme.accent).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(app.theme.text)
        };
        let label = format!("{}{:<35} {:>12}", marker, truncate(&m.name, 35), format_tag);
        ListItem::new(label).style(style)
    }).collect();

    let mut list_state = ListState::default();
    list_state.select(Some(app.dialog_selected_idx.min(models.len().saturating_sub(1))));

    let list = List::new(items)
        .highlight_style(Style::default().fg(app.theme.bg).bg(app.theme.accent))
        .highlight_symbol("► ");
    frame.render_stateful_widget(list, chunks[2], &mut list_state);

    // Details panel for selected model
    if let Some(m) = models.get(app.dialog_selected_idx) {
        render_model_details(frame, chunks[3], app, &m.name);
    }

    // Hints
    let hint = Paragraph::new(Line::from(vec![
        Span::styled(" ↑↓ navigate  Tab switch tab  Enter select  Esc close", Style::default().fg(app.theme.text_dim)),
    ]));
    frame.render_widget(hint, chunks[4]);
}

fn render_download_tab(frame: &mut Frame, chunks: &[Rect], app: &mut App) {
    let teal   = ratatui::style::Color::Rgb(0,   245, 212);
    let green  = ratatui::style::Color::Rgb(80,  250, 123);
    let orange = ratatui::style::Color::Rgb(255, 184, 108);
    let muted  = ratatui::style::Color::Rgb(90,  90, 138);

    // chunks[1]: download progress or idle hint
    if let Some(ref model_name) = app.model_dl_active.clone() {
        let ratio = if app.model_dl_bytes_total > 0 {
            app.model_dl_bytes_done as f64 / app.model_dl_bytes_total as f64
        } else { 0.0 };
        let pct   = (ratio * 100.0) as u64;
        let done  = app.model_dl_bytes_done;
        let total = app.model_dl_bytes_total;
        let speed = app.model_dl_speed_bps;

        let info = if speed > 0.0 {
            let eta_s = if speed > 0.0 && total > done {
                ((total - done) as f64 / speed) as u64
            } else { 0 };
            let eta = if eta_s >= 60 {
                format!("{}m {}s", eta_s / 60, eta_s % 60)
            } else {
                format!("{}s", eta_s)
            };
            format!(" {}  {}/{} MB  {:.1} MB/s  ETA {}",
                model_name,
                done / 1_048_576,
                total / 1_048_576,
                speed / 1_048_576.0,
                eta)
        } else {
            format!(" {}  connecting…", model_name)
        };

        let gauge = Gauge::default()
            .block(Block::default().borders(Borders::ALL)
                .border_style(Style::default().fg(teal))
                .title(Line::from(vec![Span::styled(" Downloading ", Style::default().fg(teal))])))
            .gauge_style(Style::default().fg(teal).bg(muted))
            .ratio(ratio.clamp(0.0, 1.0))
            .label(format!("{}%  {}", pct, info));
        frame.render_widget(gauge, chunks[1]);
    } else {
        let hint_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(app.theme.border))
            .style(Style::default().bg(app.theme.bg_element));
        let hint_inner = hint_block.inner(chunks[1]);
        frame.render_widget(hint_block, chunks[1]);
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(" Select a model and press Enter to download via Ollama", Style::default().fg(muted)),
            ])),
            hint_inner,
        );
    }

    // chunks[2]: list of downloadable models
    let q = app.dialog_search_query.to_lowercase();
    let entries: Vec<&DownloadEntry> = DOWNLOADABLE.iter()
        .filter(|e| q.is_empty()
            || e.display.to_lowercase().contains(&q)
            || e.name.contains(q.as_str()))
        .collect();

    let items: Vec<ListItem> = entries.iter().map(|e| {
        let installed = app.available_models.iter()
            .any(|m| m.name.starts_with(e.name.split(':').next().unwrap_or(e.name)));
        let downloading = app.model_dl_active.as_deref() == Some(e.name);
        let (marker, color) = if downloading {
            ("⬇ ", teal)
        } else if installed {
            ("✓ ", green)
        } else {
            ("  ", app.theme.text)
        };
        let label = format!("{}{:<28} {:>5} GB  {}", marker, e.display, e.size_gb, e.desc);
        ListItem::new(truncate(&label, 72)).style(Style::default().fg(color))
    }).collect();

    let mut list_state = ListState::default();
    list_state.select(Some(app.dialog_selected_idx.min(entries.len().saturating_sub(1))));

    let list = List::new(items)
        .highlight_style(Style::default().fg(app.theme.bg).bg(app.theme.accent))
        .highlight_symbol("► ");
    frame.render_stateful_widget(list, chunks[2], &mut list_state);

    // chunks[3]: description of selected model
    if let Some(e) = entries.get(app.dialog_selected_idx) {
        let block = Block::default()
            .borders(Borders::TOP)
            .border_style(Style::default().fg(app.theme.border))
            .style(Style::default().bg(app.theme.bg_panel));
        let inner = block.inner(chunks[3]);
        frame.render_widget(block, chunks[3]);
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(e.desc, Style::default().fg(app.theme.text_muted)),
            ])),
            inner,
        );
    }

    // chunks[4]: hints
    let hint_text = if app.model_dl_active.is_some() {
        " Downloading…  Tab switch tab  Esc close"
    } else {
        " ↑↓ navigate  Enter download  Tab switch tab  Esc close"
    };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(hint_text, Style::default().fg(app.theme.text_dim)),
        ])),
        chunks[4],
    );
}

fn filter_models(app: &App) -> Vec<crate::providers::LocalModel> {
    let query = app.dialog_search_query.to_lowercase();
    let mut models = app.available_models.clone();

    match app.model_picker_tab {
        1 => {
            // Recommended: filter by hardware tier
            let tier = app.hardware.max_model_tier();
            models.retain(|m| {
                if let Some(family) = codex::identify(&m.name) {
                    // vram_q4 is &[(param_count_m, vram_mb)] — find smallest entry
                    let min_vram = family.vram_q4.iter().map(|&(_, v)| v).min().unwrap_or(0);
                    let available = app.hardware.gpus.iter().map(|g| g.vram_total_mb as u64).max().unwrap_or(0);
                    available >= min_vram as u64
                } else {
                    true
                }
            });
        }
        2 => {
            // Group by backend — already sorted below
        }
        _ => {} // All
    }

    if !query.is_empty() {
        models.retain(|m| m.name.to_lowercase().contains(&query) || format!("{:?}", m.backend).to_lowercase().contains(&query));
    }

    models
}

fn render_model_details(frame: &mut Frame, area: Rect, app: &App, name: &str) {
    let block = Block::default()
        .borders(Borders::TOP)
        .border_style(Style::default().fg(app.theme.border))
        .style(Style::default().bg(app.theme.bg_panel));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if let Some(family) = codex::identify(name) {
        let caps: String = family.capabilities.iter().map(|c| format!("{} {} ", c.icon(), c.label())).collect();
        let lines = vec![
            Line::from(vec![Span::styled(caps, Style::default().fg(app.theme.accent))]),
            Line::from(vec![Span::styled(family.description, Style::default().fg(app.theme.text_muted))]),
        ];
        frame.render_widget(Paragraph::new(lines), inner);
    } else {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(name, Style::default().fg(app.theme.text_muted)))),
            inner,
        );
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max { return s.to_string(); }
    let cut: String = s.chars().take(max.saturating_sub(1)).collect();
    format!("{}…", cut)
}
