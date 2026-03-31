pub mod theme;
pub mod layout;
pub mod markdown;
pub mod syntax;
pub mod components;
pub mod dialogs;

use ratatui::Frame;
use crate::app::App;

/// Main render entry point — called every frame.
pub fn draw(frame: &mut Frame, app: &mut App) {
    let area = frame.area();
    let lay  = layout::compute(area, app);

    // Background fill with theme bg color
    let bg = ratatui::widgets::Block::default()
        .style(ratatui::style::Style::default().bg(app.theme.bg));
    frame.render_widget(bg, area);

    // Message list
    components::message_list::render(frame, lay.messages, app);

    // Sidebar
    if lay.sidebar.is_some() {
        components::sidebar::render(frame, lay.sidebar.unwrap(), app);
    }

    // Input / permission / question area
    match &app.active_prompt {
        crate::app::ActivePrompt::Input | crate::app::ActivePrompt::Rename => {
            components::input::render(frame, lay.input, app);
        }
        crate::app::ActivePrompt::Permission => {
            components::permission::render(frame, lay.input, app);
        }
    }

    // Footer
    components::footer::render(frame, lay.footer, app);

    // Toast overlay
    components::toast::render(frame, area, app);

    // Dialog overlay (topmost)
    if app.active_dialog != crate::app::ActiveDialog::None {
        dialogs::render(frame, area, app);
    }
}
