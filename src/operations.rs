use std::{
    fmt::Debug,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};

use crate::config::Config;

use itertools::Itertools;

const CONFIG_FILE_NAME: &str = "rgent.toml";
const CONTENT_AND_DRAFTS_DIR: &str = "content/drafts";
const OUTPUT_DIR: &str = "output";
const THEMES_DIR: &str = "themes";

pub struct Operations {}

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct Frontmatter {
    title: String,
    author: String,
    layout: Option<String>,
    date: String,
    tags: Vec<String>,
    slug: Option<String>,
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

        let path_to_config = path.join(CONFIG_FILE_NAME);
        let serialized_config = toml::to_string_pretty(&Config::default())
            .context("Failed to serialize default config")?;
        fs::write(&path_to_config, serialized_config).context("Failed to write to config file")?;

        println!("Initialized new site config at {:?}", path_to_config);
        Ok(())
    }

    pub fn preview(_port: u16) -> Result<()> {
        Ok(())
    }

    pub fn publish(_rebuild: bool) -> Result<(), anyhow::Error> {
        let config_str = fs::read_to_string(CONFIG_FILE_NAME)
            .context("Failed to read config file. Are we in the right directory?")?;
        let config: Config =
            toml::from_str(&config_str).context("Failed to parse config. Is it valid TOML?")?;

        let mut md_options = pulldown_cmark::Options::empty();
        md_options.insert(pulldown_cmark::Options::ENABLE_FOOTNOTES);
        md_options.insert(pulldown_cmark::Options::ENABLE_SMART_PUNCTUATION);
        md_options.insert(pulldown_cmark::Options::ENABLE_STRIKETHROUGH);
        md_options.insert(pulldown_cmark::Options::ENABLE_TABLES);

        for entry in walkdir::WalkDir::new(&config.source) {
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

                let content_parser = pulldown_cmark::Parser::new_ext(&markdown, md_options);

                let mut html = String::with_capacity(markdown.len());
                pulldown_cmark::html::push_html(&mut html, content_parser);

                // let title = &frontmatter.title;

                let slug = match &frontmatter.slug {
                    Some(s) => s,
                    None => &frontmatter.title.trim().to_lowercase().split_whitespace().join("-")
                };

                dbg!(&frontmatter);
                dbg!(&slug);

                // let dir_file_stem = dir_entry.path().file_stem().with_context(|| {
                //     format!("Failed to get file stem from path: {:?}", dir_entry.path())
                // })?;

                let indexed_output_file_path = dir_entry
                    .path()
                    .with_file_name(slug.trim())
                    .join("index.html");
                let output_path = Self::rebase_path(
                    &indexed_output_file_path,
                    &config.source,
                    &config.output,
                    Some("html"),
                )?;

                let indexed_base_dir = &output_path.parent().with_context(|| {
                    format!(
                        "Failed to extract base directory from path: {:?}",
                        output_path
                    )
                })?;

                dbg!(&indexed_output_file_path);
                dbg!(&indexed_base_dir);

                fs::create_dir_all(indexed_base_dir).with_context(|| {
                    format!(
                        "Failed to create index base directory: {:?}",
                        indexed_output_file_path.parent()
                    )
                })?;

                fs::write(&output_path, html).with_context(|| {
                    format!(
                        "Failed to write generated HTML for: {:?} to {:?}",
                        dir_entry.path(),
                        output_path
                    )
                })?;

                /*

                After, we will take the slug from the page/post frontmatter.
                */
                // TODO: Also write posts to dir/index.html
                // TODO: Copy over all assets
            }
        }
        Ok(())
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
        expected_dir
            .assert(predicate::path::exists().and(predicate::path::is_dir()));
            
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
        Operations::new(&symlink_dir.to_path_buf()).expect("Failed new operation with symlink path");
        
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
date = '2024-12-28'
tags = []

#####

# Hello world"#;

        fs::write(&markdown, content).expect("Failed to write test markdown");

        let old_pwd = std::env::current_dir().expect("Failed to get PWD");
        std::env::set_current_dir(&target_dir).expect("Failed to set PWD");

        Operations::publish(false).expect("Failed publish operation");
        std::env::set_current_dir(&old_pwd).expect("Failed to set PWD");

        print_dir(&target_dir).expect("Error during iterating over target dir");

        target_dir
            .child("output/test-post/index.html")
            .assert(predicate::path::exists().and(predicate::path::is_file()));

        let contents = fs::read_to_string(target_dir.child("output/test-post/index.html"))
            .expect("Failed to read file");
        assert_eq!("<h1>Hello world</h1>", contents.trim());
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
date = '2024-12-28'
tags = []
slug = 'custom-test-post'

#####

# Hello world"#;

        fs::write(&markdown, content).expect("Failed to write test markdown");

        let old_pwd = std::env::current_dir().expect("Failed to get PWD");
        std::env::set_current_dir(&target_dir).expect("Failed to set PWD");

        Operations::publish(false).expect("Failed publish operation");
        std::env::set_current_dir(&old_pwd).expect("Failed to set PWD");

        print_dir(&target_dir).expect("Error during iterating over target dir");

        target_dir
            .child("output/custom-test-post/index.html")
            .assert(predicate::path::exists().and(predicate::path::is_file()));

        let contents = fs::read_to_string(target_dir.child("output/custom-test-post/index.html"))
            .expect("Failed to read file");
        assert_eq!("<h1>Hello world</h1>", contents.trim());

        temp_dir.close().expect("Failed to delete temp directory")
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
