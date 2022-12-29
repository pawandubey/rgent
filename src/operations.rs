use std::{fs, path::PathBuf};

use anyhow::{Context, Result};

use crate::config::Config;

const CONFIG_FILE_NAME: &str = "rgent.toml";
const CONTENT_AND_DRAFTS_DIR: &str = "content/drafts";
const OUTPUT_DIR: &str = "output";
const THEMES_DIR: &str = "themes";

pub struct Operations {}

impl Operations {
    pub fn new(path: &PathBuf) -> Result<()> {
        fs::create_dir_all(path).context("Failed to create site directory")?;

        fs::create_dir_all(path.join(CONTENT_AND_DRAFTS_DIR)).context("Failed to create content directory")?;
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
        Ok(())
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
