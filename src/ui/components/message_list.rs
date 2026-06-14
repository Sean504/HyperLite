/// Scrollable message history with sticky-bottom behaviour.

use ratatui::Frame;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap};
use crate::app::App;
use super::message::render_message;

pub fn render(frame: &mut Frame, area: Rect, app: &mut App) {
    let block = Block::default()
        .style(Style::default().bg(app.theme.bg));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let width  = inner.width;
    let height = inner.height as usize;

    // Empty session + nothing streaming → splash instead of dead air
    let visible_count = app.messages.iter().filter(|m| !m.hidden).count();
    if visible_count == 0 && app.streaming_buf.is_empty() && !app.streaming {
        render_splash(frame, inner, app);
        return;
    }

    // Build all display lines from messages
    let mut all_lines: Vec<Line<'static>> = vec![];

    let mut first = true;
    for msg in app.messages.iter().filter(|m| !m.hidden) {
        if !first {
            all_lines.push(separator_line(width, app));
        }
        first = false;
        let msg_lines = render_message(msg, &app.theme, width, app.show_tool_details);
        all_lines.extend(msg_lines);
    }

    // Streaming partial line
    if !app.streaming_buf.is_empty() {
        let partial_md = crate::ui::markdown::render(&app.streaming_buf, &app.theme, width.saturating_sub(2));
        for md_line in partial_md.lines {
            let mut spans = vec![ratatui::text::Span::styled("  ", Style::default())];
            spans.extend(md_line.spans.into_iter());
            all_lines.push(Line::from(spans));
        }
    }

    // Count visual lines after wrapping so scroll calculation stays accurate.
    // Without this, wrapped lines inflate visual height but max_offset stays too
    // small, hiding the last messages under the input bar.
    let visual_lines = visual_line_count(&all_lines, width as usize);

    // Sticky bottom: if scroll_offset is at natural bottom, keep it there
    let max_offset = visual_lines.saturating_sub(height);
    if app.scroll_stick_bottom || app.scroll_offset >= max_offset {
        app.scroll_offset = max_offset;
    }

    let offset = app.scroll_offset.min(max_offset);

    let para = Paragraph::new(all_lines)
        .wrap(Wrap { trim: false })
        .scroll((offset as u16, 0))
        .style(Style::default().bg(app.theme.bg));

    frame.render_widget(para, inner);

    // Scrollbar
    if app.show_scrollbar && visual_lines > height {
        let mut sb_state = ScrollbarState::new(max_offset).position(offset);
        let sb = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(Some("↑"))
            .end_symbol(Some("↓"))
            .track_symbol(Some("│"))
            .thumb_symbol("█");

        let sb_area = Rect {
            x: area.x + area.width.saturating_sub(1),
            y: area.y,
            width: 1,
            height: area.height,
        };
        frame.render_stateful_widget(sb, sb_area, &mut sb_state);
    }
}

/// Dim CRT-printout separator between messages.
fn separator_line(width: u16, app: &App) -> Line<'static> {
    let w = (width as usize).saturating_sub(4).min(56);
    Line::from(vec![
        Span::styled("  ░▒".to_string(), Style::default().fg(app.theme.text_dim)),
        Span::styled("┄".repeat(w), Style::default().fg(app.theme.text_dim)),
        Span::styled("▒░".to_string(), Style::default().fg(app.theme.text_dim)),
    ])
}

// ── Empty-state splash ────────────────────────────────────────────────────────
// Pixel-art robot + wordmark + first-step hints. Drawn from a char-grid sprite:
//   # = casing  . = face panel  E = eye  M = mouth  (space = transparent)
// Each sprite pixel renders as a 2-char block so pixels come out square-ish.

const ROBOT: &[&str] = &[
    "    ##    ",
    "##########",
    "#........#",
    "#.EE..EE.#",
    "#........#",
    "#.MMMMMM.#",
    "##########",
    " ##    ## ",
];

fn render_splash(frame: &mut Frame, area: Rect, app: &App) {
    let theme = &app.theme;

    let mut lines: Vec<Line<'static>> = vec![];

    // Robot sprite, pixel grid → colored block spans
    for row in ROBOT {
        let mut spans: Vec<Span<'static>> = vec![];
        for ch in row.chars() {
            let (glyph, color) = match ch {
                '#' => ("██", theme.primary),
                '.' => ("░░", theme.bg_element),
                'E' => ("██", theme.accent),
                'M' => ("▓▓", theme.text_dim),
                _   => ("  ", theme.bg),
            };
            spans.push(Span::styled(glyph.to_string(), Style::default().fg(color)));
        }
        lines.push(Line::from(spans));
    }

    lines.push(Line::default());
    lines.push(Line::from(vec![
        Span::styled("H Y P E R L I T E", Style::default().fg(theme.text).add_modifier(Modifier::BOLD)),
    ]));
    lines.push(Line::default());

    // Each hint is its own centered line so the whole block sits under the robot
    let hint = |key: &'static str, desc: &'static str| -> Line<'static> {
        Line::from(vec![
            Span::styled(key.to_string(), Style::default().fg(theme.accent).add_modifier(Modifier::BOLD)),
            Span::styled(format!("  {}", desc), Style::default().fg(theme.text_muted)),
        ])
    };
    lines.push(hint("⏎", "type below to start"));
    lines.push(hint("Ctrl+K", "command palette"));
    lines.push(hint("Ctrl+M", "switch model"));
    lines.push(hint("?", "all keybinds"));

    // Center vertically + horizontally
    let h = lines.len() as u16;
    let y = area.y + area.height.saturating_sub(h) / 2;
    let splash_area = Rect { x: area.x, y, width: area.width, height: h.min(area.height) };
    frame.render_widget(
        Paragraph::new(lines).alignment(Alignment::Center),
        splash_area,
    );
}

/// Count the total visual lines after word-wrap at the given width.
fn visual_line_count(lines: &[Line], width: usize) -> usize {
    if width == 0 { return lines.len(); }
    lines.iter().map(|line| {
        let w: usize = line.spans.iter()
            .map(|s| unicode_width::UnicodeWidthStr::width(s.content.as_ref()))
            .sum();
        if w == 0 { 1 } else { (w + width - 1) / width }
    }).sum()
}
