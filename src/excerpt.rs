use std::ops::Range;

use pulldown_cmark::{escape::escape_html, Event, Options, Parser, Tag};

pub const MORE_MARKER: &str = "##more##";
pub const EXCERPT_LENGTH: usize = 255;

#[derive(Debug, PartialEq, Eq)]
pub struct MarkdownParts<'a> {
    pub excerpt: Option<&'a str>,
    pub body: String,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Summary {
    pub html: String,
    pub has_more: bool,
}

pub fn split_at_marker(markdown: &str, options: Options) -> MarkdownParts<'_> {
    match find_marker(markdown, options) {
        Some(range) => MarkdownParts {
            excerpt: Some(markdown[..range.start].trim_end()),
            body: format!("{}{}", &markdown[..range.start], &markdown[range.end..]),
        },
        None => MarkdownParts {
            excerpt: None,
            body: markdown.to_string(),
        },
    }
}

pub fn summarize(markdown: &str, options: Options, limit: usize) -> Summary {
    let mut text = String::new();
    let mut skipped_content = false;
    let mut skip_depth = 0usize;

    for event in Parser::new_ext(markdown, options) {
        match event {
            Event::Start(Tag::CodeBlock(_) | Tag::FootnoteDefinition(_) | Tag::Image(..)) => {
                skip_depth += 1;
                skipped_content = true;
            }
            Event::End(Tag::CodeBlock(_) | Tag::FootnoteDefinition(_) | Tag::Image(..)) => {
                skip_depth = skip_depth.saturating_sub(1);
            }
            _ if skip_depth > 0 => {}
            Event::Text(t) | Event::Code(t) => text.push_str(&t),
            Event::SoftBreak | Event::HardBreak => text.push(' '),
            Event::End(Tag::Emphasis | Tag::Strong | Tag::Strikethrough | Tag::Link(..)) => {}
            Event::End(_) => text.push(' '),
            Event::Html(_) => skipped_content = true,
            _ => {}
        }
    }

    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let (summary, truncated) = truncate_at_word(&text, limit);

    let mut html = String::new();
    if !summary.is_empty() {
        html.push_str("<p>");
        escape_html(&mut html, &summary).expect("Writing to a String cannot fail");
        if truncated {
            html.push('…');
        }
        html.push_str("</p>\n");
    }

    Summary {
        html,
        has_more: truncated || skipped_content,
    }
}

fn find_marker(markdown: &str, options: Options) -> Option<Range<usize>> {
    let mut paragraph: Option<Range<usize>> = None;
    let mut paragraph_text = String::new();

    for (event, range) in Parser::new_ext(markdown, options).into_offset_iter() {
        match event {
            Event::Start(Tag::Paragraph) => {
                paragraph = Some(range);
                paragraph_text.clear();
            }
            Event::Text(text) if paragraph.is_some() => paragraph_text.push_str(&text),
            Event::End(Tag::Paragraph) => {
                if paragraph_text.trim() == MORE_MARKER {
                    return paragraph;
                }
                paragraph = None;
            }
            _ if paragraph.is_some() => paragraph_text.push('\0'),
            _ => {}
        }
    }

    None
}

fn truncate_at_word(text: &str, limit: usize) -> (String, bool) {
    if text.chars().count() <= limit {
        return (text.to_string(), false);
    }

    let cut: String = text.chars().take(limit).collect();
    let next_char_starts_word = text.chars().nth(limit).is_some_and(char::is_whitespace);
    let summary = if next_char_starts_word {
        cut.as_str()
    } else {
        cut.rsplit_once(char::is_whitespace)
            .map(|(head, _)| head)
            .unwrap_or(cut.as_str())
    };

    let summary = summary.trim_end_matches(|c: char| c.is_whitespace() || ",;:".contains(c));
    (summary.to_string(), true)
}

#[cfg(test)]
mod test {
    use indoc::indoc;

    use crate::excerpt::*;

    #[test]
    fn test_split_at_marker() {
        let markdown = indoc! {"
            First paragraph.

            ##more##

            Rest of the post.
        "};

        let parts = split_at_marker(markdown, Options::empty());

        assert_eq!(Some("First paragraph."), parts.excerpt);
        assert_eq!("First paragraph.\n\n\nRest of the post.\n", parts.body);
    }

    #[test]
    fn test_split_without_marker_keeps_body() {
        let parts = split_at_marker("Just text.\n", Options::empty());

        assert_eq!(None, parts.excerpt);
        assert_eq!("Just text.\n", parts.body);
    }

    #[test]
    fn test_marker_inside_code_or_text_is_ignored() {
        let markdown = indoc! {"
            ```
            ##more##
            ```

            This mentions ##more## inline.

            `##more##`
        "};

        let parts = split_at_marker(markdown, Options::empty());

        assert_eq!(None, parts.excerpt);
        assert_eq!(markdown, parts.body);
    }

    #[test]
    fn test_marker_with_surrounding_whitespace() {
        let parts = split_at_marker("Intro\n\n   ##more##  \n\nBody\n", Options::empty());

        assert_eq!(Some("Intro"), parts.excerpt);
    }

    #[test]
    fn test_summarize_short_text_is_not_truncated() {
        let summary = summarize("Hello *world*.\n", Options::empty(), 255);

        assert_eq!(
            Summary {
                html: "<p>Hello world.</p>\n".into(),
                has_more: false
            },
            summary
        );
    }

    #[test]
    fn test_summarize_truncates_at_word_boundary() {
        let summary = summarize(
            "The quick brown fox jumps over the lazy dog.",
            Options::empty(),
            17,
        );

        assert_eq!("<p>The quick brown…</p>\n", summary.html);
        assert!(summary.has_more);
    }

    #[test]
    fn test_summarize_keeps_word_ending_exactly_at_limit() {
        let summary = summarize("The quick brown fox", Options::empty(), 15);

        assert_eq!("<p>The quick brown…</p>\n", summary.html);
    }

    #[test]
    fn test_summarize_flattens_blocks_and_skips_code() {
        let markdown = indoc! {"
            # Heading

            Some `inline` code and a [link](https://example.com).

            ```rust
            fn hidden() {}
            ```

            - item one
            - item two
        "};

        let summary = summarize(markdown, Options::empty(), 255);

        assert_eq!(
            "<p>Heading Some inline code and a link. item one item two</p>\n",
            summary.html
        );
        assert!(summary.has_more);
    }

    #[test]
    fn test_summarize_escapes_html() {
        let summary = summarize("Fish & \\<chips\\>", Options::empty(), 255);

        assert_eq!("<p>Fish &amp; &lt;chips&gt;</p>\n", summary.html);
    }

    #[test]
    fn test_summarize_skips_raw_html_and_images() {
        let summary = summarize(
            "Hello <b>there</b> ![alt text](a.png) friend",
            Options::empty(),
            255,
        );

        assert_eq!("<p>Hello there friend</p>\n", summary.html);
        assert!(summary.has_more);
    }

    #[test]
    fn test_summarize_counts_characters_not_bytes() {
        let summary = summarize("héllo wörld ünïcode", Options::empty(), 11);

        assert_eq!("<p>héllo wörld…</p>\n", summary.html);
    }

    #[test]
    fn test_summarize_empty_content() {
        let summary = summarize("", Options::empty(), 255);

        assert_eq!(
            Summary {
                html: String::new(),
                has_more: false
            },
            summary
        );
    }
}
