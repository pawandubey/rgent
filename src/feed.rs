use anyhow::{Context, Result};
use chrono::NaiveDate;
use rss::{
    extension::atom::{AtomExtension, Link},
    Category, ChannelBuilder, GuidBuilder, Item, ItemBuilder,
};

pub const FEED_FILE: &str = "feed.xml";
pub const FEED_ITEMS: usize = 20;

pub struct FeedChannel<'a> {
    pub title: &'a str,
    pub link: &'a str,
    pub description: &'a str,
    pub feed_url: &'a str,
}

pub struct FeedItem<'a> {
    pub title: &'a str,
    pub link: &'a str,
    pub description: &'a str,
    pub content: &'a str,
    pub published_on: NaiveDate,
    pub tags: &'a [String],
}

pub fn render_rss(channel: &FeedChannel, items: &[FeedItem]) -> Result<String> {
    let rss_items: Vec<Item> = items.iter().map(rss_item).collect();
    let last_build_date = items
        .iter()
        .map(|item| item.published_on)
        .max()
        .map(rfc2822);

    let self_link = Link {
        href: channel.feed_url.to_string(),
        rel: "self".into(),
        mime_type: Some("application/rss+xml".into()),
        ..Default::default()
    };

    let rss_channel = ChannelBuilder::default()
        .title(channel.title)
        .link(channel.link)
        .description(channel.description)
        .generator(Some(format!("rgent {}", env!("CARGO_PKG_VERSION"))))
        .last_build_date(last_build_date)
        .atom_ext(Some(AtomExtension {
            links: vec![self_link],
        }))
        .items(rss_items)
        .build();

    let bytes = rss_channel
        .pretty_write_to(Vec::new(), b' ', 2)
        .context("Failed to write RSS feed")?;
    String::from_utf8(bytes).context("RSS feed is not valid UTF-8")
}

fn rss_item(item: &FeedItem) -> Item {
    ItemBuilder::default()
        .title(Some(item.title.to_string()))
        .link(Some(item.link.to_string()))
        .guid(Some(
            GuidBuilder::default()
                .value(item.link)
                .permalink(true)
                .build(),
        ))
        .pub_date(Some(rfc2822(item.published_on)))
        .description(Some(item.description.to_string()))
        .content(Some(item.content.to_string()))
        .categories(
            item.tags
                .iter()
                .map(|tag| Category {
                    name: tag.clone(),
                    domain: None,
                })
                .collect::<Vec<_>>(),
        )
        .build()
}

fn rfc2822(date: NaiveDate) -> String {
    date.and_hms_opt(0, 0, 0)
        .expect("Midnight is always a valid time")
        .and_utc()
        .to_rfc2822()
}

#[cfg(test)]
mod test {
    use crate::feed::*;

    fn channel() -> FeedChannel<'static> {
        FeedChannel {
            title: "My <Site>",
            link: "https://example.com/",
            description: "A tagline",
            feed_url: "https://example.com/feed.xml",
        }
    }

    #[test]
    fn test_render_rss_with_items() {
        let tags = vec!["rust".to_string(), "web".to_string()];
        let items = [
            FeedItem {
                title: "Newer & better",
                link: "https://example.com/newer/",
                description: "<p>Short</p>",
                content: "<p>Full <em>post</em></p>",
                published_on: NaiveDate::from_ymd_opt(2024, 3, 15).unwrap(),
                tags: &tags,
            },
            FeedItem {
                title: "Older",
                link: "https://example.com/older/",
                description: "<p>Old</p>",
                content: "<p>Old post</p>",
                published_on: NaiveDate::from_ymd_opt(2024, 1, 5).unwrap(),
                tags: &[],
            },
        ];

        let xml = render_rss(&channel(), &items).expect("Failed to render feed");
        let parsed: rss::Channel = xml.parse().expect("Feed should be valid RSS");

        assert!(xml.starts_with("<?xml"), "{}", xml);
        assert!(
            xml.contains("xmlns:content=\"http://purl.org/rss/1.0/modules/content/\""),
            "{}",
            xml
        );
        assert!(
            xml.contains("<atom:link href=\"https://example.com/feed.xml\" rel=\"self\" type=\"application/rss+xml\"/>"),
            "{}",
            xml
        );
        assert_eq!("My <Site>", parsed.title());
        assert_eq!("https://example.com/", parsed.link());
        assert_eq!("A tagline", parsed.description());
        assert_eq!(
            Some("Fri, 15 Mar 2024 00:00:00 +0000"),
            parsed.last_build_date()
        );

        let first = &parsed.items()[0];
        assert_eq!(Some("Newer & better"), first.title());
        assert_eq!(Some("https://example.com/newer/"), first.link());
        assert_eq!(
            Some("https://example.com/newer/"),
            first.guid().map(|g| g.value())
        );
        assert!(first.guid().unwrap().is_permalink());
        assert_eq!(Some("Fri, 15 Mar 2024 00:00:00 +0000"), first.pub_date());
        assert_eq!(Some("<p>Short</p>"), first.description());
        assert_eq!(Some("<p>Full <em>post</em></p>"), first.content());
        assert_eq!(
            vec!["rust", "web"],
            first
                .categories()
                .iter()
                .map(|c| c.name())
                .collect::<Vec<_>>()
        );
        assert_eq!(2, parsed.items().len());
    }

    #[test]
    fn test_render_rss_without_items() {
        let xml = render_rss(&channel(), &[]).expect("Failed to render feed");
        let parsed: rss::Channel = xml.parse().expect("Feed should be valid RSS");

        assert!(parsed.items().is_empty());
        assert_eq!(None, parsed.last_build_date());
    }
}
