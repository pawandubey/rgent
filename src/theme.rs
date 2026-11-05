use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{bail, Context, Result};
use handlebars::Handlebars;
use serde::Serialize;

use crate::highlight::{self, HighlightThemes};

const LAYOUT_EXTENSION: &str = "html";
const THEME_CONFIG_FILE: &str = "theme.toml";
const PARTIALS_DIR: &str = "partials";
const ASSETS_DIR: &str = "assets";

const STARTER_THEME: &[(&str, &str)] = &[
    ("theme.toml", include_str!("starter_theme/theme.toml")),
    ("index.html", include_str!("starter_theme/index.html")),
    ("post.html", include_str!("starter_theme/post.html")),
    ("page.html", include_str!("starter_theme/page.html")),
    (
        "partials/header.html",
        include_str!("starter_theme/partials/header.html"),
    ),
    (
        "partials/footer.html",
        include_str!("starter_theme/partials/footer.html"),
    ),
    (
        "assets/style.css",
        include_str!("starter_theme/assets/style.css"),
    ),
    (
        "assets/highlight.css",
        include_str!("starter_theme/assets/highlight.css"),
    ),
];

pub struct Theme {
    dir: PathBuf,
    registry: Handlebars<'static>,
    highlight: Option<HighlightThemes>,
}

impl Theme {
    pub fn load(dir: &Path) -> Result<Self> {
        if !dir.is_dir() {
            bail!(
                "Theme directory {:?} does not exist. Check `theme` in the config.",
                dir
            );
        }

        let mut registry = Handlebars::new();
        Self::register_templates(&mut registry, dir, "layout")?;

        let partials_dir = dir.join(PARTIALS_DIR);
        if partials_dir.is_dir() {
            Self::register_templates(&mut registry, &partials_dir, "partial")?;
        }

        let config_path = dir.join(THEME_CONFIG_FILE);
        let highlight = if config_path.is_file() {
            let contents = fs::read_to_string(&config_path)
                .with_context(|| format!("Failed to read theme config {:?}", config_path))?;
            highlight::parse_theme_config(&contents, &config_path)?
        } else {
            None
        };

        Ok(Theme {
            dir: dir.to_path_buf(),
            registry,
            highlight,
        })
    }

    pub fn highlight_themes(&self) -> Option<&HighlightThemes> {
        self.highlight.as_ref()
    }

    pub fn has_layout(&self, layout: &str) -> bool {
        self.registry.has_template(layout)
    }

    pub fn render<T: Serialize>(&self, layout: &str, data: &T) -> Result<String> {
        if !self.registry.has_template(layout) {
            bail!(
                "Theme {:?} has no layout named {:?}. Expected a file at {:?}",
                self.dir,
                layout,
                self.dir.join(layout).with_extension(LAYOUT_EXTENSION)
            );
        }

        self.registry.render(layout, data).with_context(|| {
            format!(
                "Failed to render layout {:?} of theme {:?}",
                layout, self.dir
            )
        })
    }

    pub fn copy_assets(&self, output_dir: &Path) -> Result<()> {
        let assets_dir = self.dir.join(ASSETS_DIR);
        if !assets_dir.is_dir() {
            return Ok(());
        }

        let output_assets_dir = output_dir.join(ASSETS_DIR);
        for entry in walkdir::WalkDir::new(&assets_dir) {
            let entry = entry
                .with_context(|| format!("Failed to read theme assets in {:?}", assets_dir))?;
            let relative_path = entry
                .path()
                .strip_prefix(&assets_dir)
                .with_context(|| format!("Asset {:?} is outside {:?}", entry.path(), assets_dir))?;
            let destination = output_assets_dir.join(relative_path);

            if entry.file_type().is_dir() {
                fs::create_dir_all(&destination).with_context(|| {
                    format!("Failed to create asset directory {:?}", destination)
                })?;
            } else {
                fs::copy(entry.path(), &destination).with_context(|| {
                    format!(
                        "Failed to copy asset {:?} to {:?}",
                        entry.path(),
                        destination
                    )
                })?;
            }
        }

        Ok(())
    }

    pub fn write_starter(dir: &Path) -> Result<()> {
        for (relative_path, contents) in STARTER_THEME {
            let path = dir.join(relative_path);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).with_context(|| {
                    format!("Failed to create starter theme directory {:?}", parent)
                })?;
            }
            fs::write(&path, contents)
                .with_context(|| format!("Failed to write starter theme file {:?}", path))?;
        }

        Ok(())
    }

    fn register_templates(registry: &mut Handlebars, dir: &Path, kind: &str) -> Result<()> {
        let entries = fs::read_dir(dir)
            .with_context(|| format!("Failed to read theme directory {:?}", dir))?;

        for entry in entries {
            let path = entry
                .with_context(|| format!("Failed to read entry in {:?}", dir))?
                .path();
            let is_template =
                path.is_file() && path.extension().unwrap_or_default() == LAYOUT_EXTENSION;
            if !is_template {
                continue;
            }

            let name = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .with_context(|| format!("Template file name {:?} is not valid UTF-8", path))?;

            if registry.has_template(name) {
                bail!(
                    "Theme {} {:?} clashes with another template named {:?}",
                    kind,
                    path,
                    name
                );
            }

            registry
                .register_template_file(name, &path)
                .with_context(|| format!("Failed to parse theme {} {:?}", kind, path))?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use assert_fs::{assert::PathAssert, fixture::PathChild};
    use predicates::prelude::*;
    use serde_json::json;

    use crate::theme::*;

    #[test]
    fn test_render_layout_with_partial() {
        let temp_dir = assert_fs::TempDir::new().unwrap();
        let theme_dir = temp_dir.child("theme");
        fs::create_dir_all(theme_dir.child("partials")).unwrap();
        fs::write(
            theme_dir.child("post.html"),
            "{{> header}}<main>{{{post.content}}}</main>",
        )
        .unwrap();
        fs::write(
            theme_dir.child("partials/header.html"),
            "<title>{{post.title}}</title>",
        )
        .unwrap();

        let theme = Theme::load(&theme_dir).expect("Failed to load theme");
        let html = theme
            .render(
                "post",
                &json!({"post": {"title": "Fish & <Chips>", "content": "<p>Hi</p>"}}),
            )
            .expect("Failed to render");

        assert_eq!(
            "<title>Fish &amp; &lt;Chips&gt;</title><main><p>Hi</p></main>",
            html
        );
        temp_dir.close().unwrap();
    }

    #[test]
    fn test_render_missing_layout_fails() {
        let temp_dir = assert_fs::TempDir::new().unwrap();
        let theme_dir = temp_dir.child("theme");
        fs::create_dir_all(&theme_dir).unwrap();
        fs::write(theme_dir.child("post.html"), "post").unwrap();

        let theme = Theme::load(&theme_dir).expect("Failed to load theme");
        let error = theme
            .render("gallery", &json!({}))
            .expect_err("Rendering a missing layout should fail");

        assert!(
            format!("{:#}", error).contains("no layout named \"gallery\""),
            "{:#}",
            error
        );
        temp_dir.close().unwrap();
    }

    #[test]
    fn test_load_missing_theme_fails() {
        let temp_dir = assert_fs::TempDir::new().unwrap();

        let result = Theme::load(&temp_dir.child("nope"));

        assert!(result.is_err());
        temp_dir.close().unwrap();
    }

    #[test]
    fn test_load_rejects_partial_clashing_with_layout() {
        let temp_dir = assert_fs::TempDir::new().unwrap();
        let theme_dir = temp_dir.child("theme");
        fs::create_dir_all(theme_dir.child("partials")).unwrap();
        fs::write(theme_dir.child("post.html"), "layout").unwrap();
        fs::write(theme_dir.child("partials/post.html"), "partial").unwrap();

        let result = Theme::load(&theme_dir);

        assert!(result.is_err());
        temp_dir.close().unwrap();
    }

    #[test]
    fn test_copy_assets() {
        let temp_dir = assert_fs::TempDir::new().unwrap();
        let theme_dir = temp_dir.child("theme");
        fs::create_dir_all(theme_dir.child("assets/css")).unwrap();
        fs::write(theme_dir.child("post.html"), "post").unwrap();
        fs::write(theme_dir.child("assets/css/site.css"), "body {}").unwrap();
        let output_dir = temp_dir.child("output");

        Theme::load(&theme_dir)
            .expect("Failed to load theme")
            .copy_assets(&output_dir)
            .expect("Failed to copy assets");

        output_dir
            .child("assets/css/site.css")
            .assert(predicate::path::is_file());
        assert_eq!(
            "body {}",
            fs::read_to_string(output_dir.child("assets/css/site.css")).unwrap()
        );
        temp_dir.close().unwrap();
    }

    #[test]
    fn test_starter_highlight_css_matches_giallo_output() {
        let temp_dir = assert_fs::TempDir::new().unwrap();
        let theme_dir = temp_dir.child("theme");
        Theme::write_starter(&theme_dir).expect("Failed to write starter theme");

        let theme = Theme::load(&theme_dir).expect("Failed to load starter theme");
        let themes = theme
            .highlight_themes()
            .expect("Starter theme should configure highlighting")
            .clone();
        let expected = crate::highlight::Highlighter::new(themes)
            .expect("Failed to create highlighter")
            .stylesheet()
            .expect("Failed to generate stylesheet");

        assert_eq!(
            expected,
            include_str!("starter_theme/assets/highlight.css"),
            "src/starter_theme/assets/highlight.css is out of date. Regenerate it with `cargo run -- highlight-css src/starter_theme > src/starter_theme/assets/highlight.css`"
        );
        temp_dir.close().unwrap();
    }

    #[test]
    fn test_load_reads_highlight_config() {
        let temp_dir = assert_fs::TempDir::new().unwrap();
        let theme_dir = temp_dir.child("theme");
        fs::create_dir_all(&theme_dir).unwrap();
        fs::write(theme_dir.child("post.html"), "post").unwrap();

        assert_eq!(
            None,
            Theme::load(&theme_dir).unwrap().highlight_themes().cloned()
        );

        fs::write(theme_dir.child("theme.toml"), "[highlight]\ntheme = 'nord'").unwrap();
        assert_eq!(
            Some(HighlightThemes::Single {
                theme: "nord".into()
            }),
            Theme::load(&theme_dir).unwrap().highlight_themes().cloned()
        );

        fs::write(theme_dir.child("theme.toml"), "[highlight]\nlight = 'nord'").unwrap();
        assert!(Theme::load(&theme_dir).is_err());
        temp_dir.close().unwrap();
    }

    #[test]
    fn test_starter_theme_loads_and_renders() {
        let temp_dir = assert_fs::TempDir::new().unwrap();
        let theme_dir = temp_dir.child("theme");

        Theme::write_starter(&theme_dir).expect("Failed to write starter theme");
        let theme = Theme::load(&theme_dir).expect("Failed to load starter theme");
        let data = json!({
            "config": {"site": {"name": "My Site", "baseurl": "https://example.com"}},
            "post": {"title": "Hello", "content": "<p>Body</p>", "tags": ["a"]},
            "nav_pages": [{"title": "About", "url": "/about/"}],
        });

        for layout in ["post", "page"] {
            let html = theme.render(layout, &data).expect("Failed to render");
            assert!(html.contains("<p>Body</p>"), "{}", html);
            assert!(html.contains("My Site"), "{}", html);
            assert!(
                html.contains("<li><a href=\"/about/\">About</a></li>"),
                "{}",
                html
            );
        }

        let index_data = json!({
            "config": {"site": {"name": "My Site"}},
            "index": {
                "posts": [
                    {"title": "First", "url": "/first/", "excerpt": "<p>One</p>", "has_more": true},
                    {"title": "Second", "url": "/second/", "excerpt": "<p>Two</p>", "has_more": false},
                ],
                "current_page": 1,
                "total_pages": 2,
                "next_page": "/page/2/",
                "pages": [
                    {"number": 1, "url": "/", "current": true},
                    {"number": 2, "url": "/page/2/", "current": false},
                ],
            },
        });
        let html = theme
            .render("index", &index_data)
            .expect("Failed to render index");
        assert!(html.contains("<title>My Site</title>"), "{}", html);
        assert!(html.contains("<a href=\"/first/\">First</a>"), "{}", html);
        assert!(html.contains("<p>One</p>"), "{}", html);
        assert!(html.contains("<p>Two</p>"), "{}", html);
        assert_eq!(1, html.matches("Read more").count(), "{}", html);
        assert!(
            html.contains("<a href=\"/first/\">Read more &rarr;</a>"),
            "{}",
            html
        );
        assert!(
            html.contains("<a href=\"/page/2/\">Older &rarr;</a>"),
            "{}",
            html
        );
        assert!(!html.contains("Newer"), "{}", html);

        theme_dir
            .child("assets/style.css")
            .assert(predicate::path::is_file());
        temp_dir.close().unwrap();
    }
}
