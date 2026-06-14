/// Single-row status HUD at the bottom.
///
/// Left:  state chip (RDY / blinking GEN / DIFF) · model · mode chips · key hints
/// Right: token count · clock

use ratatui::Frame;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use crate::app::App;

pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let bg_style = Style::default().bg(app.theme.bg_element);

    frame.render_widget(
        Paragraph::new(Line::from(build_left(app))).style(bg_style),
        area,
    );
    frame.render_widget(
        Paragraph::new(Line::from(build_right(app)))
            .style(bg_style)
            .alignment(Alignment::Right),
        area,
    );
}

fn build_left(app: &App) -> Vec<Span<'static>> {
    let mut spans = vec![];

    // ── State chip ───────────────────────────────────────────────────────────
    if app.is_streaming() {
        // Blinking REC-style indicator while the model generates
        let (fg, bg) = if app.cursor_blink_on {
            (app.theme.bg, app.theme.warning)
        } else {
            (app.theme.warning, app.theme.bg_element)
        };
        spans.push(Span::styled(" ▮ GEN ", Style::default().fg(fg).bg(bg).add_modifier(Modifier::BOLD)));
    } else if app.changeset.is_some() {
        spans.push(Span::styled(" ± REVIEW ", Style::default().fg(app.theme.bg).bg(app.theme.accent).add_modifier(Modifier::BOLD)));
    } else {
        spans.push(Span::styled(" ⬢ RDY ", Style::default().fg(app.theme.bg).bg(app.theme.success).add_modifier(Modifier::BOLD)));
    }

    // ── Model · backend ──────────────────────────────────────────────────────
    let mut model = app.current_model_name();
    if model.chars().count() > 24 {
        model = model.chars().take(23).collect::<String>() + "…";
    }
    spans.push(Span::styled(" ", Style::default()));
    spans.push(Span::styled(model, Style::default().fg(app.theme.primary).add_modifier(Modifier::BOLD)));
    spans.push(Span::styled(format!("·{} ", app.current_backend_name()), Style::default().fg(app.theme.text_dim)));

    // ── Mode chips ───────────────────────────────────────────────────────────
    if app.sandbox_enabled {
        spans.push(Span::styled("⬡ sbx ", Style::default().fg(app.theme.yellow)));
    }

    // ── Key hints ────────────────────────────────────────────────────────────
    let kb = |key: &'static str, desc: &'static str| -> Vec<Span<'static>> {
        vec![
            Span::styled(format!(" {} ", key), Style::default().fg(app.theme.bg_menu).bg(app.theme.text_muted)),
            Span::styled(format!(" {} ", desc), Style::default().fg(app.theme.text_muted)),
        ]
    };

    if app.changeset.is_some() {
        // Reviewer is deferred (closed but pending) — offer to reopen
        spans.extend(kb("Ctrl+R", "review changes"));
    } else if app.is_streaming() {
        spans.extend(kb("Ctrl+C", "stop"));
    } else {
        spans.extend(kb("?", "help"));
        spans.extend(kb("Ctrl+K", "palette"));
        spans.extend(kb("Ctrl+N", "new"));
        spans.extend(kb("Ctrl+S", "sessions"));
    }

    spans
}

fn build_right(app: &App) -> Vec<Span<'static>> {
    let mut spans = vec![];

    if let Some(tokens) = app.last_token_count {
        let label = if tokens >= 1000 {
            format!("{:.1}k tok", tokens as f64 / 1000.0)
        } else {
            format!("{} tok", tokens)
        };
        spans.push(Span::styled(label, Style::default().fg(app.theme.text_dim)));
        spans.push(Span::styled(" · ", Style::default().fg(app.theme.text_dim)));
    }

    spans.push(Span::styled(
        chrono::Local::now().format("%H:%M").to_string(),
        Style::default().fg(app.theme.text_muted),
    ));
    spans.push(Span::styled(" ", Style::default()));

    spans
}
