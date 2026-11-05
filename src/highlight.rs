use std::path::Path;

use anyhow::{bail, Context, Result};
use giallo::{
    parse_markdown_fence, HighlightOptions, HtmlRenderer, Registry, ThemeVariant, GIALLO_CSS,
    PLAIN_GRAMMAR_NAME,
};
use pulldown_cmark::{CodeBlockKind, CowStr, Event, Tag};
use serde::Deserialize;

const CSS_CLASS_PREFIX: &str = "hl-";
const PLAIN_ALIASES: &[&str] = &["text", "txt", "plaintext"];

#[derive(Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(untagged)]
pub enum HighlightThemes {
    Single { theme: String },
    Dual { light: String, dark: String },
}

impl HighlightThemes {
    fn variant(&self) -> ThemeVariant<&str> {
        match self {
            HighlightThemes::Single { theme } => ThemeVariant::Single(theme),
            HighlightThemes::Dual { light, dark } => ThemeVariant::Dual { light, dark },
        }
    }

    fn names(&self) -> Vec<&str> {
        match self {
            HighlightThemes::Single { theme } => vec![theme],
            HighlightThemes::Dual { light, dark } => vec![light, dark],
        }
    }
}

pub struct Highlighter {
    registry: Registry,
    themes: HighlightThemes,
    renderer: HtmlRenderer,
}

impl Highlighter {
    pub fn new(themes: HighlightThemes) -> Result<Self> {
        let mut registry =
            Registry::builtin().context("Failed to load syntax highlighting data")?;

        if !registry.contains_grammar(PLAIN_GRAMMAR_NAME) {
            let aliases: Vec<&str> = PLAIN_ALIASES
                .iter()
                .copied()
                .filter(|alias| !registry.contains_grammar(alias))
                .collect();
            registry
                .add_plain_grammar(&aliases)
                .context("Failed to register plain text grammar")?;
        }
        registry.link_grammars();

        for name in themes.names() {
            if !registry.contains_theme(name) {
                bail!(
                    "Unknown syntax highlighting theme {:?}. See https://github.com/getzola/giallo#themes for the available themes.",
                    name
                );
            }
        }

        Ok(Highlighter {
            registry,
            themes,
            renderer: HtmlRenderer {
                css_class_prefix: Some(CSS_CLASS_PREFIX.to_string()),
                ..Default::default()
            },
        })
    }

    pub fn stylesheet(&self) -> Result<String> {
        let css = match &self.themes {
            HighlightThemes::Single { theme } => self
                .registry
                .generate_css(theme, CSS_CLASS_PREFIX)
                .with_context(|| format!("Failed to generate CSS for theme {:?}", theme))?,
            HighlightThemes::Dual { light, dark } => {
                let (light_css, dark_css) = self
                    .registry
                    .generate_dual_css(light, dark, CSS_CLASS_PREFIX)
                    .with_context(|| {
                        format!(
                            "Failed to generate CSS for themes {:?} and {:?}",
                            light, dark
                        )
                    })?;
                format!(
                    "{}\n@media (prefers-color-scheme: dark) {{\n{}}}\n",
                    light_css,
                    Self::indent(&dark_css)
                )
            }
        };

        Ok(format!("{}\n{}", GIALLO_CSS, css))
    }

    pub fn highlight_events<'a>(
        &self,
        events: impl Iterator<Item = Event<'a>>,
        source: &Path,
    ) -> Result<Vec<Event<'a>>> {
        let mut output = Vec::new();
        let mut code_block: Option<(String, String)> = None;

        for event in events {
            match (event, code_block.as_mut()) {
                (Event::Start(Tag::CodeBlock(kind)), None) => {
                    let fence = match kind {
                        CodeBlockKind::Fenced(info) => info.to_string(),
                        CodeBlockKind::Indented => String::new(),
                    };
                    code_block = Some((fence, String::new()));
                }
                (Event::Text(text), Some((_, code))) => code.push_str(&text),
                (Event::End(Tag::CodeBlock(_)), Some(_)) => {
                    let (fence, code) = code_block.take().unwrap_or_default();
                    let html = self.highlight_block(&fence, &code, source)?;
                    output.push(Event::Html(CowStr::from(html)));
                }
                (event, _) => output.push(event),
            }
        }

        Ok(output)
    }

    fn highlight_block(&self, fence: &str, code: &str, source: &Path) -> Result<String> {
        let fence = parse_markdown_fence(fence);
        if !self.registry.contains_grammar(&fence.lang) {
            eprintln!(
                "Warning: unknown code block language {:?} in {:?}, rendering it as plain text",
                fence.lang, source
            );
        }

        let options =
            HighlightOptions::new(&fence.lang, self.themes.variant()).fallback_to_plain(true);
        let highlighted = self.registry.highlight(code, &options).with_context(|| {
            format!(
                "Failed to highlight {:?} code block in {:?}",
                fence.lang, source
            )
        })?;

        Ok(self.renderer.render(&highlighted, &fence.options))
    }

    fn indent(css: &str) -> String {
        css.lines()
            .map(|line| {
                if line.is_empty() {
                    "\n".to_string()
                } else {
                    format!("  {}\n", line)
                }
            })
            .collect()
    }
}

pub fn parse_theme_config(contents: &str, path: &Path) -> Result<Option<HighlightThemes>> {
    #[derive(Deserialize)]
    struct ThemeConfig {
        highlight: Option<HighlightThemes>,
    }

    let config: ThemeConfig = toml::from_str(contents).with_context(|| {
        format!(
            "Failed to parse {:?}. `[highlight]` needs either `theme = \"...\"` or both `light = \"...\"` and `dark = \"...\"`.",
            path
        )
    })?;
    Ok(config.highlight)
}

#[cfg(test)]
mod test {
    use std::sync::OnceLock;

    use pulldown_cmark::{html, Parser};

    use crate::highlight::*;

    fn dual() -> HighlightThemes {
        HighlightThemes::Dual {
            light: "github-light".into(),
            dark: "github-dark".into(),
        }
    }

    fn highlighter() -> &'static Highlighter {
        static HIGHLIGHTER: OnceLock<Highlighter> = OnceLock::new();
        HIGHLIGHTER.get_or_init(|| Highlighter::new(dual()).expect("Failed to create highlighter"))
    }

    fn render(markdown: &str) -> String {
        let events = highlighter()
            .highlight_events(Parser::new(markdown), Path::new("test.md"))
            .expect("Failed to highlight");
        let mut output = String::new();
        html::push_html(&mut output, events.into_iter());
        output
    }

    #[test]
    fn test_highlights_fenced_code_with_classes() {
        let html = render("Intro\n\n```rust\nfn main() {}\n```\n\nOutro");

        assert!(
            html.starts_with("<p>Intro</p>\n<pre class=\"giallo hl-l-code hl-d-code\""),
            "{}",
            html
        );
        assert!(html.contains("data-lang=\"rust\""), "{}", html);
        assert!(html.contains("class=\"hl-l-"), "{}", html);
        assert!(!html.contains("style="), "{}", html);
        assert!(html.contains("<p>Outro</p>"), "{}", html);
    }

    #[test]
    fn test_escapes_code_contents() {
        let html = render("```html\n<script>alert(1)</script>\n```");

        assert!(!html.contains("<script>"), "{}", html);
        assert!(html.contains("&lt;"), "{}", html);
    }

    #[test]
    fn test_unknown_and_missing_languages_render_as_plain() {
        for markdown in [
            "```nosuchlang\nsome code\n```",
            "```\nsome code\n```",
            "    some code\n",
        ] {
            let html = render(markdown);
            assert!(html.contains("<pre class=\"giallo"), "{}", html);
            assert!(html.contains("some code"), "{}", html);
        }
    }

    #[test]
    fn test_fence_options_are_applied() {
        let html = render("```rust,linenos\nlet a = 1;\nlet b = 2;\n```");

        assert!(html.contains("class=\"giallo-ln\""), "{}", html);
    }

    #[test]
    fn test_inline_code_is_untouched() {
        let html = render("Use `cargo test` here.");

        assert_eq!("<p>Use <code>cargo test</code> here.</p>\n", html);
    }

    #[test]
    fn test_dual_stylesheet_scopes_dark_theme_to_media_query() {
        let css = highlighter()
            .stylesheet()
            .expect("Failed to build stylesheet");

        assert!(css.contains(".giallo-l {"), "{}", css);
        assert!(css.contains(".hl-l-code {"), "{}", css);
        let (light, dark) = css
            .split_once("@media (prefers-color-scheme: dark) {")
            .expect("Missing dark media query");
        assert!(!light.contains(".hl-d-"), "{}", css);
        assert!(dark.contains("  .hl-d-code {"), "{}", css);
    }

    #[test]
    fn test_unknown_theme_fails() {
        let result = Highlighter::new(HighlightThemes::Single {
            theme: "no-such-theme".into(),
        });

        let error = result.err().expect("Unknown theme should fail");
        assert!(
            format!("{:#}", error).contains("Unknown syntax highlighting theme \"no-such-theme\""),
            "{:#}",
            error
        );
    }

    #[test]
    fn test_parse_theme_config() {
        let path = Path::new("theme.toml");

        assert_eq!(None, parse_theme_config("", path).unwrap());
        assert_eq!(
            Some(HighlightThemes::Single {
                theme: "nord".into()
            }),
            parse_theme_config("[highlight]\ntheme = 'nord'", path).unwrap()
        );
        assert_eq!(
            Some(dual()),
            parse_theme_config(
                "[highlight]\nlight = 'github-light'\ndark = 'github-dark'",
                path
            )
            .unwrap()
        );

        let error = parse_theme_config("[highlight]\nlight = 'github-light'", path).unwrap_err();
        assert!(
            format!("{:#}", error).contains("needs either `theme"),
            "{:#}",
            error
        );
    }
}
