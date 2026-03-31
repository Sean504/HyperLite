/// Markdown → ratatui Text spans via pulldown-cmark.

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use crate::ui::theme::Theme;

pub fn render(markdown: &str, theme: &Theme, width: u16) -> Text<'static> {
    let opts = Options::all();
    let parser = Parser::new_ext(markdown, opts);

    let mut lines: Vec<Line<'static>> = vec![];
    let mut current_spans: Vec<Span<'static>> = vec![];

    let mut bold        = false;
    let mut italic      = false;
    let mut code_inline = false;
    let mut in_code_block = false;
    let mut code_lang   = String::new();
    let mut code_buf    = String::new();
    let mut heading_level = 0u8;
    let mut _in_list    = false;
    let mut list_item   = false;

    let base_style  = Style::default().fg(theme.text);
    let muted_style = Style::default().fg(theme.text_muted);
    let code_style  = Style::default().fg(theme.primary);
    let h1_style    = Style::default().fg(theme.primary).add_modifier(Modifier::BOLD);
    let h2_style    = Style::default().fg(theme.accent).add_modifier(Modifier::BOLD);
    let h3_style    = Style::default().fg(theme.secondary).add_modifier(Modifier::BOLD);
    let link_style  = Style::default().fg(theme.accent);
    let quote_style = Style::default().fg(theme.text_muted).add_modifier(Modifier::ITALIC);
    let hr_char     = "─".repeat(width.saturating_sub(4) as usize);

    macro_rules! push_text {
        ($text:expr, $style:expr) => {
            current_spans.push(Span::styled($text.to_string(), $style))
        };
    }

    macro_rules! flush_line {
        () => {{
            let spans = std::mem::take(&mut current_spans);
            lines.push(Line::from(spans));
        }};
    }

    for event in parser {
        match event {
            // ── Headings ────────────────────────────────────────────────────
            Event::Start(Tag::Heading { level, .. }) => {
                flush_line!();
                heading_level = level as u8;
            }
            Event::End(TagEnd::Heading(_)) => {
                let prefix = "#".repeat(heading_level as usize);
                let style  = match heading_level {
                    1 => h1_style,
                    2 => h2_style,
                    _ => h3_style,
                };
                // Prepend marker to first span
                if let Some(first) = current_spans.first_mut() {
                    let new_content = format!("{} {}", prefix, first.content);
                    *first = Span::styled(new_content, style);
                    for span in current_spans.iter_mut().skip(1) {
                        span.style = style;
                    }
                }
                flush_line!();
                lines.push(Line::default());
                heading_level = 0;
            }

            // ── Paragraphs ──────────────────────────────────────────────────
            Event::Start(Tag::Paragraph) => {}
            Event::End(TagEnd::Paragraph) => {
                flush_line!();
                lines.push(Line::default());
            }

            // ── Code block ──────────────────────────────────────────────────
            Event::Start(Tag::CodeBlock(kind)) => {
                in_code_block = true;
                code_lang = match kind {
                    pulldown_cmark::CodeBlockKind::Fenced(lang) => lang.to_string(),
                    _ => String::new(),
                };
                code_buf.clear();
                flush_line!();
            }
            Event::End(TagEnd::CodeBlock) => {
                in_code_block = false;
                // Top border with language tag
                let lang_label = if code_lang.is_empty() {
                    "code".to_string()
                } else {
                    code_lang.clone()
                };
                let border_line = format!(" ╭─ {} ", lang_label);
                lines.push(Line::from(vec![
                    Span::styled(border_line, Style::default().fg(theme.border_hi)),
                ]));
                for code_line in code_buf.lines() {
                    lines.push(Line::from(vec![
                        Span::styled("  ", Style::default().fg(theme.border)),
                        Span::styled(code_line.to_string(), Style::default().fg(theme.text)),
                    ]));
                }
                lines.push(Line::from(vec![
                    Span::styled(" ╰────", Style::default().fg(theme.border_hi)),
                ]));
                lines.push(Line::default());
                code_buf.clear();
            }

            // ── Inline code ─────────────────────────────────────────────────
            Event::Start(Tag::Emphasis)  => { italic = true; }
            Event::End(TagEnd::Emphasis) => { italic = false; }
            Event::Start(Tag::Strong)    => { bold = true; }
            Event::End(TagEnd::Strong)   => { bold = false; }

            // ── Lists ────────────────────────────────────────────────────────
            Event::Start(Tag::List(_)) => { _in_list = true; }
            Event::End(TagEnd::List(_)) => { _in_list = false; lines.push(Line::default()); }
            Event::Start(Tag::Item) => { list_item = true; }
            Event::End(TagEnd::Item) => {
                list_item = false;
                flush_line!();
            }

            // ── Blockquote ───────────────────────────────────────────────────
            Event::Start(Tag::BlockQuote(_)) => {
                push_text!("  ┃ ", quote_style);
            }
            Event::End(TagEnd::BlockQuote(_)) => {
                flush_line!();
            }

            // ── Horizontal rule ──────────────────────────────────────────────
            Event::Rule => {
                flush_line!();
                lines.push(Line::from(vec![
                    Span::styled(hr_char.clone(), Style::default().fg(theme.border)),
                ]));
            }

            // ── Links ────────────────────────────────────────────────────────
            Event::Start(Tag::Link { dest_url, .. }) => {
                // We'll show the link text in accent + URL in muted
                let url = dest_url.to_string();
                // Push URL as suffix when End fires — store it
                // (simple: just set a flag; link text flows as normal events)
            }
            Event::End(TagEnd::Link) => {}

            // ── Text content ─────────────────────────────────────────────────
            Event::Text(text) => {
                if in_code_block {
                    code_buf.push_str(&text);
                    continue;
                }

                let text_str = text.to_string();

                let style = if code_inline {
                    code_style
                } else {
                    let mut s = base_style;
                    if bold   { s = s.add_modifier(Modifier::BOLD); }
                    if italic { s = s.add_modifier(Modifier::ITALIC); }
                    s
                };

                if list_item && current_spans.is_empty() {
                    push_text!("  • ", Style::default().fg(theme.accent));
                }

                push_text!(text_str, style);
            }

            Event::Code(text) => {
                push_text!(format!("`{}`", text), code_style);
            }

            Event::SoftBreak | Event::HardBreak => {
                flush_line!();
            }

            _ => {}
        }
    }

    // Flush any remaining content
    if !current_spans.is_empty() {
        flush_line!();
    }

    // Remove trailing empty lines
    while lines.last().map(|l: &Line| l.spans.is_empty()).unwrap_or(false) {
        lines.pop();
    }

    Text::from(lines)
}
