/// Top-right toast notification overlay.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
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
    let width    = (msg_len + 4).min(area.width.saturating_sub(2));
    let x        = area.width.saturating_sub(width + 1);
    let toast_area = Rect { x, y: area.y, width, height: 3 };

    let (fg, icon) = match toast.level {
        ToastLevel::Info    => (app.theme.primary, " ℹ "),
        ToastLevel::Success => (app.theme.success,  " ✓ "),
        ToastLevel::Warning => (app.theme.warning,  " ⚠ "),
        ToastLevel::Error   => (app.theme.error,    " ✗ "),
    };

    let border_style = Style::default().fg(fg);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .style(Style::default().bg(app.theme.bg_panel));

    let line = Line::from(vec![
        Span::styled(icon, Style::default().fg(fg).add_modifier(Modifier::BOLD)),
        Span::styled(toast.message.clone(), Style::default().fg(app.theme.text)),
    ]);

    let para = Paragraph::new(line).block(block);

    frame.render_widget(Clear, toast_area);
    frame.render_widget(para, toast_area);
}
