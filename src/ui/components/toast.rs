/// Top-right toast notification overlay.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph};
use crate::app::App;

#[derive(Debug, Clone, PartialEq)]
pub enum ToastLevel {
    Info,
    Success,
    Warning,
    Error,
}

#[derive(Debug, Clone)]
pub struct Toast {
    pub message: String,
    pub level:   ToastLevel,
    pub ttl:     u8,   // ticks remaining
}

impl Toast {
    pub fn info(msg: impl Into<String>) -> Self {
        Self { message: msg.into(), level: ToastLevel::Info, ttl: 30 }
    }
    pub fn success(msg: impl Into<String>) -> Self {
        Self { message: msg.into(), level: ToastLevel::Success, ttl: 30 }
    }
    pub fn warning(msg: impl Into<String>) -> Self {
        Self { message: msg.into(), level: ToastLevel::Warning, ttl: 40 }
    }
    pub fn error(msg: impl Into<String>) -> Self {
        Self { message: msg.into(), level: ToastLevel::Error, ttl: 60 }
    }

    pub fn tick(&mut self) -> bool {
        if self.ttl > 0 { self.ttl -= 1; }
        self.ttl > 0
    }
}

pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let Some(toast) = app.toast.as_ref() else { return };

    let msg_len  = toast.message.chars().count() as u16;
    let width    = (msg_len + 6).min(area.width.saturating_sub(2));
    let x        = area.width.saturating_sub(width + 1);
    // 3 rows of bubble + 1 row of tail
    let toast_area = Rect { x, y: area.y, width, height: 4 };

    let (fg, icon) = match toast.level {
        ToastLevel::Info    => (app.theme.primary, " ▸ "),
        ToastLevel::Success => (app.theme.success,  " ✓ "),
        ToastLevel::Warning => (app.theme.warning,  " ⚠ "),
        ToastLevel::Error   => (app.theme.error,    " ✗ "),
    };

    frame.render_widget(Clear, toast_area);

    // Pixel speech bubble
    let bubble = Rect { x, y: area.y, width, height: 3 };
    frame.render_widget(
        Block::default().style(Style::default().bg(app.theme.bg_panel)),
        bubble,
    );
    super::pixel::frame(frame, bubble, fg, app.theme.bg_panel);

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(icon, Style::default().fg(fg).add_modifier(Modifier::BOLD)),
            Span::styled(toast.message.clone(), Style::default().fg(app.theme.text)),
        ])),
        super::pixel::inner(bubble),
    );

    // Tail: small pixel step under the bubble's left edge
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled("▀█", Style::default().fg(fg)))),
        Rect { x: x + 3, y: area.y + 3, width: 2.min(area.width.saturating_sub(x + 3)), height: 1 },
    );
}
