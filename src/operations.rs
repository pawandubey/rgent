use std::{
    fmt::Debug,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{bail, Context, Result};
use chrono::NaiveDate;

use crate::{
    config::Config,
    dates::DatePattern,
    excerpt::{self, EXCERPT_LENGTH},
    feed::{self, FeedChannel, FeedItem, FEED_FILE, FEED_ITEMS},
    highlight::Highlighter,
    pagination::{self, IndexPage},
    preview,
    taxonomy::{self, TagLink, TAGS_DIR},
    theme::Theme,
};

use itertools::Itertools;

const CONFIG_FILE_NAME: &str = "rgent.toml";
const CONTENT_AND_DRAFTS_DIR: &str = "content/drafts";
const OUTPUT_DIR: &str = "output";
const THEMES_DIR: &str = "themes";
const DEFAULT_LAYOUT: &str = "post";
const INDEX_LAYOUT: &str = "index";
const PAGE_LAYOUT: &str = "page";
const NAV_TAG: &str = "nav";
const FEED_LAYOUT: &str = "feed";
const TAG_INDEX_LAYOUT: &str = "tagindex";

pub struct Operations {}

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct Frontmatter {
    title: String,
    author: Option<String>,
    layout: Option<String>,
    date: Option<String>,
    #[serde(default)]
    tags: Vec<String>,
    slug: Option<String>,
}

#[derive(Serialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum ContentKind {
    Post,
    Page,
}

impl ContentKind {
    fn from_layout(layout: &str) -> Self {
        if layout == PAGE_LAYOUT {
            ContentKind::Page
        } else {
            ContentKind::Post
        }
    }
}

#[derive(Serialize)]
struct LayoutData<'a> {
    config: &'a Config,
    post: &'a Entry,
    nav_pages: &'a [&'a Entry],
}

#[derive(Serialize)]
struct IndexLayoutData<'a> {
    config: &'a Config,
    index: IndexPage<'a, &'a Entry>,
    nav_pages: &'a [&'a Entry],
}

#[derive(Serialize)]
struct TagIndexLayoutData<'a> {
    config: &'a Config,
    tag: &'a TagLink,
    index: IndexPage<'a, &'a Entry>,
    tags: &'a [TagLink],
    nav_pages: &'a [&'a Entry],
}

#[derive(Serialize)]
struct FeedLayoutData<'a> {
    config: &'a Config,
    feed: FeedData<'a>,
    nav_pages: &'a [&'a Entry],
}

#[derive(Serialize)]
struct FeedData<'a> {
    url: &'a str,
    posts: &'a [&'a Entry],
}

#[derive(Serialize)]
struct Entry {
    kind: ContentKind,
    title: String,
    author: String,
    date: Option<String>,
    date_iso: Option<String>,
    tags: Vec<String>,
    tag_links: Vec<TagLink>,
    slug: String,
    url: String,
    permalink: String,
    content: String,
    excerpt: String,
    has_more: bool,
    #[serde(skip)]
    published_on: Option<NaiveDate>,
    #[serde(skip)]
    layout: String,
    #[serde(skip)]
    source_path: PathBuf,
    #[serde(skip)]
    output_path: PathBuf,
}

impl Operations {
    pub fn new(path: &PathBuf) -> Result<()> {
        // TODO: Canonicalize path for more consistent path representation in logs?
        // Current behavior works correctly with symlinks and relative paths.
        fs::create_dir_all(path).context("Failed to create site directory")?;

        fs::create_dir_all(path.join(CONTENT_AND_DRAFTS_DIR))
            .context("Failed to create content directory")?;
        fs::create_dir(path.join(OUTPUT_DIR)).context("Failed to create output directory")?;
        fs::create_dir(path.join(THEMES_DIR)).context("Failed to create themes directory")?;

        let config = Config::default();
        Theme::write_starter(&path.join(THEMES_DIR).join(&config.theme))
            .context("Failed to create starter theme")?;

        let path_to_config = path.join(CONFIG_FILE_NAME);
        let serialized_config =
            toml::to_string_pretty(&config).context("Failed to serialize default config")?;
        fs::write(&path_to_config, serialized_config).context("Failed to write to config file")?;

        println!("Initialized new site config at {:?}", path_to_config);
        Ok(())
    }

    pub fn preview(root: &Path, port: Option<u16>) -> Result<()> {
        Self::publish(root, false)?;

        let config = Self::load_config(root)?;
        let port = port.unwrap_or(config.preview.port);
        let output_dir = root.join(&config.output);

        preview::serve(output_dir, port)
    }

    pub fn publish(root: &Path, _rebuild: bool) -> Result<(), anyhow::Error> {
        let config = Self::load_config(root)?;

        let mut md_options = pulldown_cmark::Options::empty();
        md_options.insert(pulldown_cmark::Options::ENABLE_FOOTNOTES);
        md_options.insert(pulldown_cmark::Options::ENABLE_SMART_PUNCTUATION);
        md_options.insert(pulldown_cmark::Options::ENABLE_STRIKETHROUGH);
        md_options.insert(pulldown_cmark::Options::ENABLE_TABLES);

        let source_dir = root.join(&config.source);
        let output_dir = root.join(&config.output);
        let theme = Theme::load(&root.join(THEMES_DIR).join(&config.theme))?;
        let highlighter = theme
            .highlight_themes()
            .cloned()
            .map(Highlighter::new)
            .transpose()?;
        let input_date =
            DatePattern::new(&config.input_date_format).context("Invalid `inputdate` in config")?;
        let output_date = DatePattern::new(&config.output_date_format)
            .context("Invalid `outputdate` in config")?;
        let excluded_dirs: Vec<PathBuf> = config
            .exclude
            .iter()
            .map(|dir| source_dir.join(dir))
            .collect();
        let mut entries = Vec::new();

        let walker = walkdir::WalkDir::new(&source_dir)
            .into_iter()
            .filter_entry(|entry| {
                !excluded_dirs
                    .iter()
                    .any(|dir| entry.path().starts_with(dir))
            });

        for entry in walker {
            let dir_entry = entry.context("Failed to get result from walkdir entry")?;
            let extension = dir_entry.path().extension().unwrap_or_default();

            if dir_entry.file_type().is_file() && extension == "md" {
                let raw_content = fs::read_to_string(dir_entry.path())
                    .with_context(|| format!("Failed to read file: {:?}", dir_entry.file_name()))?;

                let (header, markdown) = raw_content.split_once("#####").with_context(|| {
                    format!(
                        "Failed to parse frontmatter and content from file {:?}",
                        dir_entry.path()
                    )
                })?;

                let frontmatter: Frontmatter = toml::from_str(header).with_context(|| {
                    format!("Failed to parse frontmatter for {:?}", dir_entry.path())
                })?;

                let parts = excerpt::split_at_marker(markdown, md_options);
                let html = Self::render_markdown(
                    &parts.body,
                    md_options,
                    highlighter.as_ref(),
                    dir_entry.path(),
                )?;
                let (excerpt_html, has_more) = match parts.excerpt {
                    Some(excerpt_markdown) => (
                        Self::render_markdown(
                            excerpt_markdown,
                            md_options,
                            highlighter.as_ref(),
                            dir_entry.path(),
                        )?,
                        true,
                    ),
                    None => {
                        let summary = excerpt::summarize(&parts.body, md_options, EXCERPT_LENGTH);
                        (summary.html, summary.has_more)
                    }
                };

                // let title = &frontmatter.title;

                let slug = match &frontmatter.slug {
                    Some(s) => s,
                    None => &frontmatter
                        .title
                        .trim()
                        .to_lowercase()
                        .split_whitespace()
                        .join("-"),
                };

                // let dir_file_stem = dir_entry.path().file_stem().with_context(|| {
                //     format!("Failed to get file stem from path: {:?}", dir_entry.path())
                // })?;

                let indexed_output_file_path = dir_entry
                    .path()
                    .with_file_name(slug.trim())
                    .join("index.html");
                let output_path = Self::rebase_path(
                    &indexed_output_file_path,
                    &source_dir,
                    &output_dir,
                    Some("html"),
                )?;

                let indexed_base_dir = &output_path.parent().with_context(|| {
                    format!(
                        "Failed to extract base directory from path: {:?}",
                        output_path
                    )
                })?;

                fs::create_dir_all(indexed_base_dir).with_context(|| {
                    format!(
                        "Failed to create index base directory: {:?}",
                        indexed_output_file_path.parent()
                    )
                })?;

                let url = Self::url_for_dir(indexed_base_dir, &output_dir)?;
                let layout = frontmatter
                    .layout
                    .clone()
                    .unwrap_or_else(|| DEFAULT_LAYOUT.to_string());
                let kind = ContentKind::from_layout(&layout);

                let published_on = match (&frontmatter.date, kind) {
                    (Some(date), _) => Some(input_date.parse(date).with_context(|| {
                        format!(
                            "Invalid `date` in {:?}. Dates must match `inputdate` from the config.",
                            dir_entry.path()
                        )
                    })?),
                    (None, ContentKind::Page) => None,
                    (None, ContentKind::Post) => bail!(
                        "Post {:?} has no `date`. Add one, or set `layout = 'page'` if it is a page.",
                        dir_entry.path()
                    ),
                };

                let entry = Entry {
                    kind,
                    title: frontmatter.title.clone(),
                    author: frontmatter
                        .author
                        .clone()
                        .unwrap_or_else(|| config.site.author.clone()),
                    date: published_on.map(|date| output_date.format(date)),
                    date_iso: published_on.map(|date| date.format("%Y-%m-%d").to_string()),
                    tags: frontmatter.tags.clone(),
                    tag_links: frontmatter
                        .tags
                        .iter()
                        .map(|tag| TagLink::new(tag))
                        .collect(),
                    slug: slug.trim().to_string(),
                    permalink: format!("{}{}", config.site.base_url.trim_end_matches('/'), url),
                    url,
                    content: html,
                    excerpt: excerpt_html,
                    has_more,
                    published_on,
                    layout,
                    source_path: dir_entry.path().to_path_buf(),
                    output_path,
                };

                /*

                After, we will take the slug from the page/post frontmatter.
                */
                // TODO: Also write posts to dir/index.html
                // TODO: Copy over all assets

                entries.push(entry);
            }
        }

        let mut nav_pages: Vec<&Entry> = entries
            .iter()
            .filter(|entry| {
                entry.kind == ContentKind::Page && entry.tags.iter().any(|tag| tag == NAV_TAG)
            })
            .collect();
        nav_pages.sort_by(|a, b| a.title.cmp(&b.title));

        for entry in &entries {
            Self::write_entry(&theme, &config, entry, &nav_pages)?;
        }

        let mut posts: Vec<&Entry> = entries
            .iter()
            .filter(|entry| entry.kind == ContentKind::Post)
            .collect();
        posts.sort_by(|a, b| {
            b.published_on
                .cmp(&a.published_on)
                .then_with(|| a.title.cmp(&b.title))
        });

        Self::write_index_pages(&theme, &config, &posts, &nav_pages, &output_dir)?;
        if config.render_tags {
            Self::write_tag_pages(&theme, &config, &posts, &nav_pages, &output_dir)?;
        }
        Self::write_feed(&theme, &config, &posts, &nav_pages, &output_dir)?;
        theme.copy_assets(&output_dir)?;
        Ok(())
    }

    fn render_markdown(
        markdown: &str,
        options: pulldown_cmark::Options,
        highlighter: Option<&Highlighter>,
        source: &Path,
    ) -> Result<String> {
        let parser = pulldown_cmark::Parser::new_ext(markdown, options);
        let mut html = String::with_capacity(markdown.len());

        match highlighter {
            Some(highlighter) => pulldown_cmark::html::push_html(
                &mut html,
                highlighter.highlight_events(parser, source)?.into_iter(),
            ),
            None => pulldown_cmark::html::push_html(&mut html, parser),
        }

        Ok(html)
    }

    fn write_entry(
        theme: &Theme,
        config: &Config,
        entry: &Entry,
        nav_pages: &[&Entry],
    ) -> Result<()> {
        let layout_data = LayoutData {
            config,
            post: entry,
            nav_pages,
        };
        let rendered = theme
            .render(&entry.layout, &layout_data)
            .with_context(|| format!("Failed to render {:?}", entry.source_path))?;

        fs::write(&entry.output_path, rendered).with_context(|| {
            format!(
                "Failed to write generated HTML for: {:?} to {:?}",
                entry.source_path, entry.output_path
            )
        })
    }

    fn write_index_pages(
        theme: &Theme,
        config: &Config,
        posts: &[&Entry],
        nav_pages: &[&Entry],
        output_dir: &Path,
    ) -> Result<()> {
        Self::write_paginated(
            theme,
            INDEX_LAYOUT,
            posts,
            config.posts_per_page,
            "/",
            output_dir,
            |index| IndexLayoutData {
                config,
                index,
                nav_pages,
            },
        )
    }

    fn write_tag_pages(
        theme: &Theme,
        config: &Config,
        posts: &[&Entry],
        nav_pages: &[&Entry],
        output_dir: &Path,
    ) -> Result<()> {
        if !theme.has_layout(TAG_INDEX_LAYOUT) {
            bail!(
                "`rendertags` is enabled, but the theme has no `{}.html` layout for tag pages",
                TAG_INDEX_LAYOUT
            );
        }

        let groups = taxonomy::group_by_tag(posts, |post| &post.tags)?;
        let all_tags: Vec<TagLink> = groups.iter().map(|group| group.tag.clone()).collect();

        for group in &groups {
            Self::write_paginated(
                theme,
                TAG_INDEX_LAYOUT,
                &group.posts,
                config.posts_per_page,
                &group.tag.url,
                &output_dir.join(TAGS_DIR).join(&group.tag.slug),
                |index| TagIndexLayoutData {
                    config,
                    tag: &group.tag,
                    index,
                    tags: &all_tags,
                    nav_pages,
                },
            )
            .with_context(|| format!("Failed to write pages for tag {:?}", group.tag.name))?;
        }

        Ok(())
    }

    fn write_paginated<'a, D: Serialize>(
        theme: &Theme,
        layout: &str,
        posts: &'a [&'a Entry],
        per_page: u64,
        base_url: &str,
        dir: &Path,
        layout_data: impl Fn(IndexPage<'a, &'a Entry>) -> D,
    ) -> Result<()> {
        for index in pagination::paginate(posts, per_page, base_url) {
            let output_path = pagination::page_output_path(dir, index.current_page);
            let current_page = index.current_page;

            if let Some(parent) = output_path.parent() {
                fs::create_dir_all(parent).with_context(|| {
                    format!("Failed to create index page directory {:?}", parent)
                })?;
            }

            let rendered = theme
                .render(layout, &layout_data(index))
                .with_context(|| format!("Failed to render {} page {}", layout, current_page))?;

            fs::write(&output_path, rendered).with_context(|| {
                format!(
                    "Failed to write {} page {} to {:?}",
                    layout, current_page, output_path
                )
            })?;
        }

        Ok(())
    }

    fn write_feed(
        theme: &Theme,
        config: &Config,
        posts: &[&Entry],
        nav_pages: &[&Entry],
        output_dir: &Path,
    ) -> Result<()> {
        let base_url = config.site.base_url.trim_end_matches('/');
        let site_url = format!("{}/", base_url);
        let feed_url = format!("{}/{}", base_url, FEED_FILE);
        let latest = &posts[..posts.len().min(FEED_ITEMS)];

        let xml = if theme.has_layout(FEED_LAYOUT) {
            theme.render(
                FEED_LAYOUT,
                &FeedLayoutData {
                    config,
                    feed: FeedData {
                        url: &feed_url,
                        posts: latest,
                    },
                    nav_pages,
                },
            )?
        } else {
            let items: Vec<FeedItem> = latest
                .iter()
                .filter_map(|post| {
                    post.published_on.map(|published_on| FeedItem {
                        title: &post.title,
                        link: &post.permalink,
                        description: &post.excerpt,
                        content: &post.content,
                        published_on,
                        tags: &post.tags,
                    })
                })
                .collect();
            let channel = FeedChannel {
                title: &config.site.name,
                link: &site_url,
                description: config.site.tagline.as_deref().unwrap_or(&config.site.name),
                feed_url: &feed_url,
            };
            feed::render_rss(&channel, &items)?
        };

        let output_path = output_dir.join(FEED_FILE);
        fs::write(&output_path, xml)
            .with_context(|| format!("Failed to write feed to {:?}", output_path))
    }

    fn url_for_dir(dir: &Path, output_dir: &Path) -> Result<String> {
        let relative_dir = dir.strip_prefix(output_dir).with_context(|| {
            format!("Output directory {:?} is not inside {:?}", dir, output_dir)
        })?;
        let segments = relative_dir
            .components()
            .map(|component| component.as_os_str().to_string_lossy())
            .join("/");

        if segments.is_empty() {
            Ok("/".into())
        } else {
            Ok(format!("/{}/", segments))
        }
    }

    pub fn highlight_css(theme_dir: &Path) -> Result<()> {
        let theme = Theme::load(theme_dir)?;
        let themes = theme.highlight_themes().with_context(|| {
            format!(
                "Theme {:?} has no `[highlight]` section in its theme.toml",
                theme_dir
            )
        })?;

        print!("{}", Highlighter::new(themes.clone())?.stylesheet()?);
        Ok(())
    }

    fn load_config(root: &Path) -> Result<Config> {
        let config_path = root.join(CONFIG_FILE_NAME);
        let config_str = fs::read_to_string(&config_path).with_context(|| {
            format!(
                "Failed to read config file at {:?}. Are we in the right directory?",
                config_path
            )
        })?;
        toml::from_str(&config_str).with_context(|| {
            format!(
                "Failed to parse config {:?}. Is it valid TOML?",
                config_path
            )
        })
    }

    /**
    Takes the path for a file from the "source" directory, and returns a corresponding
    path relative to the "output" directory for it.

    E.g. if the source is /site/content, and output is at /site/output,
    then /site/content/foo/bar.xyz will be returned as /site/output/foo/bar.xyz

    If rewrite_ext is provided, it is used as the extension.
     */
    fn rebase_path<P, F, T>(path: P, from: F, to: T, rewrite_ext: Option<&str>) -> Result<PathBuf>
    where
        P: AsRef<Path> + Debug,
        F: AsRef<Path> + Debug,
        T: AsRef<Path>,
    {
        let relative_path = pathdiff::diff_paths(&path, &from)
            .with_context(|| format!("Failed to diff path: {:?} from base: {:?}", path, from))?;
        let output_path = to.as_ref().join(relative_path);

        match rewrite_ext {
            Some(ext) => Ok(output_path.with_extension(ext)),
            None => Ok(output_path),
        }
    }
}

#[cfg(test)]
mod test {
    use assert_fs::{assert::PathAssert, fixture::PathChild};
    use indoc::indoc;
    use predicates::prelude::*;

    use crate::operations::*;

    #[test]
    fn test_scaffold_new_site() {
        let temp_dir = assert_fs::TempDir::new().unwrap();
        let target_dir = temp_dir.child("rgent_test");
        Operations::new(&target_dir.to_path_buf()).expect("Failed new operation");

        target_dir
            .child("rgent.toml")
            .assert(predicate::path::exists().and(predicate::path::is_file()));

        target_dir
            .child("content/drafts")
            .assert(predicate::path::exists().and(predicate::path::is_dir()));

        target_dir
            .child("output")
            .assert(predicate::path::exists().and(predicate::path::is_dir()));

        target_dir
            .child("themes/")
            .assert(predicate::path::exists().and(predicate::path::is_dir()));

        target_dir
            .child("themes/wells/post.html")
            .assert(predicate::path::is_file());

        let contents = fs::read_to_string(target_dir.child("rgent.toml"))
            .expect("Failed to read test config file");
        assert_eq!(new_config().trim(), contents.trim());
    }

    #[test]
    fn test_scaffold_with_relative_path_components() {
        let temp_dir = assert_fs::TempDir::new().unwrap();

        // Create a directory structure with a nested directory
        let nested_dir = temp_dir.child("nested");
        fs::create_dir(&nested_dir).expect("Failed to create nested directory");

        // Create a path with relative components (parent directory reference)
        let path_with_relative = nested_dir.join("..").join("rgent_relative_test");
        Operations::new(&path_with_relative).expect("Failed new operation with relative path");

        // The site should be created at the resolved path (same level as nested, not inside it)
        let expected_dir = temp_dir.child("rgent_relative_test");
        expected_dir.assert(predicate::path::exists().and(predicate::path::is_dir()));

        // Verify all the site directories were created at the correct location
        expected_dir
            .child("rgent.toml")
            .assert(predicate::path::exists().and(predicate::path::is_file()));

        expected_dir
            .child("content/drafts")
            .assert(predicate::path::exists().and(predicate::path::is_dir()));

        expected_dir
            .child("output")
            .assert(predicate::path::exists().and(predicate::path::is_dir()));

        expected_dir
            .child("themes")
            .assert(predicate::path::exists().and(predicate::path::is_dir()));

        // The config file content should be correct
        let contents = fs::read_to_string(expected_dir.child("rgent.toml"))
            .expect("Failed to read test config file");
        assert_eq!(new_config().trim(), contents.trim());
    }

    #[test]
    fn test_scaffold_with_symlink_path() {
        // Skip this test on platforms where symlinks aren't well supported
        if cfg!(windows) {
            return;
        }

        let temp_dir = assert_fs::TempDir::new().unwrap();

        // Create a real directory for the site
        let real_dir = temp_dir.child("real_site_dir");
        fs::create_dir(&real_dir).expect("Failed to create real directory");

        // Create a symlink to that directory
        let symlink_dir = temp_dir.child("symlink_to_site");
        std::os::unix::fs::symlink(&real_dir, &symlink_dir).expect("Failed to create symlink");

        // Initialize the site through the symlink
        Operations::new(&symlink_dir.to_path_buf())
            .expect("Failed new operation with symlink path");

        // Check if the site was created in the real directory
        real_dir
            .child("rgent.toml")
            .assert(predicate::path::exists().and(predicate::path::is_file()));

        real_dir
            .child("content/drafts")
            .assert(predicate::path::exists().and(predicate::path::is_dir()));

        real_dir
            .child("output")
            .assert(predicate::path::exists().and(predicate::path::is_dir()));

        real_dir
            .child("themes")
            .assert(predicate::path::exists().and(predicate::path::is_dir()));

        // Also verify the site is accessible through the symlink
        symlink_dir
            .child("rgent.toml")
            .assert(predicate::path::exists().and(predicate::path::is_file()));
    }

    #[test]
    fn test_relativize_path() {
        let temp_dir = assert_fs::TempDir::new().unwrap();
        let target_dir = temp_dir.child("rgent_test");
        Operations::new(&target_dir.to_path_buf()).expect("Failed new operation");

        let dir_path = target_dir.child("content/drafts").to_path_buf();
        let from = target_dir.child("content").to_path_buf();
        let to = target_dir.child("output").to_path_buf();

        let relative_dir_path = Operations::rebase_path(&dir_path, &from, &to, None)
            .expect("Failed to get relative path");
        assert_eq!(
            target_dir.child("output/drafts").to_path_buf(),
            relative_dir_path
        );

        let relative_dir_path_ext = Operations::rebase_path(&dir_path, &from, &to, Some("ext"))
            .expect("Failed to get relative path");
        assert_eq!(
            target_dir.child("output/drafts.ext").to_path_buf(),
            relative_dir_path_ext
        );

        let file_path = target_dir.child("content/some-file.md").to_path_buf();
        let relative_file_path = Operations::rebase_path(&file_path, &from, &to, None)
            .expect("Failed to get relative path");
        assert_eq!(
            target_dir.child("output/some-file.md").to_path_buf(),
            relative_file_path
        );

        let file_path_ext = target_dir.child("content/some-file.md").to_path_buf();
        let relative_file_path_ext =
            Operations::rebase_path(&file_path_ext, &from, &to, Some("ext"))
                .expect("Failed to get relative path");
        assert_eq!(
            target_dir.child("output/some-file.ext").to_path_buf(),
            relative_file_path_ext
        );
    }

    #[test]
    fn test_publish_markdown_without_slug() {
        let temp_dir = assert_fs::TempDir::new().unwrap();
        let target_dir = temp_dir.child("rgent_test");
        Operations::new(&target_dir.to_path_buf()).expect("Failed new operation");

        // let dir_path = target_dir.child("content/drafts").to_path_buf();
        // let from = target_dir.child("content").to_path_buf();
        // let to = target_dir.child("output").to_path_buf();

        let markdown = target_dir.child("content/test.md");

        let content = r#"
title = 'Test post'
author = 'Pawan Dubey'
date = '2024 12 28'
tags = []

#####

# Hello world"#;

        fs::write(&markdown, content).expect("Failed to write test markdown");

        Operations::publish(&target_dir, false).expect("Failed publish operation");

        print_dir(&target_dir).expect("Error during iterating over target dir");

        target_dir
            .child("output/test-post/index.html")
            .assert(predicate::path::exists().and(predicate::path::is_file()));

        let contents = fs::read_to_string(target_dir.child("output/test-post/index.html"))
            .expect("Failed to read file");
        assert!(contents.contains("<h1>Hello world</h1>"), "{}", contents);
        assert!(
            contents.contains("<title>Test post | New Rgent Site</title>"),
            "{}",
            contents
        );
        temp_dir.close().expect("Failed to delete temp directory")
    }

    #[test]
    fn test_publish_markdown_with_slug() {
        let temp_dir = assert_fs::TempDir::new().unwrap();
        let target_dir = temp_dir.child("rgent_test");
        Operations::new(&target_dir.to_path_buf()).expect("Failed new operation");

        // let dir_path = target_dir.child("content/drafts").to_path_buf();
        // let from = target_dir.child("content").to_path_buf();
        // let to = target_dir.child("output").to_path_buf();

        let markdown = target_dir.child("content/test.md");

        let content = r#"
title = 'Test post'
author = 'Pawan Dubey'
date = '2024 12 28'
tags = []
slug = 'custom-test-post'

#####

# Hello world"#;

        fs::write(&markdown, content).expect("Failed to write test markdown");

        Operations::publish(&target_dir, false).expect("Failed publish operation");

        print_dir(&target_dir).expect("Error during iterating over target dir");

        target_dir
            .child("output/custom-test-post/index.html")
            .assert(predicate::path::exists().and(predicate::path::is_file()));

        let contents = fs::read_to_string(target_dir.child("output/custom-test-post/index.html"))
            .expect("Failed to read file");
        assert!(contents.contains("<h1>Hello world</h1>"), "{}", contents);
        assert!(
            contents.contains("<title>Test post | New Rgent Site</title>"),
            "{}",
            contents
        );

        temp_dir.close().expect("Failed to delete temp directory")
    }

    #[test]
    fn test_publish_uses_frontmatter_layout() {
        let temp_dir = assert_fs::TempDir::new().unwrap();
        let target_dir = temp_dir.child("rgent_test");
        Operations::new(&target_dir.to_path_buf()).expect("Failed new operation");

        fs::write(
            target_dir.child("themes/wells/gallery.html"),
            "{{config.site.name}}|{{post.title}}|{{post.permalink}}|{{#each post.tags}}{{this}},{{/each}}|{{{post.content}}}",
        )
        .expect("Failed to write layout");

        let content = indoc! {r#"
            title = 'Holiday <Pics>'
            author = 'Pawan Dubey'
            date = '2024 12 28'
            tags = ['travel', 'photos']
            layout = 'gallery'
            slug = 'holiday'

            #####

            *Sunny*"#};
        fs::write(target_dir.child("content/holiday.md"), content)
            .expect("Failed to write test markdown");

        Operations::publish(&target_dir, false).expect("Failed publish operation");

        let contents = fs::read_to_string(target_dir.child("output/holiday/index.html"))
            .expect("Failed to read file");
        assert_eq!(
            "New Rgent Site|Holiday &lt;Pics&gt;|https://blog.example.com/holiday/|travel,photos,|<p><em>Sunny</em></p>",
            contents.trim()
        );
        temp_dir.close().expect("Failed to delete temp directory")
    }

    #[test]
    fn test_publish_copies_theme_assets() {
        let temp_dir = assert_fs::TempDir::new().unwrap();
        let target_dir = temp_dir.child("rgent_test");
        Operations::new(&target_dir.to_path_buf()).expect("Failed new operation");

        Operations::publish(&target_dir, false).expect("Failed publish operation");

        target_dir
            .child("output/assets/style.css")
            .assert(predicate::path::is_file());
        temp_dir.close().expect("Failed to delete temp directory")
    }

    #[test]
    fn test_publish_fails_for_unknown_layout() {
        let temp_dir = assert_fs::TempDir::new().unwrap();
        let target_dir = temp_dir.child("rgent_test");
        Operations::new(&target_dir.to_path_buf()).expect("Failed new operation");

        let content = indoc! {r#"
            title = 'Test post'
            author = 'Pawan Dubey'
            date = '2024 12 28'
            tags = []
            layout = 'missing'

            #####

            # Hello world"#};
        fs::write(target_dir.child("content/test.md"), content)
            .expect("Failed to write test markdown");

        let error = Operations::publish(&target_dir, false)
            .expect_err("Publishing with an unknown layout should fail");
        assert!(
            format!("{:#}", error).contains("no layout named \"missing\""),
            "{:#}",
            error
        );
        temp_dir.close().expect("Failed to delete temp directory")
    }

    fn write_post(target_dir: &assert_fs::fixture::ChildPath, file: &str, title: &str, date: &str) {
        let content = format!(
            "title = '{}'\nauthor = 'Pawan Dubey'\ndate = '{}'\ntags = []\n\n#####\n\nBody of {}",
            title, date, title
        );
        let path = target_dir.child("content").child(file);
        fs::create_dir_all(path.parent().unwrap()).expect("Failed to create content dir");
        fs::write(path, content).expect("Failed to write test markdown");
    }

    fn write_content(target_dir: &assert_fs::fixture::ChildPath, file: &str, frontmatter: &str) {
        let path = target_dir.child("content").child(file);
        fs::create_dir_all(path.parent().unwrap()).expect("Failed to create content dir");
        fs::write(path, format!("{}\n#####\n\nBody of {}", frontmatter, file))
            .expect("Failed to write test markdown");
    }

    fn replace_in_config(target_dir: &assert_fs::fixture::ChildPath, from: &str, to: &str) {
        let config_path = target_dir.child("rgent.toml");
        let config = fs::read_to_string(&config_path).expect("Failed to read config");
        assert!(config.contains(from), "{} not in config:\n{}", from, config);
        fs::write(&config_path, config.replace(from, to)).expect("Failed to write config");
    }

    fn read_output(target_dir: &assert_fs::fixture::ChildPath, path: &str) -> String {
        fs::read_to_string(target_dir.child("output").child(path))
            .unwrap_or_else(|e| panic!("Failed to read output/{}: {}", path, e))
            .trim()
            .to_string()
    }

    #[test]
    fn test_publish_highlights_code_blocks_with_theme_config() {
        let temp_dir = assert_fs::TempDir::new().unwrap();
        let target_dir = temp_dir.child("rgent_test");
        Operations::new(&target_dir.to_path_buf()).expect("Failed new operation");
        fs::write(
            target_dir.child("themes/wells/post.html"),
            "{{{post.content}}}",
        )
        .unwrap();

        let path = target_dir.child("content/code.md");
        fs::write(
            &path,
            "title = 'Code'\ndate = '2024 01 01'\n\n#####\n\n```rust\nfn main() {}\n```\n",
        )
        .unwrap();

        Operations::publish(&target_dir, false).expect("Failed publish operation");

        let contents = read_output(&target_dir, "code/index.html");
        assert!(
            contents.starts_with("<pre class=\"giallo hl-l-code hl-d-code\""),
            "{}",
            contents
        );
        assert!(contents.contains("data-lang=\"rust\""), "{}", contents);
        assert_eq!(
            include_str!("starter_theme/assets/highlight.css"),
            fs::read_to_string(target_dir.child("output/assets/highlight.css")).unwrap()
        );
        temp_dir.close().unwrap();
    }

    #[test]
    fn test_publish_leaves_code_blocks_plain_without_theme_config() {
        let temp_dir = assert_fs::TempDir::new().unwrap();
        let target_dir = temp_dir.child("rgent_test");
        Operations::new(&target_dir.to_path_buf()).expect("Failed new operation");
        fs::remove_file(target_dir.child("themes/wells/theme.toml")).unwrap();
        fs::write(
            target_dir.child("themes/wells/post.html"),
            "{{{post.content}}}",
        )
        .unwrap();

        fs::write(
            target_dir.child("content/code.md"),
            "title = 'Code'\ndate = '2024 01 01'\n\n#####\n\n```rust\nfn main() {}\n```\n",
        )
        .unwrap();

        Operations::publish(&target_dir, false).expect("Failed publish operation");

        assert_eq!(
            "<pre><code class=\"language-rust\">fn main() {}\n</code></pre>",
            read_output(&target_dir, "code/index.html")
        );
        temp_dir.close().unwrap();
    }

    #[test]
    fn test_publish_exposes_excerpts() {
        let temp_dir = assert_fs::TempDir::new().unwrap();
        let target_dir = temp_dir.child("rgent_test");
        Operations::new(&target_dir.to_path_buf()).expect("Failed new operation");
        fs::write(
            target_dir.child("themes/wells/index.html"),
            "{{#each index.posts}}[{{title}}|{{has_more}}|{{{excerpt}}}]{{/each}}",
        )
        .unwrap();
        fs::write(
            target_dir.child("themes/wells/post.html"),
            "{{{post.content}}}",
        )
        .unwrap();

        fs::write(
            target_dir.child("content/marked.md"),
            "title = 'Marked'\ndate = '2024 01 02'\n\n#####\n\nIntro *here*.\n\n```rust\nfn intro() {}\n```\n\n##more##\n\nThe rest.\n",
        )
        .unwrap();
        fs::write(
            target_dir.child("content/short.md"),
            "title = 'Short'\ndate = '2024 01 01'\n\n#####\n\nAll of it.\n",
        )
        .unwrap();

        Operations::publish(&target_dir, false).expect("Failed publish operation");

        let index = read_output(&target_dir, "index.html");
        let (marked, short) = index.split_once("][").expect("Expected two posts");
        assert!(
            marked.starts_with("[Marked|true|<p>Intro <em>here</em>.</p>\n<pre class=\"giallo"),
            "{}",
            marked
        );
        assert!(!marked.contains("The rest."), "{}", marked);
        assert_eq!("Short|false|<p>All of it.</p>\n]", short);

        let post = read_output(&target_dir, "marked/index.html");
        assert!(post.contains("<p>Intro <em>here</em>.</p>"), "{}", post);
        assert!(post.contains("<p>The rest.</p>"), "{}", post);
        assert!(!post.contains("more##"), "{}", post);
        temp_dir.close().unwrap();
    }

    #[test]
    fn test_publish_writes_rss_feed() {
        let temp_dir = assert_fs::TempDir::new().unwrap();
        let target_dir = temp_dir.child("rgent_test");
        Operations::new(&target_dir.to_path_buf()).expect("Failed new operation");

        for day in 1..=22 {
            write_content(
                &target_dir,
                &format!("post-{:02}.md", day),
                &format!(
                    "title = 'Post {:02}'\ndate = '2024 01 {:02}'\ntags = ['t{}']",
                    day, day, day
                ),
            );
        }
        write_content(
            &target_dir,
            "about.md",
            "title = 'About'\nlayout = 'page'\ntags = ['nav']",
        );

        Operations::publish(&target_dir, false).expect("Failed publish operation");

        let xml =
            fs::read_to_string(target_dir.child("output/feed.xml")).expect("Failed to read feed");
        let channel: rss::Channel = xml.parse().expect("Feed should be valid RSS");

        assert_eq!("New Rgent Site", channel.title());
        assert_eq!("https://blog.example.com/", channel.link());
        assert_eq!("Your Catchy Tagline", channel.description());
        assert_eq!(FEED_ITEMS, channel.items().len());

        let titles: Vec<&str> = channel.items().iter().filter_map(|i| i.title()).collect();
        assert_eq!(Some(&"Post 22"), titles.first());
        assert_eq!(Some(&"Post 03"), titles.last());
        assert!(!titles.contains(&"About"));

        let newest = &channel.items()[0];
        assert_eq!(Some("https://blog.example.com/post-22/"), newest.link());
        assert_eq!(Some("Mon, 22 Jan 2024 00:00:00 +0000"), newest.pub_date());
        assert_eq!(
            Some("<p>Body of post-22.md</p>"),
            newest.description().map(str::trim)
        );
        assert_eq!(
            Some("<p>Body of post-22.md</p>"),
            newest.content().map(str::trim)
        );
        assert_eq!(
            Some("Mon, 22 Jan 2024 00:00:00 +0000"),
            channel.last_build_date()
        );
        temp_dir.close().unwrap();
    }

    #[test]
    fn test_publish_uses_theme_feed_layout_when_present() {
        let temp_dir = assert_fs::TempDir::new().unwrap();
        let target_dir = temp_dir.child("rgent_test");
        Operations::new(&target_dir.to_path_buf()).expect("Failed new operation");
        fs::write(
            target_dir.child("themes/wells/feed.html"),
            "{{feed.url}}|{{#each feed.posts}}{{title}}@{{permalink}},{{/each}}",
        )
        .unwrap();
        write_content(&target_dir, "a.md", "title = 'First'\ndate = '2024 01 01'");
        write_content(&target_dir, "b.md", "title = 'Second'\ndate = '2024 01 02'");

        Operations::publish(&target_dir, false).expect("Failed publish operation");

        assert_eq!(
            "https://blog.example.com/feed.xml|Second@https://blog.example.com/second/,First@https://blog.example.com/first/,",
            read_output(&target_dir, "feed.xml")
        );
        temp_dir.close().unwrap();
    }

    #[test]
    fn test_publish_writes_paginated_tag_pages() {
        let temp_dir = assert_fs::TempDir::new().unwrap();
        let target_dir = temp_dir.child("rgent_test");
        Operations::new(&target_dir.to_path_buf()).expect("Failed new operation");
        replace_in_config(&target_dir, "rendertags = false", "rendertags = true");
        set_posts_per_page(&target_dir, 2);
        fs::write(
            target_dir.child("themes/wells/tagindex.html"),
            "{{tag.name}}@{{tag.url}}|{{index.current_page}}/{{index.total_pages}}|{{index.next_page}}|{{#each index.posts}}{{title}},{{/each}}|{{#each tags}}{{slug}},{{/each}}",
        )
        .unwrap();
        fs::write(
            target_dir.child("themes/wells/post.html"),
            "{{#each post.tag_links}}{{name}}={{url}},{{/each}}",
        )
        .unwrap();

        write_content(
            &target_dir,
            "a.md",
            "title = 'A'\ndate = '2024 01 01'\ntags = ['Rust', 'Web Dev']",
        );
        write_content(
            &target_dir,
            "b.md",
            "title = 'B'\ndate = '2024 01 02'\ntags = ['rust']",
        );
        write_content(
            &target_dir,
            "c.md",
            "title = 'C'\ndate = '2024 01 03'\ntags = ['Rust']",
        );
        write_content(
            &target_dir,
            "about.md",
            "title = 'About'\nlayout = 'page'\ntags = ['nav']",
        );

        Operations::publish(&target_dir, false).expect("Failed publish operation");

        assert_eq!(
            "Rust@/tags/rust/|1/2|/tags/rust/page/2/|C,B,|rust,web-dev,",
            read_output(&target_dir, "tags/rust/index.html")
        );
        assert_eq!(
            "Rust@/tags/rust/|2/2||A,|rust,web-dev,",
            read_output(&target_dir, "tags/rust/page/2/index.html")
        );
        assert_eq!(
            "Web Dev@/tags/web-dev/|1/1||A,|rust,web-dev,",
            read_output(&target_dir, "tags/web-dev/index.html")
        );
        target_dir
            .child("output/tags/nav")
            .assert(predicate::path::missing());
        assert_eq!(
            "Rust=/tags/rust/,Web Dev=/tags/web-dev/,",
            read_output(&target_dir, "a/index.html")
        );
        temp_dir.close().unwrap();
    }

    #[test]
    fn test_publish_skips_tag_pages_when_disabled() {
        let temp_dir = assert_fs::TempDir::new().unwrap();
        let target_dir = temp_dir.child("rgent_test");
        Operations::new(&target_dir.to_path_buf()).expect("Failed new operation");
        write_content(
            &target_dir,
            "a.md",
            "title = 'A'\ndate = '2024 01 01'\ntags = ['rust']",
        );

        Operations::publish(&target_dir, false).expect("Failed publish operation");

        target_dir
            .child("output/tags")
            .assert(predicate::path::missing());
        temp_dir.close().unwrap();
    }

    #[test]
    fn test_publish_tag_pages_require_tagindex_layout() {
        let temp_dir = assert_fs::TempDir::new().unwrap();
        let target_dir = temp_dir.child("rgent_test");
        Operations::new(&target_dir.to_path_buf()).expect("Failed new operation");
        replace_in_config(&target_dir, "rendertags = false", "rendertags = true");
        fs::remove_file(target_dir.child("themes/wells/tagindex.html")).unwrap();

        let error = Operations::publish(&target_dir, false)
            .expect_err("Publishing tag pages without a tagindex layout should fail");

        assert!(
            format!("{:#}", error).contains("has no `tagindex.html` layout"),
            "{:#}",
            error
        );
        temp_dir.close().unwrap();
    }

    #[test]
    fn test_publish_reports_colliding_tags() {
        let temp_dir = assert_fs::TempDir::new().unwrap();
        let target_dir = temp_dir.child("rgent_test");
        Operations::new(&target_dir.to_path_buf()).expect("Failed new operation");
        replace_in_config(&target_dir, "rendertags = false", "rendertags = true");
        write_content(
            &target_dir,
            "a.md",
            "title = 'A'\ndate = '2024 01 01'\ntags = ['c']",
        );
        write_content(
            &target_dir,
            "b.md",
            "title = 'B'\ndate = '2024 01 02'\ntags = ['c++']",
        );

        let error =
            Operations::publish(&target_dir, false).expect_err("Colliding tags should fail");

        assert!(
            format!("{:#}", error).contains("would both be published at /tags/c/"),
            "{:#}",
            error
        );
        temp_dir.close().unwrap();
    }

    #[test]
    fn test_publish_separates_pages_from_posts() {
        let temp_dir = assert_fs::TempDir::new().unwrap();
        let target_dir = temp_dir.child("rgent_test");
        Operations::new(&target_dir.to_path_buf()).expect("Failed new operation");
        fs::write(
            target_dir.child("themes/wells/index.html"),
            "{{#each index.posts}}{{title}}:{{kind}},{{/each}}",
        )
        .unwrap();
        fs::write(
            target_dir.child("themes/wells/page.html"),
            "page|{{post.kind}}|{{post.title}}|{{post.author}}|{{#if post.date}}dated{{else}}undated{{/if}}",
        )
        .unwrap();

        write_content(
            &target_dir,
            "post.md",
            "title = 'A Post'\nauthor = 'Pawan'\ndate = '2024 01 01'",
        );
        write_content(&target_dir, "about.md", "title = 'About'\nlayout = 'page'");

        Operations::publish(&target_dir, false).expect("Failed publish operation");

        assert_eq!("A Post:post,", read_output(&target_dir, "index.html"));
        assert_eq!(
            "page|page|About|You!|undated",
            read_output(&target_dir, "about/index.html")
        );
        temp_dir.close().unwrap();
    }

    #[test]
    fn test_publish_exposes_nav_pages_to_all_layouts() {
        let temp_dir = assert_fs::TempDir::new().unwrap();
        let target_dir = temp_dir.child("rgent_test");
        Operations::new(&target_dir.to_path_buf()).expect("Failed new operation");
        let nav = "{{#each nav_pages}}{{title}}@{{url}},{{/each}}";
        for layout in ["index", "post", "page"] {
            fs::write(
                target_dir.child(format!("themes/wells/{}.html", layout)),
                format!("{}|{}", layout, nav),
            )
            .unwrap();
        }

        write_content(
            &target_dir,
            "post.md",
            "title = 'A Post'\ndate = '2024 01 01'\ntags = ['nav']",
        );
        write_content(
            &target_dir,
            "projects.md",
            "title = 'Projects'\nlayout = 'page'\ntags = ['nav']",
        );
        write_content(
            &target_dir,
            "about.md",
            "title = 'About'\nlayout = 'page'\ntags = ['nav', 'me']",
        );
        write_content(
            &target_dir,
            "hidden.md",
            "title = 'Hidden'\nlayout = 'page'",
        );

        Operations::publish(&target_dir, false).expect("Failed publish operation");

        let expected_nav = "About@/about/,Projects@/projects/,";
        assert_eq!(
            format!("index|{}", expected_nav),
            read_output(&target_dir, "index.html")
        );
        assert_eq!(
            format!("post|{}", expected_nav),
            read_output(&target_dir, "a-post/index.html")
        );
        assert_eq!(
            format!("page|{}", expected_nav),
            read_output(&target_dir, "hidden/index.html")
        );
        temp_dir.close().unwrap();
    }

    #[test]
    fn test_publish_skips_excluded_directories() {
        let temp_dir = assert_fs::TempDir::new().unwrap();
        let target_dir = temp_dir.child("rgent_test");
        Operations::new(&target_dir.to_path_buf()).expect("Failed new operation");
        fs::write(
            target_dir.child("themes/wells/index.html"),
            "{{#each index.posts}}{{title}},{{/each}}",
        )
        .unwrap();

        write_content(
            &target_dir,
            "live.md",
            "title = 'Live'\ndate = '2024 01 01'",
        );
        write_content(
            &target_dir,
            "drafts/wip.md",
            "title = 'Work In Progress'\ndate = '2024 01 02'",
        );
        write_content(
            &target_dir,
            "drafts/nested/idea.md",
            "title = 'Idea'\ndate = '2024 01 03'",
        );
        write_content(
            &target_dir,
            "drafts-not/kept.md",
            "title = 'Kept'\ndate = '2024 01 04'",
        );

        Operations::publish(&target_dir, false).expect("Failed publish operation");

        assert_eq!("Kept,Live,", read_output(&target_dir, "index.html"));
        target_dir
            .child("output/drafts")
            .assert(predicate::path::missing());
        target_dir
            .child("output/drafts-not/kept/index.html")
            .assert(predicate::path::is_file());
        temp_dir.close().unwrap();
    }

    #[test]
    fn test_publish_fails_for_post_without_date() {
        let temp_dir = assert_fs::TempDir::new().unwrap();
        let target_dir = temp_dir.child("rgent_test");
        Operations::new(&target_dir.to_path_buf()).expect("Failed new operation");
        write_content(&target_dir, "undated.md", "title = 'Undated'");

        let error = Operations::publish(&target_dir, false)
            .expect_err("Publishing a post without a date should fail");

        assert!(
            format!("{:#}", error).contains("has no `date`"),
            "{:#}",
            error
        );
        temp_dir.close().unwrap();
    }

    #[test]
    fn test_publish_fails_for_date_not_matching_input_format() {
        let temp_dir = assert_fs::TempDir::new().unwrap();
        let target_dir = temp_dir.child("rgent_test");
        Operations::new(&target_dir.to_path_buf()).expect("Failed new operation");
        write_content(
            &target_dir,
            "post.md",
            "title = 'Post'\ndate = '2024-01-01'",
        );

        let error = Operations::publish(&target_dir, false)
            .expect_err("Publishing a post with a malformed date should fail");

        let message = format!("{:#}", error);
        assert!(message.contains("Invalid `date`"), "{}", message);
        assert!(
            message.contains("does not match the date pattern \"yyyy MM dd\""),
            "{}",
            message
        );
        temp_dir.close().unwrap();
    }

    #[test]
    fn test_publish_formats_and_sorts_by_parsed_dates() {
        let temp_dir = assert_fs::TempDir::new().unwrap();
        let target_dir = temp_dir.child("rgent_test");
        Operations::new(&target_dir.to_path_buf()).expect("Failed new operation");
        replace_in_config(
            &target_dir,
            "inputdate = 'yyyy MM dd'",
            "inputdate = 'd/M/yyyy'",
        );
        replace_in_config(
            &target_dir,
            "outputdate = 'MMM d yyyy'",
            "outputdate = 'MMMM d, yyyy'",
        );
        fs::write(
            target_dir.child("themes/wells/index.html"),
            "{{#each index.posts}}{{title}}={{date}}/{{date_iso}},{{/each}}",
        )
        .unwrap();

        write_content(&target_dir, "a.md", "title = 'Early'\ndate = '2/1/2024'");
        write_content(&target_dir, "b.md", "title = 'Late'\ndate = '10/1/2024'");
        write_content(
            &target_dir,
            "c.md",
            "title = 'Previous Year'\ndate = '31/12/2023'",
        );

        Operations::publish(&target_dir, false).expect("Failed publish operation");

        assert_eq!(
            "Late=January 10, 2024/2024-01-10,Early=January 2, 2024/2024-01-02,Previous Year=December 31, 2023/2023-12-31,",
            read_output(&target_dir, "index.html")
        );
        temp_dir.close().unwrap();
    }

    fn set_posts_per_page(target_dir: &assert_fs::fixture::ChildPath, per_page: u64) {
        let config_path = target_dir.child("rgent.toml");
        let config = fs::read_to_string(&config_path).expect("Failed to read config");
        let config = config.replace(
            "postsperindex = 10",
            &format!("postsperindex = {}", per_page),
        );
        fs::write(&config_path, config).expect("Failed to write config");
    }

    #[test]
    fn test_publish_writes_paginated_index() {
        let temp_dir = assert_fs::TempDir::new().unwrap();
        let target_dir = temp_dir.child("rgent_test");
        Operations::new(&target_dir.to_path_buf()).expect("Failed new operation");
        set_posts_per_page(&target_dir, 2);
        fs::write(
            target_dir.child("themes/wells/index.html"),
            "{{index.current_page}}/{{index.total_pages}}|{{index.previous_page}}|{{index.next_page}}|{{#each index.posts}}{{title}}@{{url}},{{/each}}",
        )
        .expect("Failed to write index layout");

        write_post(&target_dir, "a.md", "Oldest", "2024 01 01");
        write_post(&target_dir, "b.md", "Newest", "2024 03 01");
        write_post(&target_dir, "c.md", "Middle", "2024 02 01");

        Operations::publish(&target_dir, false).expect("Failed publish operation");

        let first = fs::read_to_string(target_dir.child("output/index.html"))
            .expect("Failed to read first index page");
        assert_eq!(
            "1/2||/page/2/|Newest@/newest/,Middle@/middle/,",
            first.trim()
        );

        let second = fs::read_to_string(target_dir.child("output/page/2/index.html"))
            .expect("Failed to read second index page");
        assert_eq!("2/2|/||Oldest@/oldest/,", second.trim());

        target_dir
            .child("output/page/3")
            .assert(predicate::path::missing());
        temp_dir.close().expect("Failed to delete temp directory")
    }

    #[test]
    fn test_publish_writes_index_for_empty_site() {
        let temp_dir = assert_fs::TempDir::new().unwrap();
        let target_dir = temp_dir.child("rgent_test");
        Operations::new(&target_dir.to_path_buf()).expect("Failed new operation");

        Operations::publish(&target_dir, false).expect("Failed publish operation");

        let contents = fs::read_to_string(target_dir.child("output/index.html"))
            .expect("Failed to read index page");
        assert!(contents.contains("No posts yet."), "{}", contents);
        assert!(!contents.contains("pagination"), "{}", contents);
        temp_dir.close().expect("Failed to delete temp directory")
    }

    #[test]
    fn test_publish_urls_include_subdirectories() {
        let temp_dir = assert_fs::TempDir::new().unwrap();
        let target_dir = temp_dir.child("rgent_test");
        Operations::new(&target_dir.to_path_buf()).expect("Failed new operation");
        fs::write(
            target_dir.child("themes/wells/post.html"),
            "{{post.url}}|{{post.permalink}}",
        )
        .expect("Failed to write post layout");

        write_post(&target_dir, "notes/rust/tip.md", "Handy Tip", "2024 01 01");

        Operations::publish(&target_dir, false).expect("Failed publish operation");

        let contents =
            fs::read_to_string(target_dir.child("output/notes/rust/handy-tip/index.html"))
                .expect("Failed to read post");
        assert_eq!(
            "/notes/rust/handy-tip/|https://blog.example.com/notes/rust/handy-tip/",
            contents.trim()
        );
        temp_dir.close().expect("Failed to delete temp directory")
    }

    #[test]
    fn test_url_for_dir() {
        let output = Path::new("/site/output");

        assert_eq!(
            "/",
            Operations::url_for_dir(output, output).expect("Failed to build url")
        );
        assert_eq!(
            "/a/b/",
            Operations::url_for_dir(&output.join("a/b"), output).expect("Failed to build url")
        );
        assert!(Operations::url_for_dir(Path::new("/elsewhere"), output).is_err());
    }

    fn print_dir<T>(root: T) -> Result<(), walkdir::Error>
    where
        T: AsRef<Path>,
    {
        use walkdir::WalkDir;

        for entry in WalkDir::new(root) {
            println!("{}", entry?.path().display());
        }

        Ok(())
    }

    fn new_config() -> &'static str {
        return indoc! {r#"
          source = 'content'
          output = 'output'
          exclude = ['drafts']
          inputdate = 'yyyy MM dd'
          outputdate = 'MMM d yyyy'
          theme = 'wells'
          headerimage = ''
          postsperindex = 10
          rendertags = false

          [preview]
          port = 9090

          [social]
          twitter = ''
          github = ''
          mastodon = ''

          [site]
          name = 'New Rgent Site'
          tagline = 'Your Catchy Tagline'
          author = 'You!'
          baseurl = 'https://blog.example.com'
      "#};
    }
}
