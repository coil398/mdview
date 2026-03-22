use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};
use ratatui::style::{Color, Modifier, Style};

use crate::parser::Highlighter;
use crate::types::{StyledLine, StyledSpan, TocEntry};

pub fn parse_markdown(text: &str, hl: &Highlighter) -> (Vec<StyledLine>, Vec<TocEntry>) {
    let mut lines: Vec<StyledLine> = Vec::new();
    let mut toc: Vec<TocEntry> = Vec::new();

    let mut current_line: StyledLine = Vec::new();

    // Inline state stack
    let mut inline_style: Style = Style::default();
    let mut in_heading: Option<(u8, Style)> = None;
    let mut in_code_block: bool = false;
    let mut code_block_lang: String = String::new();
    let mut code_block_content: String = String::new();
    let mut in_strong: bool = false;
    let mut in_emphasis: bool = false;
    let mut in_link: bool = false;
    let mut in_item: bool = false;
    let mut item_prefix_done: bool = false;
    let mut _in_paragraph: bool = false;

    let options = Options::all();
    let parser = Parser::new_ext(text, options);

    let h1_style = Style::default()
        .fg(Color::Cyan)
        .add_modifier(Modifier::BOLD);
    let h2_style = Style::default()
        .fg(Color::Blue)
        .add_modifier(Modifier::BOLD);
    let h3_style = Style::default().fg(Color::Green);
    let inline_code_style = Style::default().fg(Color::Yellow);
    let link_style = Style::default().fg(Color::Cyan);
    let bullet_style = Style::default().fg(Color::Yellow);
    let rule_style = Style::default().fg(Color::DarkGray);
    let badge_style = Style::default()
        .fg(Color::Black)
        .bg(Color::Green)
        .add_modifier(Modifier::BOLD);

    // Helper closure is not possible in Rust easily, so we use a macro-like approach via inline logic
    let flush_line = |lines: &mut Vec<StyledLine>, current_line: &mut StyledLine| {
        let line = std::mem::take(current_line);
        lines.push(line);
    };

    for event in parser {
        match event {
            // ── Block starts ──────────────────────────────────────────────
            Event::Start(Tag::Heading { level, .. }) => {
                let lvl = level as u8;
                let style = match lvl {
                    1 => h1_style,
                    2 => h2_style,
                    _ => h3_style,
                };
                in_heading = Some((lvl, style));
                inline_style = style;

                // Heading prefix
                let prefix = match lvl {
                    1 => "█ ",
                    2 => "▌ ",
                    _ => "  ▸ ",
                };
                current_line.push(StyledSpan {
                    text: prefix.to_string(),
                    style,
                });
            }
            Event::End(TagEnd::Heading(_)) => {
                if let Some((lvl, _)) = in_heading.take() {
                    let line_index = lines.len();
                    // Extract heading text from current_line (skip prefix span)
                    let title = current_line
                        .iter()
                        .skip(1) // skip the prefix span
                        .map(|s| s.text.as_str())
                        .collect::<Vec<_>>()
                        .join("");
                    toc.push(TocEntry {
                        line_index,
                        title,
                        level: lvl,
                    });
                    flush_line(&mut lines, &mut current_line);
                    inline_style = Style::default();
                }
            }

            Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(lang))) => {
                in_code_block = true;
                code_block_lang = lang.to_string();
                code_block_content.clear();
            }
            Event::Start(Tag::CodeBlock(CodeBlockKind::Indented)) => {
                in_code_block = true;
                code_block_lang = String::new();
                code_block_content.clear();
            }
            Event::End(TagEnd::CodeBlock) => {
                in_code_block = false;
                // Badge line
                let badge_text = if code_block_lang.is_empty() {
                    " code ".to_string()
                } else {
                    format!(" {} ", code_block_lang)
                };
                lines.push(vec![StyledSpan {
                    text: badge_text,
                    style: badge_style,
                }]);

                // Highlighted code lines
                let highlighted = hl.highlight_code(&code_block_content, &code_block_lang);
                for hl_line in highlighted {
                    let mut line: StyledLine = vec![StyledSpan {
                        text: "  ".to_string(),
                        style: Style::default(),
                    }];
                    line.extend(hl_line);
                    lines.push(line);
                }
                // Blank line after code block
                lines.push(Vec::new());
                code_block_content.clear();
                code_block_lang.clear();
            }

            Event::Start(Tag::Strong) => {
                in_strong = true;
                inline_style = inline_style.add_modifier(Modifier::BOLD);
            }
            Event::End(TagEnd::Strong) => {
                in_strong = false;
                inline_style = compute_inline_style(
                    in_heading.as_ref().map(|(_, s)| *s),
                    in_link,
                    in_strong,
                    in_emphasis,
                    h1_style,
                    h2_style,
                    h3_style,
                    link_style,
                );
            }

            Event::Start(Tag::Emphasis) => {
                in_emphasis = true;
                inline_style = inline_style.add_modifier(Modifier::ITALIC);
            }
            Event::End(TagEnd::Emphasis) => {
                in_emphasis = false;
                inline_style = compute_inline_style(
                    in_heading.as_ref().map(|(_, s)| *s),
                    in_link,
                    in_strong,
                    in_emphasis,
                    h1_style,
                    h2_style,
                    h3_style,
                    link_style,
                );
            }

            Event::Start(Tag::Link { .. }) => {
                in_link = true;
                inline_style = link_style;
            }
            Event::End(TagEnd::Link) => {
                in_link = false;
                inline_style = compute_inline_style(
                    in_heading.as_ref().map(|(_, s)| *s),
                    in_link,
                    in_strong,
                    in_emphasis,
                    h1_style,
                    h2_style,
                    h3_style,
                    link_style,
                );
            }

            Event::Start(Tag::Item) => {
                in_item = true;
                item_prefix_done = false;
                inline_style = Style::default();
            }
            Event::End(TagEnd::Item) => {
                in_item = false;
                item_prefix_done = false;
                if !current_line.is_empty() {
                    flush_line(&mut lines, &mut current_line);
                }
                inline_style = Style::default();
            }

            Event::Start(Tag::Paragraph) => {
                _in_paragraph = true;
            }
            Event::End(TagEnd::Paragraph) => {
                _in_paragraph = false;
                if !current_line.is_empty() {
                    flush_line(&mut lines, &mut current_line);
                }
                // Blank line after paragraph
                lines.push(Vec::new());
            }

            Event::Start(Tag::BlockQuote(_)) => {
                // blockquote marker will be handled by text within
            }
            Event::End(TagEnd::BlockQuote(_)) => {
                if !current_line.is_empty() {
                    flush_line(&mut lines, &mut current_line);
                }
            }

            Event::Start(Tag::List(_)) => {
                // list container start - no special handling needed
            }
            Event::End(TagEnd::List(_)) => {
                // blank line after list
                lines.push(Vec::new());
            }

            // ── Text ──────────────────────────────────────────────────────
            Event::Text(t) => {
                if in_code_block {
                    code_block_content.push_str(&t);
                } else {
                    // Add bullet prefix for list items
                    if in_item && !item_prefix_done {
                        item_prefix_done = true;
                        current_line.push(StyledSpan {
                            text: "  • ".to_string(),
                            style: bullet_style,
                        });
                    }
                    let text = t.to_string();
                    current_line.push(StyledSpan {
                        text,
                        style: inline_style,
                    });
                }
            }

            Event::Code(t) => {
                current_line.push(StyledSpan {
                    text: t.to_string(),
                    style: inline_code_style,
                });
            }

            Event::SoftBreak => {
                if !in_code_block {
                    // In a paragraph, soft break is just a space
                    current_line.push(StyledSpan {
                        text: " ".to_string(),
                        style: inline_style,
                    });
                }
            }

            Event::HardBreak => {
                if !in_code_block {
                    flush_line(&mut lines, &mut current_line);
                }
            }

            Event::Rule => {
                lines.push(vec![StyledSpan {
                    text: "─".repeat(60),
                    style: rule_style,
                }]);
            }

            _ => {}
        }
    }

    // Flush any remaining content
    if !current_line.is_empty() {
        lines.push(current_line);
    }

    (lines, toc)
}

#[allow(clippy::too_many_arguments)]
fn compute_inline_style(
    heading_style: Option<Style>,
    in_link: bool,
    in_strong: bool,
    in_emphasis: bool,
    h1_style: Style,
    h2_style: Style,
    h3_style: Style,
    link_style: Style,
) -> Style {
    let _ = (h1_style, h2_style, h3_style);
    if in_link {
        link_style
    } else if let Some(s) = heading_style {
        let mut style = s;
        if in_strong {
            style = style.add_modifier(Modifier::BOLD);
        }
        if in_emphasis {
            style = style.add_modifier(Modifier::ITALIC);
        }
        style
    } else {
        let mut style = Style::default();
        if in_strong {
            style = style.add_modifier(Modifier::BOLD);
        }
        if in_emphasis {
            style = style.add_modifier(Modifier::ITALIC);
        }
        style
    }
}
