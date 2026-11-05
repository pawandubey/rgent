use anyhow::{bail, Context, Result};
use indexmap::IndexMap;
use serde::Serialize;

#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
pub struct SocialLink {
    pub network: String,
    pub label: String,
    pub username: String,
    pub url: String,
}

enum Network {
    Link {
        label: &'static str,
        url: fn(&str) -> Result<String>,
    },
    NotALink,
}

pub fn social_links(social: &IndexMap<String, String>) -> Result<Vec<SocialLink>> {
    let mut links = Vec::new();

    for (network, value) in social {
        let value = value.trim();
        if value.is_empty() {
            continue;
        }

        let key = network.to_lowercase();
        let link = match (network_for(&key), is_url(value)) {
            (Some(Network::NotALink), _) => continue,
            (Some(Network::Link { label, .. }), true) => SocialLink {
                network: key,
                label: label.into(),
                username: value.into(),
                url: value.into(),
            },
            (Some(Network::Link { label, url }), false) => SocialLink {
                url: url(value)
                    .with_context(|| format!("Invalid `{}` entry in [social]", network))?,
                network: key,
                label: label.into(),
                username: value.trim_start_matches('@').into(),
            },
            (None, true) => SocialLink {
                network: key,
                label: network.clone(),
                username: value.into(),
                url: value.into(),
            },
            (None, false) => bail!(
                "Unknown network `{}` in [social]. Use one of {}, or give a full URL like `{} = \"https://...\"`.",
                network,
                KNOWN_NETWORKS.join(", "),
                network
            ),
        };
        links.push(link);
    }

    Ok(links)
}

const KNOWN_NETWORKS: &[&str] = &[
    "github",
    "gitlab",
    "codeberg",
    "twitter",
    "x",
    "mastodon",
    "bluesky",
    "threads",
    "linkedin",
    "stackoverflow",
    "so",
    "facebook",
    "fb",
    "instagram",
    "youtube",
    "twitch",
    "reddit",
    "hackernews",
    "keybase",
    "email",
];

fn network_for(key: &str) -> Option<Network> {
    let link = |label, url| Some(Network::Link { label, url });

    match key {
        "github" => link("GitHub", |u| {
            Ok(format!("https://github.com/{}", handle(u)))
        }),
        "gitlab" => link("GitLab", |u| {
            Ok(format!("https://gitlab.com/{}", handle(u)))
        }),
        "codeberg" => link("Codeberg", |u| {
            Ok(format!("https://codeberg.org/{}", handle(u)))
        }),
        "twitter" => link("Twitter", |u| {
            Ok(format!("https://twitter.com/{}", handle(u)))
        }),
        "x" => link("X", |u| Ok(format!("https://x.com/{}", handle(u)))),
        "mastodon" => link("Mastodon", mastodon_url),
        "bluesky" => link("Bluesky", |u| {
            Ok(format!("https://bsky.app/profile/{}", handle(u)))
        }),
        "threads" => link("Threads", |u| {
            Ok(format!("https://www.threads.net/@{}", handle(u)))
        }),
        "linkedin" => link("LinkedIn", |u| {
            Ok(format!("https://www.linkedin.com/in/{}", handle(u)))
        }),
        "stackoverflow" | "so" => link("Stack Overflow", |u| {
            Ok(format!("https://stackoverflow.com/users/{}", handle(u)))
        }),
        "facebook" | "fb" => link("Facebook", |u| {
            Ok(format!("https://www.facebook.com/{}", handle(u)))
        }),
        "instagram" => link("Instagram", |u| {
            Ok(format!("https://www.instagram.com/{}", handle(u)))
        }),
        "youtube" => link("YouTube", |u| {
            Ok(format!("https://www.youtube.com/@{}", handle(u)))
        }),
        "twitch" => link("Twitch", |u| {
            Ok(format!("https://www.twitch.tv/{}", handle(u)))
        }),
        "reddit" => link("Reddit", |u| {
            Ok(format!(
                "https://www.reddit.com/user/{}",
                handle(u).trim_start_matches("u/")
            ))
        }),
        "hackernews" => link("Hacker News", |u| {
            Ok(format!(
                "https://news.ycombinator.com/user?id={}",
                handle(u)
            ))
        }),
        "keybase" => link("Keybase", |u| {
            Ok(format!("https://keybase.io/{}", handle(u)))
        }),
        "email" => link("Email", |u| {
            if u.contains('@') {
                Ok(format!("mailto:{}", u))
            } else {
                bail!("{:?} is not an email address", u)
            }
        }),
        "disqus" | "gplus" => Some(Network::NotALink),
        _ => None,
    }
}

fn handle(value: &str) -> &str {
    value.trim().trim_start_matches('@')
}

fn mastodon_url(value: &str) -> Result<String> {
    match handle(value).split_once('@') {
        Some((user, instance)) if !user.is_empty() && !instance.is_empty() => {
            Ok(format!("https://{}/@{}", instance, user))
        }
        _ => bail!(
            "{:?} should look like `@user@instance.social`, or be a full profile URL",
            value
        ),
    }
}

fn is_url(value: &str) -> bool {
    value.starts_with("https://") || value.starts_with("http://")
}

#[cfg(test)]
mod test {
    use crate::social::*;

    fn links(entries: &[(&str, &str)]) -> Result<Vec<SocialLink>> {
        social_links(
            &entries
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        )
    }

    fn urls(entries: &[(&str, &str)]) -> Vec<String> {
        links(entries)
            .expect("Failed to build social links")
            .into_iter()
            .map(|link| link.url)
            .collect()
    }

    #[test]
    fn test_known_networks_build_profile_urls() {
        assert_eq!(
            vec![
                "https://github.com/pawandubey",
                "https://twitter.com/thepawandubey",
                "https://hachyderm.io/@pawan",
                "https://bsky.app/profile/pawan.bsky.social",
                "https://www.linkedin.com/in/pawan",
                "https://stackoverflow.com/users/12345",
                "https://www.facebook.com/pawan",
                "https://www.youtube.com/@pawan",
                "mailto:me@example.com",
            ],
            urls(&[
                ("github", "pawandubey"),
                ("twitter", "@thepawandubey"),
                ("mastodon", "@pawan@hachyderm.io"),
                ("bluesky", "pawan.bsky.social"),
                ("linkedin", "pawan"),
                ("so", "12345"),
                ("fb", "pawan"),
                ("youtube", "@pawan"),
                ("email", "me@example.com"),
            ])
        );
    }

    #[test]
    fn test_link_details_and_order() {
        let links = links(&[
            ("Mastodon", "pawan@hachyderm.io"),
            ("github", "@pawandubey"),
        ])
        .unwrap();

        assert_eq!(
            vec![
                SocialLink {
                    network: "mastodon".into(),
                    label: "Mastodon".into(),
                    username: "pawan@hachyderm.io".into(),
                    url: "https://hachyderm.io/@pawan".into(),
                },
                SocialLink {
                    network: "github".into(),
                    label: "GitHub".into(),
                    username: "pawandubey".into(),
                    url: "https://github.com/pawandubey".into(),
                },
            ],
            links
        );
    }

    #[test]
    fn test_full_urls_are_used_as_is() {
        let links = links(&[
            ("github", "https://github.com/someone-else"),
            ("Website", "https://pawandubey.com"),
        ])
        .unwrap();

        assert_eq!("https://github.com/someone-else", links[0].url);
        assert_eq!("GitHub", links[0].label);
        assert_eq!("website", links[1].network);
        assert_eq!("Website", links[1].label);
        assert_eq!("https://pawandubey.com", links[1].url);
    }

    #[test]
    fn test_empty_and_non_link_entries_are_skipped() {
        assert!(urls(&[
            ("twitter", ""),
            ("github", "   "),
            ("disqus", "myblog"),
            ("gplus", "+pawan"),
        ])
        .is_empty());
    }

    #[test]
    fn test_unknown_network_without_url_fails() {
        let error = links(&[("myspace", "tom")]).unwrap_err();

        let message = error.to_string();
        assert!(message.contains("Unknown network `myspace`"), "{}", message);
        assert!(message.contains("myspace = \"https://...\""), "{}", message);
    }

    #[test]
    fn test_invalid_values_fail() {
        let error = links(&[("mastodon", "pawan")]).unwrap_err();
        assert!(
            error.to_string().contains("Invalid `mastodon` entry"),
            "{}",
            error
        );

        assert!(links(&[("email", "not-an-email")]).is_err());
    }
}
