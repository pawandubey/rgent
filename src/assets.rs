use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Component, Path, PathBuf},
};

use anyhow::{bail, Context, Result};
use pulldown_cmark::{CowStr, Event, Tag};

const PUBLISHED_HIDDEN_FILES: &[&str] = &[".nojekyll"];

pub struct ContentAssets {
    output_dir: PathBuf,
    assets: HashMap<PathBuf, Asset>,
}

struct Asset {
    relative_path: PathBuf,
    url: String,
}

impl ContentAssets {
    pub fn new(source_dir: &Path, output_dir: &Path, files: Vec<PathBuf>) -> Result<Self> {
        let mut assets = HashMap::new();

        for file in files {
            let relative_path = file
                .strip_prefix(source_dir)
                .with_context(|| format!("Asset {:?} is not inside {:?}", file, source_dir))?
                .to_path_buf();
            let url = format!(
                "/{}",
                relative_path
                    .components()
                    .map(|component| component.as_os_str().to_string_lossy())
                    .collect::<Vec<_>>()
                    .join("/")
            );
            assets.insert(normalize(&file), Asset { relative_path, url });
        }

        Ok(ContentAssets {
            output_dir: output_dir.to_path_buf(),
            assets,
        })
    }

    pub fn is_asset(path: &Path) -> bool {
        path.extension().unwrap_or_default() != "md"
    }

    pub fn is_hidden(path: &Path) -> bool {
        path.file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with('.') && !PUBLISHED_HIDDEN_FILES.contains(&name))
    }

    pub fn rewrite_event<'a>(&self, event: Event<'a>, source: &Path) -> Event<'a> {
        match event {
            Event::Start(Tag::Image(kind, destination, title)) => Event::Start(Tag::Image(
                kind,
                self.resolve(destination, source, true),
                title,
            )),
            Event::End(Tag::Image(kind, destination, title)) => Event::End(Tag::Image(
                kind,
                self.resolve(destination, source, false),
                title,
            )),
            Event::Start(Tag::Link(kind, destination, title)) => Event::Start(Tag::Link(
                kind,
                self.resolve(destination, source, false),
                title,
            )),
            Event::End(Tag::Link(kind, destination, title)) => Event::End(Tag::Link(
                kind,
                self.resolve(destination, source, false),
                title,
            )),
            event => event,
        }
    }

    pub fn copy(&self, generated: &HashSet<PathBuf>) -> Result<Vec<PathBuf>> {
        let mut assets: Vec<(&PathBuf, &Asset)> = self.assets.iter().collect();
        assets.sort_by(|a, b| a.0.cmp(b.0));

        let mut copied = Vec::with_capacity(assets.len());
        for (source, asset) in assets {
            let destination = self.output_dir.join(&asset.relative_path);
            if generated.contains(&destination) {
                bail!(
                    "Content file {:?} would overwrite {:?}, which rgent generates. Move or rename it.",
                    source,
                    destination
                );
            }

            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("Failed to create asset directory {:?}", parent))?;
            }
            fs::copy(source, &destination).with_context(|| {
                format!("Failed to copy asset {:?} to {:?}", source, destination)
            })?;
            copied.push(destination);
        }

        Ok(copied)
    }

    pub fn resolve_image(&self, destination: &str, source: &Path) -> String {
        match self.lookup(destination, source) {
            Resolution::Asset(url) => url,
            Resolution::Unchanged => destination.to_string(),
            Resolution::Missing(path) => {
                Self::warn_missing_image(path, source);
                destination.to_string()
            }
        }
    }

    fn resolve<'a>(&self, destination: CowStr<'a>, source: &Path, warn: bool) -> CowStr<'a> {
        match self.lookup(&destination, source) {
            Resolution::Asset(url) => CowStr::from(url),
            Resolution::Unchanged => destination,
            Resolution::Missing(path) => {
                if warn {
                    Self::warn_missing_image(path, source);
                }
                destination
            }
        }
    }

    fn lookup<'d>(&self, destination: &'d str, source: &Path) -> Resolution<'d> {
        if destination.is_empty()
            || destination.starts_with('/')
            || destination.starts_with('#')
            || has_scheme(destination)
        {
            return Resolution::Unchanged;
        }

        let split_at = destination.find(['?', '#']).unwrap_or(destination.len());
        let (path_part, suffix) = destination.split_at(split_at);
        let target = normalize(&source.parent().unwrap_or(Path::new("")).join(path_part));

        match self.assets.get(&target) {
            Some(asset) => Resolution::Asset(format!("{}{}", asset.url, suffix)),
            None => Resolution::Missing(path_part),
        }
    }

    fn warn_missing_image(path: &str, source: &Path) {
        eprintln!(
            "Warning: image {:?} in {:?} was not found in the content directory",
            path, source
        );
    }
}

enum Resolution<'d> {
    Asset(String),
    Unchanged,
    Missing(&'d str),
}

fn has_scheme(destination: &str) -> bool {
    match destination.find(':') {
        Some(colon) => {
            let scheme = &destination[..colon];
            !scheme.is_empty()
                && scheme
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "+-.".contains(c))
        }
        None => false,
    }
}

fn normalize(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            other => normalized.push(other),
        }
    }
    normalized
}

#[cfg(test)]
mod test {
    use assert_fs::{assert::PathAssert, fixture::PathChild};
    use predicates::prelude::*;
    use pulldown_cmark::{html, Parser};

    use crate::assets::*;

    fn assets(source: &Path, files: &[&str]) -> ContentAssets {
        ContentAssets::new(
            source,
            Path::new("/site/output"),
            files.iter().map(|f| source.join(f)).collect(),
        )
        .unwrap()
    }

    fn render(assets: &ContentAssets, source_file: &Path, markdown: &str) -> String {
        let mut output = String::new();
        html::push_html(
            &mut output,
            Parser::new(markdown).map(|event| assets.rewrite_event(event, source_file)),
        );
        output
    }

    #[test]
    fn test_rewrites_relative_references_to_assets() {
        let source = Path::new("/site/content");
        let assets = assets(
            source,
            &["notes/diagram.png", "files/report.pdf", "logo.svg"],
        );
        let post = source.join("notes/tip.md");

        assert_eq!(
            "<p><img src=\"/notes/diagram.png\" alt=\"Diagram\" /></p>\n",
            render(&assets, &post, "![Diagram](diagram.png)")
        );
        assert_eq!(
            "<p><img src=\"/notes/diagram.png\" alt=\"\" /></p>\n",
            render(&assets, &post, "![](./diagram.png)")
        );
        assert_eq!(
            "<p><a href=\"/files/report.pdf#page=2\">report</a></p>\n",
            render(&assets, &post, "[report](../files/report.pdf#page=2)")
        );
        assert_eq!(
            "<p><img src=\"/logo.svg?v=2\" alt=\"logo\" /></p>\n",
            render(&assets, &post, "![logo](../logo.svg?v=2)")
        );
    }

    #[test]
    fn test_leaves_other_destinations_untouched() {
        let source = Path::new("/site/content");
        let assets = assets(source, &["diagram.png"]);
        let post = source.join("post.md");

        for markdown in [
            "[abs](/diagram.png)",
            "[ext](https://example.com/diagram.png)",
            "[mail](mailto:me@example.com)",
            "[frag](#section)",
            "[page](../other-post/)",
            "![missing](missing.png)",
        ] {
            let html = render(&assets, &post, markdown);
            let destination = markdown
                .split_once('(')
                .and_then(|(_, rest)| rest.strip_suffix(')'))
                .unwrap();
            assert!(
                html.contains(&format!("\"{}\"", destination)),
                "{} -> {}",
                markdown,
                html
            );
        }
    }

    #[test]
    fn test_resolve_image() {
        let source = Path::new("/site/content");
        let assets = assets(source, &["posts/cover.jpg"]);
        let post = source.join("posts/hello.md");

        assert_eq!("/posts/cover.jpg", assets.resolve_image("cover.jpg", &post));
        assert_eq!(
            "/posts/cover.jpg",
            assets.resolve_image("./cover.jpg", &post)
        );
        assert_eq!(
            "/static/x.png",
            assets.resolve_image("/static/x.png", &post)
        );
        assert_eq!(
            "https://cdn.example.com/x.png",
            assets.resolve_image("https://cdn.example.com/x.png", &post)
        );
        assert_eq!("missing.jpg", assets.resolve_image("missing.jpg", &post));
    }

    #[test]
    fn test_has_scheme() {
        assert!(has_scheme("https://example.com"));
        assert!(has_scheme("mailto:me@example.com"));
        assert!(has_scheme("data:image/png;base64,AAA"));
        assert!(!has_scheme("diagram.png"));
        assert!(!has_scheme("dir/file:name.png"));
        assert!(!has_scheme(":odd"));
    }

    #[test]
    fn test_copy_preserves_relative_paths() {
        let temp_dir = assert_fs::TempDir::new().unwrap();
        let source = temp_dir.child("content");
        let output = temp_dir.child("output");
        fs::create_dir_all(source.child("notes")).unwrap();
        fs::write(source.child("notes/diagram.png"), "png").unwrap();
        fs::write(source.child("logo.svg"), "svg").unwrap();

        let assets = ContentAssets::new(
            &source,
            &output,
            vec![
                source.child("notes/diagram.png").to_path_buf(),
                source.child("logo.svg").to_path_buf(),
            ],
        )
        .unwrap();
        let copied = assets.copy(&HashSet::new()).expect("Failed to copy");

        assert_eq!(2, copied.len());
        output
            .child("notes/diagram.png")
            .assert(predicate::str::diff("png"));
        output.child("logo.svg").assert(predicate::str::diff("svg"));
        temp_dir.close().unwrap();
    }

    #[test]
    fn test_copy_refuses_to_overwrite_generated_files() {
        let temp_dir = assert_fs::TempDir::new().unwrap();
        let source = temp_dir.child("content");
        let output = temp_dir.child("output");
        fs::create_dir_all(&source).unwrap();
        fs::write(source.child("feed.xml"), "mine").unwrap();

        let assets = ContentAssets::new(
            &source,
            &output,
            vec![source.child("feed.xml").to_path_buf()],
        )
        .unwrap();
        let generated: HashSet<PathBuf> = [output.child("feed.xml").to_path_buf()].into();

        let error = assets.copy(&generated).unwrap_err();
        assert!(error.to_string().contains("would overwrite"), "{}", error);
        temp_dir.close().unwrap();
    }

    #[test]
    fn test_is_asset_and_is_hidden() {
        assert!(ContentAssets::is_asset(Path::new("a/b.png")));
        assert!(ContentAssets::is_asset(Path::new("a/README")));
        assert!(!ContentAssets::is_asset(Path::new("a/post.md")));
        assert!(ContentAssets::is_hidden(Path::new("a/.DS_Store")));
        assert!(ContentAssets::is_hidden(Path::new(".git")));
        assert!(ContentAssets::is_hidden(Path::new("a/.nojekyll.bak")));
        assert!(!ContentAssets::is_hidden(Path::new("content/.nojekyll")));
        assert!(!ContentAssets::is_hidden(Path::new("a/b.png")));
    }
}
