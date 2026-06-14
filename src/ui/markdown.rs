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
                let lang_label = if code_lang.is_empty() { "code".to_string() } else { code_lang.clone() };

                let total    = code_buf.lines().count().max(1);
                let inner_w  = (width.saturating_sub(2) as usize).max(12);
                let gutter_w = format!("{}", total).len().max(2);

                // ── Card top: ╭─ lang ─────────────────── ⧉ copy ─╮ ──────────────
                let badge = format!("─ {} ", lang_label);
                let copy_hint = " ⧉ ⌘K copy ";
                let used = badge.chars().count() + copy_hint.chars().count() + 2;
                let fill = inner_w.saturating_sub(used);
                lines.push(Line::from(vec![
                    Span::styled(format!(" ╭{}", badge), Style::default().fg(theme.border_hi)),
                    Span::styled("─".repeat(fill), Style::default().fg(theme.border)),
                    Span::styled(copy_hint, Style::default().fg(theme.text_dim)),
                    Span::styled("╮", Style::default().fg(theme.border_hi)),
                ]));

                // ── Body: gutter line numbers, collapse very long blocks ────────
                let highlighted = crate::ui::syntax::highlight(&code_buf, &code_lang);
                let show = if total > 40 { 30 } else { total };
                for (n, hl_line) in highlighted.into_iter().enumerate() {
                    if n >= show { break; }
                    let mut spans = vec![
                        Span::styled(" │ ", Style::default().fg(theme.border_hi)),
                        Span::styled(format!("{:>w$}  ", n + 1, w = gutter_w), Style::default().fg(theme.text_dim)),
                    ];
                    spans.extend(hl_line.spans.into_iter());
                    lines.push(Line::from(spans));
                }
                if total > show {
                    lines.push(Line::from(vec![
                        Span::styled(" │ ", Style::default().fg(theme.border_hi)),
                        Span::styled(format!("… {} more lines (Copy Last Code)", total - show),
                            Style::default().fg(theme.text_dim).add_modifier(Modifier::ITALIC)),
                    ]));
                }

                // ── Card bottom: ╰─ N lines ────────────────────────────────────
                let footer = format!("─ {} line{} ", total, if total == 1 { "" } else { "s" });
                let ffill = inner_w.saturating_sub(footer.chars().count() + 1);
                lines.push(Line::from(vec![
                    Span::styled(format!(" ╰{}", footer), Style::default().fg(theme.border_hi)),
                    Span::styled("─".repeat(ffill), Style::default().fg(theme.border)),
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

                // Word-wrap prose text at the available width so ratatui never
                // has to split mid-word. The 2-space indent is already accounted
                // for by the caller, so we wrap at width directly.
                if !code_inline && !list_item && width > 4 {
                    let wrap_w = width.saturating_sub(4) as usize;
                    let wrapped = word_wrap(&text_str, wrap_w);
                    let mut iter = wrapped.into_iter();
                    if let Some(first) = iter.next() {
                        push_text!(first, style);
                        for cont in iter {
                            flush_line!();
                            push_text!(cont, style);
                        }
                    }
                } else {
                    push_text!(text_str, style);
                }
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

/// Word-wrap a string at `max_width` columns, returning one string per line.
fn word_wrap(text: &str, max_width: usize) -> Vec<String> {
    if max_width == 0 { return vec![text.to_string()]; }
    let mut lines = vec![];
    let mut current = String::new();
    let mut current_w = 0usize;

    for word in text.split(' ') {
        let word_w = unicode_width::UnicodeWidthStr::width(word);
        if current.is_empty() {
            current.push_str(word);
            current_w = word_w;
        } else if current_w + 1 + word_w <= max_width {
            current.push(' ');
            current.push_str(word);
            current_w += 1 + word_w;
        } else {
            lines.push(current.clone());
            current = word.to_string();
            current_w = word_w;
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    if lines.is_empty() { lines.push(String::new()); }
    lines
}
