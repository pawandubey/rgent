use std::{
    fmt::Debug,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};

use crate::config::Config;

const CONFIG_FILE_NAME: &str = "rgent.toml";
const CONTENT_AND_DRAFTS_DIR: &str = "content/drafts";
const OUTPUT_DIR: &str = "output";
const THEMES_DIR: &str = "themes";

pub struct Operations {}

impl Operations {
    pub fn new(path: &PathBuf) -> Result<()> {
        // TODO: Canonicalize path?
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

    pub fn publish(_rebuild: bool) -> Result<()> {
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
            let extension = dir_entry.path().extension().with_context(|| {
                format!(
                    "Failed to resolve extension for file: {:?}",
                    dir_entry.file_name()
                )
            })?;
            if dir_entry.file_type().is_file() && extension == "md" {
                let markdown = fs::read_to_string(dir_entry.path())
                    .with_context(|| format!("Failed to read file: {:?}", dir_entry.file_name()))?;

                let parser = pulldown_cmark::Parser::new_ext(&markdown, md_options);

                let mut html = String::with_capacity(markdown.len());
                pulldown_cmark::html::push_html(&mut html, parser);

                let _output_path =
                    Self::rebase_path(dir_entry.path(), &config.source, &config.output)?;
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
