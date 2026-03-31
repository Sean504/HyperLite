use ratatui::layout::{Constraint, Direction, Layout, Rect};
use crate::app::App;
use crate::config::SidebarMode;

pub struct AppLayout {
    pub messages: Rect,
    pub sidebar:  Option<Rect>,
    pub input:    Rect,
    pub footer:   Rect,
}

const SIDEBAR_WIDTH:  u16 = 42;
const FOOTER_HEIGHT:  u16 = 1;
/// Input area min/max rows
const INPUT_MIN:      u16 = 3;
const INPUT_MAX:      u16 = 12;

pub fn compute(area: Rect, app: &App) -> AppLayout {
    let wide        = area.width >= 120;
    let show_sidebar = match &app.config.sidebar {
        SidebarMode::Always => true,
        SidebarMode::Never  => false,
        SidebarMode::Auto   => wide && app.sidebar_open,
    };

    // Input height: count input lines clamped
    let input_lines = app.textarea.lines().len() as u16;
    let input_h = (input_lines + 2).clamp(INPUT_MIN, INPUT_MAX); // +2 for border

    // Vertical split: [messages+sidebar] / [input] / [footer]
    let vert = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(input_h),
            Constraint::Length(FOOTER_HEIGHT),
        ])
        .split(area);

    let content_row = vert[0];
    let input_row   = vert[1];
    let footer_row  = vert[2];

    // Horizontal split of content row
    let (messages, sidebar) = if show_sidebar && area.width > SIDEBAR_WIDTH + 20 {
        let horiz = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Min(0),
                Constraint::Length(SIDEBAR_WIDTH),
            ])
            .split(content_row);
        (horiz[0], Some(horiz[1]))
    } else {
        (content_row, None)
    };

    AppLayout { messages, sidebar, input: input_row, footer: footer_row }
}
