use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct Config {
    source: String,
    output: String,
    exclude: Vec<String>,

    #[serde(rename = "inputdate")]
    input_date_format: String,

    #[serde(rename = "outputdate")]
    output_date_format: String,

    theme: String,

    #[serde(rename = "headerimage")]
    header_image: String,

    #[serde(rename = "postsperindex")]
    posts_per_page: u64,

    #[serde(rename = "rendertags", default)]
    render_tags: bool,

    preview: PreviewConfig,

    social: SocialConfig,

    site: SiteConfig,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            source: "content".into(),
            output: "output".into(),
            exclude: vec!["drafts".into()],
            input_date_format: "yyyy MM dd".into(),
            output_date_format: "MMM d yyyy".into(),
            theme: "wells".into(),
            header_image: "".into(),
            posts_per_page: 10,
            render_tags: false,
            preview: PreviewConfig { port: 9090 },
            social: SocialConfig {
                twitter: Some("".into()),
                github: Some("".into()),
                mastodon: Some("".into()),
            },
            site: SiteConfig {
                name: "New Rgent Site".into(),
                tagline: Some("Your Catchy Tagline".into()),
                author: "You!".into(),
                base_url: "https://blog.example.com".into(),
            }
        }
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct PreviewConfig {
    port: u16,
}

#[derive(Serialize, Deserialize, Debug, Default, PartialEq, Eq)]
#[serde(default)]
pub struct SocialConfig {
    twitter: Option<String>,
    github: Option<String>,
    mastodon: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Default, PartialEq, Eq)]
pub struct SiteConfig {
    name: String,

    #[serde(default)]
    tagline: Option<String>,

    author: String,

    #[serde(rename = "baseurl")]
    base_url: String,
}

#[cfg(test)]
mod test {
    use indoc::indoc;
    use crate::config::*;

    #[test]
    fn test_config_can_be_parsed() {
        let test_config = test_config();

        let config: Config = toml::from_str(&test_config).expect("Should deserialize");

        let expected_config = Config {
            source: "content".into(),
            output: "output".into(),
            exclude: vec!["drafts".into()],
            input_date_format: "yyyy MM dd".into(),
            output_date_format: "MMM d yyyy".into(),
            theme: "wells".into(),
            header_image: "".into(),
            posts_per_page: 10,
            render_tags: false,
            preview: PreviewConfig { port: 9090 },
            social: SocialConfig {
                twitter: Some("thepawandubey".into()),
                github: Some("pawandubey".into()),
                mastodon: None,
            },
            site: SiteConfig {
                name: "Not So Null".into(),
                tagline: Some("Code += Play".into()),
                author: "Pawan Dubey".into(),
                base_url: "https://blog.pawandubey.com".into()
            }
        };

        assert_eq!(expected_config, config)
    }

    fn test_config() -> &'static str {
        return indoc! {r#"
          #parsing details
          source = "content"
          output = "output"
          exclude = ["drafts"]

          #styling
          inputdate = "yyyy MM dd"
          outputdate = "MMM d yyyy"
          theme = "wells"
          headerimage = ""
          postsperindex = 10

          #render files as per tags?
          rendertags = false

          #preview
          [preview]
            port = 9090

          #social media details
          [social]
            twitter = "thepawandubey"
            github = "pawandubey"

          #site details
          [site]
            name = "Not So Null"
            tagline = "Code += Play"
            author = "Pawan Dubey"
            baseurl = "https://blog.pawandubey.com"
      "#};
    }
}
