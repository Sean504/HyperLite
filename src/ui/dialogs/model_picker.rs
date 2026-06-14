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

/// Hardware tier a downloadable model targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    /// Single-board computers / CPU / ≤8 GB RAM (Raspberry Pi, mini-PCs, laptops).
    Sbc,
    /// Mid-range GPUs with 8–16 GB VRAM (RTX 3060 / 4060 / 4070 / 4080).
    Mid,
    /// High-end consumer GPUs with 24–32 GB VRAM (RTX 3090 / 4090 / 5080 / 5090).
    High,
}

impl Tier {
    pub fn tag(&self) -> &'static str {
        match self {
            Tier::Sbc  => "SBC",
            Tier::Mid  => "MID",
            Tier::High => "HIGH",
        }
    }
}

pub struct DownloadEntry {
    pub display: &'static str,
    pub desc:    &'static str,
    pub tier:    Tier,
    pub size_gb: f32,
    pub hf_repo: &'static str,
    pub hf_file: &'static str,   // filename saved to ~/.hyperlite/models/
}

impl DownloadEntry {
    pub fn hf_url(&self) -> String {
        format!("https://huggingface.co/{}/resolve/main/{}", self.hf_repo, self.hf_file)
    }

    /// Quantization label parsed from the GGUF filename (e.g. "Q4_K_M", "Q8_0", "MXFP4").
    pub fn quant(&self) -> &'static str {
        let f = self.hf_file.to_uppercase();
        // Ordered most-specific first so e.g. Q4_K_M wins over a bare Q4.
        const QUANTS: &[&str] = &[
            "Q8_0", "Q6_K", "Q5_K_M", "Q5_0", "Q4_K_M", "Q4_K_S", "Q4_0",
            "Q3_K_M", "Q2_K", "MXFP4", "BF16", "F16",
        ];
        QUANTS.iter().copied().find(|q| f.contains(*q)).unwrap_or("Q4")
    }
}

// Curated, hardware-tiered download catalog. All repo/file pairs are verified to
// resolve to a single-file GGUF on Hugging Face. Listed smallest-first within tier.
pub const DOWNLOADABLE: &[DownloadEntry] = &[

    // ── SBC / EDGE — Raspberry Pi, mini-PCs, CPU, laptops (tiny & small) ────────
    DownloadEntry { display: "Qwen3 0.6B",          desc: "Tiniest capable model. Hybrid reasoning, runs on a Pi.", tier: Tier::Sbc, size_gb: 0.6,  hf_repo: "Qwen/Qwen3-0.6B-GGUF",                            hf_file: "Qwen3-0.6B-Q8_0.gguf"                        },
    DownloadEntry { display: "Gemma 3 1B",          desc: "Google's featherweight. Snappy chat on edge devices.", tier: Tier::Sbc, size_gb: 0.8,  hf_repo: "ggml-org/gemma-3-1b-it-GGUF",                     hf_file: "gemma-3-1b-it-Q4_K_M.gguf"                   },
    DownloadEntry { display: "Llama 3.2 1B",        desc: "Meta's 1B. Ultra-fast on CPU and SBCs.",              tier: Tier::Sbc, size_gb: 0.8,  hf_repo: "bartowski/Llama-3.2-1B-Instruct-GGUF",            hf_file: "Llama-3.2-1B-Instruct-Q4_K_M.gguf"           },
    DownloadEntry { display: "SmolLM2 1.7B",        desc: "Tiny but capable. Runs anywhere.",                    tier: Tier::Sbc, size_gb: 1.0,  hf_repo: "HuggingFaceTB/SmolLM2-1.7B-Instruct-GGUF",        hf_file: "smollm2-1.7b-instruct-q4_k_m.gguf"           },
    DownloadEntry { display: "Qwen3 1.7B",          desc: "Compact hybrid-reasoning model. Great per-watt.",     tier: Tier::Sbc, size_gb: 1.8,  hf_repo: "Qwen/Qwen3-1.7B-GGUF",                            hf_file: "Qwen3-1.7B-Q8_0.gguf"                        },
    DownloadEntry { display: "SmolLM3 3B",          desc: "Fully-open 3B w/ reasoning. Beats older 3Bs.",        tier: Tier::Sbc, size_gb: 1.9,  hf_repo: "ggml-org/SmolLM3-3B-GGUF",                        hf_file: "SmolLM3-Q4_K_M.gguf"                         },
    DownloadEntry { display: "Llama 3.2 3B",        desc: "Meta's small Llama. Solid all-rounder.",              tier: Tier::Sbc, size_gb: 2.0,  hf_repo: "bartowski/Llama-3.2-3B-Instruct-GGUF",            hf_file: "Llama-3.2-3B-Instruct-Q4_K_M.gguf"           },
    DownloadEntry { display: "Qwen2.5 3B",          desc: "Alibaba multilingual workhorse.",                     tier: Tier::Sbc, size_gb: 2.0,  hf_repo: "Qwen/Qwen2.5-3B-Instruct-GGUF",                   hf_file: "qwen2.5-3b-instruct-q4_k_m.gguf"             },
    DownloadEntry { display: "Gemma 3 4B",          desc: "On-device multimodal (text + image). Punchy.",        tier: Tier::Sbc, size_gb: 2.5,  hf_repo: "ggml-org/gemma-3-4b-it-GGUF",                     hf_file: "gemma-3-4b-it-Q4_K_M.gguf"                   },
    DownloadEntry { display: "Qwen3 4B",            desc: "Remarkable for its size. Reasoning + tool use.",      tier: Tier::Sbc, size_gb: 2.5,  hf_repo: "Qwen/Qwen3-4B-GGUF",                              hf_file: "Qwen3-4B-Q4_K_M.gguf"                        },
    DownloadEntry { display: "Phi-4 Mini 3.8B",     desc: "Microsoft's punchy small model. Great reasoning.",    tier: Tier::Sbc, size_gb: 2.5,  hf_repo: "unsloth/Phi-4-mini-instruct-GGUF",                hf_file: "Phi-4-mini-instruct-Q4_K_M.gguf"             },

    // ── MID — 8–16 GB GPUs: 3060 / 4060 / 4070 / 4080 (instruct, general, coder) ─
    DownloadEntry { display: "Mistral 7B",          desc: "Fast and strong general-purpose.",                    tier: Tier::Mid, size_gb: 4.1,  hf_repo: "bartowski/Mistral-7B-Instruct-v0.3-GGUF",         hf_file: "Mistral-7B-Instruct-v0.3-Q4_K_M.gguf"        },
    DownloadEntry { display: "Qwen2.5-Coder 7B",    desc: "Best-in-class small code model.",                     tier: Tier::Mid, size_gb: 4.7,  hf_repo: "Qwen/Qwen2.5-Coder-7B-Instruct-GGUF",             hf_file: "qwen2.5-coder-7b-instruct-q4_k_m.gguf"       },
    DownloadEntry { display: "Llama 3.1 8B",        desc: "Meta's flagship 8B. Excellent instructions.",         tier: Tier::Mid, size_gb: 4.7,  hf_repo: "bartowski/Meta-Llama-3.1-8B-Instruct-GGUF",       hf_file: "Meta-Llama-3.1-8B-Instruct-Q4_K_M.gguf"      },
    DownloadEntry { display: "Qwen3 8B",            desc: "Hybrid reasoning all-rounder. 100+ languages.",       tier: Tier::Mid, size_gb: 5.0,  hf_repo: "Qwen/Qwen3-8B-GGUF",                              hf_file: "Qwen3-8B-Q4_K_M.gguf"                        },
    DownloadEntry { display: "Gemma 3 12B",         desc: "Google's mid multimodal. Superb writing.",            tier: Tier::Mid, size_gb: 7.3,  hf_repo: "ggml-org/gemma-3-12b-it-GGUF",                    hf_file: "gemma-3-12b-it-Q4_K_M.gguf"                  },
    DownloadEntry { display: "Qwen3 14B",           desc: "Flagship-grade reasoning at 14B.",                    tier: Tier::Mid, size_gb: 9.0,  hf_repo: "Qwen/Qwen3-14B-GGUF",                             hf_file: "Qwen3-14B-Q4_K_M.gguf"                       },
    DownloadEntry { display: "Qwen2.5 14B",         desc: "Great balance of capability and size.",               tier: Tier::Mid, size_gb: 9.0,  hf_repo: "bartowski/Qwen2.5-14B-Instruct-GGUF",             hf_file: "Qwen2.5-14B-Instruct-Q4_K_M.gguf"            },
    DownloadEntry { display: "Qwen2.5-Coder 14B",   desc: "Top coding model for mid-range GPUs.",                tier: Tier::Mid, size_gb: 9.0,  hf_repo: "Qwen/Qwen2.5-Coder-14B-Instruct-GGUF",            hf_file: "qwen2.5-coder-14b-instruct-q4_k_m.gguf"      },
    DownloadEntry { display: "DeepSeek-R1 14B",     desc: "Chain-of-thought reasoning model.",                   tier: Tier::Mid, size_gb: 9.0,  hf_repo: "bartowski/DeepSeek-R1-Distill-Qwen-14B-GGUF",     hf_file: "DeepSeek-R1-Distill-Qwen-14B-Q4_K_M.gguf"    },
    DownloadEntry { display: "gpt-oss 20B",         desc: "OpenAI open MoE. Strong reasoning + agentic tools.",  tier: Tier::Mid, size_gb: 12.1, hf_repo: "ggml-org/gpt-oss-20b-GGUF",                       hf_file: "gpt-oss-20b-mxfp4.gguf"                      },
    DownloadEntry { display: "Mistral Small 3.2",   desc: "24B instruct workhorse. Vision + tool use.",          tier: Tier::Mid, size_gb: 14.3, hf_repo: "bartowski/mistralai_Mistral-Small-3.2-24B-Instruct-2506-GGUF", hf_file: "mistralai_Mistral-Small-3.2-24B-Instruct-2506-Q4_K_M.gguf" },
    DownloadEntry { display: "Devstral 24B",        desc: "Agentic coding specialist (OpenHands).",              tier: Tier::Mid, size_gb: 14.3, hf_repo: "unsloth/Devstral-Small-2507-GGUF",                hf_file: "Devstral-Small-2507-Q4_K_M.gguf"             },
    DownloadEntry { display: "Qwen3-Coder 30B",     desc: "MoE coder (3B active). Best open agentic coder. 16GB+.", tier: Tier::Mid, size_gb: 18.6, hf_repo: "unsloth/Qwen3-Coder-30B-A3B-Instruct-GGUF",     hf_file: "Qwen3-Coder-30B-A3B-Instruct-Q4_K_M.gguf"    },

    // ── HIGH — 24–32 GB GPUs: 3090 / 4090 / 5080 / 5090 (home power users) ───────
    DownloadEntry { display: "Gemma 3 27B",         desc: "Best dense multimodal for a single 24GB card.",       tier: Tier::High, size_gb: 16.5, hf_repo: "ggml-org/gemma-3-27b-it-GGUF",                   hf_file: "gemma-3-27b-it-Q4_K_M.gguf"                  },
    DownloadEntry { display: "Qwen3 32B",           desc: "Flagship dense reasoning. Frontier-class.",           tier: Tier::High, size_gb: 19.8, hf_repo: "Qwen/Qwen3-32B-GGUF",                            hf_file: "Qwen3-32B-Q4_K_M.gguf"                       },
    DownloadEntry { display: "Qwen2.5-Coder 32B",   desc: "Best open coder. Rivals frontier on code.",           tier: Tier::High, size_gb: 19.9, hf_repo: "bartowski/Qwen2.5-Coder-32B-Instruct-GGUF",      hf_file: "Qwen2.5-Coder-32B-Instruct-Q4_K_M.gguf"      },
    DownloadEntry { display: "Qwen2.5 32B",         desc: "Frontier-level general quality.",                     tier: Tier::High, size_gb: 20.0, hf_repo: "bartowski/Qwen2.5-32B-Instruct-GGUF",            hf_file: "Qwen2.5-32B-Instruct-Q4_K_M.gguf"            },
    DownloadEntry { display: "DeepSeek-R1 32B",     desc: "Best open reasoning distill.",                        tier: Tier::High, size_gb: 20.0, hf_repo: "bartowski/DeepSeek-R1-Distill-Qwen-32B-GGUF",    hf_file: "DeepSeek-R1-Distill-Qwen-32B-Q4_K_M.gguf"    },
    DownloadEntry { display: "Llama 3.3 70B",       desc: "Near-frontier. Needs 40 GB+ VRAM or large RAM.",      tier: Tier::High, size_gb: 43.0, hf_repo: "bartowski/Llama-3.3-70B-Instruct-GGUF",          hf_file: "Llama-3.3-70B-Instruct-Q4_K_M.gguf"          },
];

pub fn render(frame: &mut Frame, area: Rect, app: &mut App) {
    let dialog = centered_rect(84, 28, area);
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
        render_download_tab(frame, &chunks, app, dialog.width);
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
        Span::styled(" / ", Style::default().fg(app.theme.accent)),
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

fn render_download_tab(frame: &mut Frame, chunks: &[Rect], app: &mut App, dialog_width: u16) {
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
                Span::styled(" Select a model and press Enter to download", Style::default().fg(muted)),
            ])),
            hint_inner,
        );
    }

    // chunks[2]: list of downloadable models
    let models_dir = dirs::home_dir()
        .map(|h| h.join(".hyperlite").join("models"))
        .unwrap_or_default();
    let q = app.dialog_search_query.to_lowercase();
    let entries: Vec<&DownloadEntry> = DOWNLOADABLE.iter()
        .filter(|e| q.is_empty() || e.display.to_lowercase().contains(&q))
        .collect();

    // inner_w = dialog_width - 2 borders; highlight_symbol "► " = 2 extra; usable = inner_w - 2
    // marker(2) + tier(5) + name(20) + " " + size("xx.x GB"=7) + "  " + quant(7) = 44 fixed
    let desc_max = dialog_width.saturating_sub(2 + 2 + 2 + 5 + 20 + 1 + 7 + 2 + 7) as usize;

    let items: Vec<ListItem> = entries.iter().map(|e| {
        let installed = models_dir.join(e.hf_file).exists();
        let downloading = app.model_dl_active.as_deref() == Some(e.hf_file);
        let (marker, color) = if downloading {
            ("⬇ ", teal)
        } else if installed {
            ("✓ ", green)
        } else {
            ("  ", app.theme.text)
        };
        let tier_color = match e.tier {
            Tier::Sbc  => green,
            Tier::Mid  => teal,
            Tier::High => orange,
        };
        let size_str = format!("{:.1} GB", e.size_gb);
        let line = Line::from(vec![
            Span::styled(marker.to_string(),               Style::default().fg(color)),
            Span::styled(format!("{:<5}", e.tier.tag()),   Style::default().fg(tier_color).add_modifier(Modifier::BOLD)),
            Span::styled(format!("{:<20} ", e.display),     Style::default().fg(color)),
            Span::styled(format!("{:>7}  ", size_str),      Style::default().fg(color)),
            Span::styled(format!("{:<7}", e.quant()),       Style::default().fg(muted)),
            Span::styled(truncate(e.desc, desc_max),        Style::default().fg(app.theme.text_dim)),
        ]);
        ListItem::new(line).style(Style::default().fg(color))
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
