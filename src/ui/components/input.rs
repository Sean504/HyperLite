/// Input area using tui-textarea, wrapped in a thin rounded frame.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders};
use crate::app::App;

const PLACEHOLDERS: &[&str] = &[
    "Ask anything…",
    "Type a message… (Enter to send, Alt+Enter for newline)",
    "Ctrl+K for command palette",
    "Ctrl+M to switch model",
    "Ctrl+\\ to toggle sidebar",
];

pub fn render(frame: &mut Frame, area: Rect, app: &mut App) {
    let is_streaming = app.is_streaming();
    let is_empty     = app.textarea_is_empty();

    let border_color = if is_streaming {
        app.theme.warning
    } else {
        app.theme.border_hi
    };

    // Title — streaming phase/spinner, or a small label chip
    let title = if is_streaming {
        let label = if !app.stream_status.is_empty() {
            format!(" {} {} ", app.spinner.current_for(app.theme.name), app.stream_status.trim())
        } else {
            format!(" {} generating… ", app.spinner.current_for(app.theme.name))
        };
        Line::from(vec![Span::styled(
            label,
            Style::default().fg(app.theme.warning).add_modifier(Modifier::BOLD),
        )])
    } else {
        Line::from(vec![Span::styled(
            " ▸ msg ",
            Style::default().fg(app.theme.primary).add_modifier(Modifier::BOLD),
        )])
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_color))
        .title(title)
        .style(Style::default().bg(app.theme.bg_panel));

    if is_empty && !is_streaming {
        // Placeholder with a single blinking caret inside the input area
        let placeholder = PLACEHOLDERS[app.placeholder_idx % PLACEHOLDERS.len()];
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let caret = if app.cursor_blink_on { "▌ " } else { "  " };
        frame.render_widget(
            ratatui::widgets::Paragraph::new(Line::from(vec![
                Span::styled(caret, Style::default().fg(app.theme.accent)),
                Span::styled(placeholder, Style::default().fg(app.theme.text_dim)),
            ])),
            inner,
        );
    } else {
        // tui-textarea with blinking cursor (hidden while streaming)
        app.textarea.set_block(block);
        app.textarea.set_style(Style::default().fg(app.theme.text).bg(app.theme.bg_panel));

        if is_streaming {
            app.textarea.set_cursor_style(Style::default().bg(app.theme.bg_panel).fg(app.theme.bg_panel));
            app.textarea.set_cursor_line_style(Style::default());
        } else {
            let cursor_style = if app.cursor_blink_on {
                Style::default().fg(app.theme.bg).bg(app.theme.accent)
            } else {
                Style::default().fg(app.theme.bg_panel).bg(app.theme.bg_panel)
            };
            app.textarea.set_cursor_style(cursor_style);
            app.textarea.set_cursor_line_style(Style::default());
        }

        frame.render_widget(&app.textarea, area);
    }
}
