/// Input area using tui-textarea.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders};
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

    // Title — generating spinner OR static "message" label
    let title_left = if is_streaming {
        Line::from(vec![
            Span::styled(
                format!(" {} generating… ", app.spinner.current()),
                Style::default().fg(app.theme.warning).add_modifier(Modifier::BOLD),
            ),
        ])
    } else {
        Line::from(vec![
            Span::styled(" message ", Style::default().fg(app.theme.primary).add_modifier(Modifier::BOLD)),
        ])
    };

    let title_right = Line::from(vec![
        Span::styled(
            format!(" {} ", app.current_model_name()),
            Style::default().fg(app.theme.text_muted),
        ),
    ]);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .title(title_left)
        .title_bottom(title_right)
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
            // Hide cursor while model is responding
            app.textarea.set_cursor_style(Style::default().bg(app.theme.bg_panel).fg(app.theme.bg_panel));
            app.textarea.set_cursor_line_style(Style::default());
        } else {
            // Blink: visible half the time
            let cursor_style = if app.cursor_blink_on {
                Style::default().fg(app.theme.bg).bg(app.theme.accent)
            } else {
                // Invisible — same color as background
                Style::default().fg(app.theme.bg_panel).bg(app.theme.bg_panel)
            };
            app.textarea.set_cursor_style(cursor_style);
            app.textarea.set_cursor_line_style(Style::default());
        }

        frame.render_widget(&app.textarea, area);
    }
}
