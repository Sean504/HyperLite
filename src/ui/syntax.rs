/// Syntax highlighting via syntect.
/// Returns ratatui-compatible styled lines from source code.

use once_cell::sync::Lazy;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use syntect::easy::HighlightLines;
use syntect::highlighting::{ThemeSet, Style as SynStyle};
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;

static SYNTAX_SET: Lazy<SyntaxSet> = Lazy::new(SyntaxSet::load_defaults_newlines);
static THEME_SET:  Lazy<ThemeSet>  = Lazy::new(ThemeSet::load_defaults);

/// Highlight `code` for the given language, returning ratatui Lines.
/// Falls back to plain text if language not recognized.
pub fn highlight(code: &str, lang: &str) -> Vec<Line<'static>> {
    let syntax = SYNTAX_SET
        .find_syntax_by_token(lang)
        .or_else(|| SYNTAX_SET.find_syntax_by_extension(lang))
        .unwrap_or_else(|| SYNTAX_SET.find_syntax_plain_text());

    // Use "base16-ocean.dark" which reads well on dark terminals
    let theme = THEME_SET.themes
        .get("base16-ocean.dark")
        .or_else(|| THEME_SET.themes.values().next())
        .unwrap();

    let mut highlighter = HighlightLines::new(syntax, theme);
    let mut lines = vec![];

    for line in LinesWithEndings::from(code) {
        let regions = highlighter
            .highlight_line(line, &SYNTAX_SET)
            .unwrap_or_default();

        let spans: Vec<Span<'static>> = regions
            .iter()
            .filter(|(_, text)| !text.is_empty() && *text != "\n")
            .map(|(style, text)| {
                let fg = syn_color_to_ratatui(style.foreground);
                let text = text.trim_end_matches('\n').to_string();
                Span::styled(text, Style::default().fg(fg))
            })
            .filter(|s| !s.content.is_empty())
            .collect();

        if !spans.is_empty() {
            lines.push(Line::from(spans));
        } else {
            lines.push(Line::default());
        }
    }

    lines
}

fn syn_color_to_ratatui(color: syntect::highlighting::Color) -> Color {
    Color::Rgb(color.r, color.g, color.b)
}
