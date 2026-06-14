/// Full-screen change reviewer — takes over the screen when the agent proposes
/// file edits. Two views: an overview (the changeset as a mini-PR) and a
/// per-file diff view you drill into.

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use crate::app::{App, ChangeStatus, ReviewView};
use crate::session::message::DiffLineKind;

pub fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let theme = &app.theme;

    // Full background
    frame.render_widget(Block::default().style(Style::default().bg(theme.bg)), area);

    let Some(cs) = app.changeset.as_ref() else { return };

    let title = Line::from(vec![
        Span::styled(" ± Review Changes ", Style::default().fg(theme.accent).add_modifier(Modifier::BOLD)),
        Span::styled(
            format!("· {} file{} · ", cs.entries.len(), if cs.entries.len() == 1 { "" } else { "s" }),
            Style::default().fg(theme.text_muted),
        ),
        Span::styled(format!("+{} ", cs.total_added()), Style::default().fg(theme.diff_add_fg)),
        Span::styled(format!("−{} ", cs.total_removed()), Style::default().fg(theme.diff_del_fg)),
    ]);

    let outer = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border_hi))
        .title(title)
        .style(Style::default().bg(theme.bg));
    let inner = outer.inner(area);
    frame.render_widget(outer, area);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);

    match app.review_view {
        ReviewView::Overview => draw_overview(frame, rows[0], app, cs),
        ReviewView::File     => draw_file(frame, rows[0], app, cs),
    }
    draw_footer(frame, rows[1], app);
}

fn status_marker(status: ChangeStatus, theme: &crate::ui::theme::Theme) -> Span<'static> {
    match status {
        ChangeStatus::Pending => Span::styled("○ ", Style::default().fg(theme.text_dim)),
        ChangeStatus::Applied => Span::styled("✓ ", Style::default().fg(theme.success)),
        ChangeStatus::Skipped => Span::styled("✗ ", Style::default().fg(theme.text_dim)),
    }
}

fn draw_overview(frame: &mut Frame, area: Rect, app: &App, cs: &crate::app::Changeset) {
    let theme = &app.theme;
    let mut lines: Vec<Line<'static>> = vec![Line::default()];

    for (i, e) in cs.entries.iter().enumerate() {
        let selected = i == app.review_cursor;
        let cursor = if selected {
            Span::styled(" ▸ ", Style::default().fg(theme.accent).add_modifier(Modifier::BOLD))
        } else {
            Span::styled("   ", Style::default())
        };
        let tag = if e.is_new { "new " } else { "edit" };
        let tag_color = if e.is_new { theme.success } else { theme.warning };

        let name_style = if selected {
            Style::default().fg(theme.text).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.text_muted)
        };

        lines.push(Line::from(vec![
            cursor,
            status_marker(e.status, theme),
            Span::styled(format!("{} ", tag), Style::default().fg(tag_color).add_modifier(Modifier::BOLD)),
            Span::styled(format!("{:<44}", truncate(&e.file_path, 44)), name_style),
            Span::styled(format!("+{:<4}", e.added), Style::default().fg(theme.diff_add_fg)),
            Span::styled(format!("−{}", e.removed), Style::default().fg(theme.diff_del_fg)),
        ]));
    }

    frame.render_widget(Paragraph::new(lines), Rect { x: area.x + 1, ..area });
}

fn draw_file(frame: &mut Frame, area: Rect, app: &App, cs: &crate::app::Changeset) {
    let theme = &app.theme;
    let Some(e) = cs.entries.get(app.review_cursor) else { return };

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(1)])
        .split(area);

    // File header
    let tag = if e.is_new { "new file" } else { "modified" };
    frame.render_widget(
        Paragraph::new(vec![
            Line::default(),
            Line::from(vec![
                Span::styled(" ", Style::default()),
                status_marker(e.status, theme),
                Span::styled(e.file_path.clone(), Style::default().fg(theme.text).add_modifier(Modifier::BOLD)),
                Span::styled(format!("  {}  ", tag), Style::default().fg(theme.text_dim)),
                Span::styled(format!("+{} ", e.added), Style::default().fg(theme.diff_add_fg)),
                Span::styled(format!("−{}  ", e.removed), Style::default().fg(theme.diff_del_fg)),
                Span::styled(
                    format!("file {} / {}", app.review_cursor + 1, cs.entries.len()),
                    Style::default().fg(theme.text_muted),
                ),
            ]),
        ]),
        rows[0],
    );

    // Diff body — sign-column gutter + colored bg, scrollable
    let body = rows[1];
    let height = body.height as usize;
    let total  = e.diff_lines.len();
    let max_scroll = total.saturating_sub(height);
    let scroll = app.review_scroll.min(max_scroll);

    let mut lines: Vec<Line<'static>> = Vec::with_capacity(height);
    for dl in e.diff_lines.iter().skip(scroll).take(height) {
        lines.push(diff_line(dl, theme));
    }
    frame.render_widget(
        Paragraph::new(lines).style(Style::default().bg(theme.bg)),
        Rect { x: body.x + 1, y: body.y, width: body.width.saturating_sub(1), height: body.height },
    );
}

fn diff_line(dl: &crate::session::message::DiffLine, theme: &crate::ui::theme::Theme) -> Line<'static> {
    let (sign, fg, bg) = match dl.kind {
        DiffLineKind::Added   => ("+ ", theme.diff_add_fg, Some(theme.diff_add_bg)),
        DiffLineKind::Removed => ("− ", theme.diff_del_fg, Some(theme.diff_del_bg)),
        DiffLineKind::Header  => ("  ", theme.text_dim,    None),
        DiffLineKind::Context => ("  ", theme.text_dim,    None),
    };
    if dl.kind == DiffLineKind::Header {
        return Line::from(vec![
            Span::styled(format!("  {}", dl.content), Style::default().fg(theme.secondary).add_modifier(Modifier::ITALIC)),
        ]);
    }
    let style = match bg {
        Some(b) => Style::default().fg(fg).bg(b),
        None    => Style::default().fg(fg),
    };
    Line::from(vec![Span::styled(format!("{}{}", sign, dl.content), style)])
}

fn draw_footer(frame: &mut Frame, area: Rect, app: &App) {
    let theme = &app.theme;
    let kb = |key: &'static str, desc: &'static str| -> Vec<Span<'static>> {
        vec![
            Span::styled(format!(" {} ", key), Style::default().fg(theme.bg).bg(theme.accent).add_modifier(Modifier::BOLD)),
            Span::styled(format!(" {}  ", desc), Style::default().fg(theme.text_muted)),
        ]
    };
    let mut spans = vec![];
    match app.review_view {
        ReviewView::Overview => {
            spans.extend(kb("j/k", "move"));
            spans.extend(kb("⏎", "review file"));
            spans.extend(kb("A", "apply all"));
            spans.extend(kb("D", "discard all"));
            spans.extend(kb("Esc", "later"));
        }
        ReviewView::File => {
            spans.extend(kb("j/k", "scroll"));
            spans.extend(kb("[ ]", "prev/next file"));
            spans.extend(kb("y", "apply"));
            spans.extend(kb("n", "skip"));
            spans.extend(kb("A", "apply all"));
            spans.extend(kb("Esc", "back"));
        }
    }
    frame.render_widget(
        Paragraph::new(Line::from(spans)).alignment(Alignment::Center)
            .style(Style::default().bg(theme.bg_element)),
        area,
    );
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max { return s.to_string(); }
    // keep the tail (filename) visible
    let tail: String = s.chars().rev().take(max.saturating_sub(1)).collect::<Vec<_>>()
        .into_iter().rev().collect();
    format!("…{}", tail)
}
