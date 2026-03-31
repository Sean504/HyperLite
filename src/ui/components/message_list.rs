/// Scrollable message history with sticky-bottom behaviour.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::widgets::{Block, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState};
use crate::app::App;
use super::message::render_message;

pub fn render(frame: &mut Frame, area: Rect, app: &mut App) {
    let block = Block::default()
        .style(Style::default().bg(app.theme.bg));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let width  = inner.width;
    let height = inner.height as usize;

    // Build all display lines from messages
    let mut all_lines: Vec<Line<'static>> = vec![];

    // Project context banner if active
    if app.project_context_active {
        all_lines.push(Line::from(ratatui::text::Span::styled(
            format!(" 󰉋  Project context active: {}", app.project_name()),
            Style::default().fg(app.theme.accent),
        )));
        all_lines.push(Line::default());
    }

    for msg in &app.messages {
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

    let total_lines = all_lines.len();

    // Sticky bottom: if scroll_offset is at natural bottom, keep it there
    let max_offset = total_lines.saturating_sub(height);
    if app.scroll_stick_bottom || app.scroll_offset >= max_offset {
        app.scroll_offset = max_offset;
    }

    let offset = app.scroll_offset.min(max_offset);

    let para = Paragraph::new(all_lines)
        .scroll((offset as u16, 0))
        .style(Style::default().bg(app.theme.bg));

    frame.render_widget(para, inner);

    // Scrollbar
    if app.show_scrollbar && total_lines > height {
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
