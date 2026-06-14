/// Pixel-art toolkit: chunky quantized frames and half-block sprite rendering.
///
/// The 16-bit look comes from drawing chrome on a pixel grid instead of with
/// thin box-drawing lines:
///   - walls are half-blocks (▄ ▀ ▐ ▌) — half a cell thick
///   - corners are quadrant blocks (▗ ▖ ▝ ▘) — rounded by quantization
///   - sprites are char grids composited 2-pixels-per-cell with ▀/▄
///     (fg = top pixel, bg = bottom pixel), which gives square-ish pixels.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

/// Draw a chunky pixel frame around `area` (border occupies the outermost
/// cells). Contents go inside `inner(area)`.
pub fn frame(f: &mut Frame, area: Rect, color: Color, bg: Color) {
    if area.width < 2 || area.height < 2 { return; }
    let fg = Style::default().fg(color).bg(bg);
    let w  = area.width as usize;

    // Top: ▗▄▄▄▄▖
    let top = format!("▗{}▖", "▄".repeat(w.saturating_sub(2)));
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(top, fg))),
        Rect { x: area.x, y: area.y, width: area.width, height: 1 },
    );

    // Bottom: ▝▀▀▀▀▘
    let bottom = format!("▝{}▘", "▀".repeat(w.saturating_sub(2)));
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(bottom, fg))),
        Rect { x: area.x, y: area.y + area.height - 1, width: area.width, height: 1 },
    );

    // Sides: ▐ (left wall hugs content) and ▌ (right wall)
    for dy in 1..area.height.saturating_sub(1) {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled("▐", fg))),
            Rect { x: area.x, y: area.y + dy, width: 1, height: 1 },
        );
        f.render_widget(
            Paragraph::new(Line::from(Span::styled("▌", fg))),
            Rect { x: area.x + area.width - 1, y: area.y + dy, width: 1, height: 1 },
        );
    }
}

/// The content area inside a pixel frame.
pub fn inner(area: Rect) -> Rect {
    Rect {
        x:      area.x + 1,
        y:      area.y + 1,
        width:  area.width.saturating_sub(2),
        height: area.height.saturating_sub(2),
    }
}

/// Composite a char-grid sprite into Lines using half-blocks: each pair of
/// grid rows becomes one terminal row. `palette` maps a grid char to a pixel
/// color (None = transparent). `bg` shows through transparent pixels.
pub fn sprite_lines(
    grid: &[String],
    palette: &dyn Fn(char) -> Option<Color>,
    bg: Color,
) -> Vec<Line<'static>> {
    let mut lines = Vec::with_capacity((grid.len() + 1) / 2);

    for pair in grid.chunks(2) {
        let top: Vec<char>    = pair[0].chars().collect();
        let bottom: Vec<char> = pair.get(1).map(|r| r.chars().collect()).unwrap_or_default();
        let width = top.len().max(bottom.len());

        let mut spans: Vec<Span<'static>> = Vec::with_capacity(width);
        for i in 0..width {
            let a = top.get(i).copied().map(palette).flatten();
            let b = bottom.get(i).copied().map(palette).flatten();
            let span = match (a, b) {
                (Some(ca), Some(cb)) => Span::styled("▀", Style::default().fg(ca).bg(cb)),
                (Some(ca), None)     => Span::styled("▀", Style::default().fg(ca).bg(bg)),
                (None, Some(cb))     => Span::styled("▄", Style::default().fg(cb).bg(bg)),
                (None, None)         => Span::styled(" ", Style::default().bg(bg)),
            };
            spans.push(span);
        }
        lines.push(Line::from(spans));
    }
    lines
}
