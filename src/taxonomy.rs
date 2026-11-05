use std::collections::BTreeMap;

use anyhow::{bail, Result};
use serde::Serialize;

pub const TAGS_DIR: &str = "tags";

#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
pub struct TagLink {
    pub name: String,
    pub slug: String,
    pub url: String,
}

impl TagLink {
    pub fn new(name: &str) -> Self {
        let slug = slugify(name);
        TagLink {
            name: name.trim().to_string(),
            url: tag_url(&slug),
            slug,
        }
    }
}

#[derive(Debug)]
pub struct TagGroup<'a, T> {
    pub tag: TagLink,
    pub posts: Vec<&'a T>,
}

pub fn slugify(tag: &str) -> String {
    tag.trim()
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

pub fn tag_url(slug: &str) -> String {
    format!("/{}/{}/", TAGS_DIR, slug)
}

pub fn group_by_tag<'a, T>(
    posts: &[&'a T],
    tags_of: impl Fn(&T) -> &[String],
) -> Result<Vec<TagGroup<'a, T>>> {
    let mut groups: BTreeMap<String, TagGroup<'a, T>> = BTreeMap::new();

    for &post in posts {
        let mut seen_in_post = Vec::new();
        for tag in tags_of(post) {
            let link = TagLink::new(tag);
            if link.slug.is_empty() {
                bail!(
                    "Tag {:?} has no letters or digits, so it cannot be turned into a URL",
                    tag
                );
            }
            if seen_in_post.contains(&link.slug) {
                continue;
            }
            seen_in_post.push(link.slug.clone());

            match groups.get_mut(&link.slug) {
                Some(group) => {
                    if normalize(&group.tag.name) != normalize(&link.name) {
                        bail!(
                            "Tags {:?} and {:?} would both be published at {}. Rename one of them.",
                            group.tag.name,
                            link.name,
                            link.url
                        );
                    }
                    group.posts.push(post);
                }
                None => {
                    groups.insert(
                        link.slug.clone(),
                        TagGroup {
                            tag: link,
                            posts: vec![post],
                        },
                    );
                }
            }
        }
    }

    Ok(groups.into_values().collect())
}

fn normalize(name: &str) -> String {
    name.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

#[cfg(test)]
mod test {
    use crate::taxonomy::*;

    #[derive(Debug)]
    struct Post {
        title: &'static str,
        tags: Vec<String>,
    }

    fn post(title: &'static str, tags: &[&str]) -> Post {
        Post {
            title,
            tags: tags.iter().map(|t| t.to_string()).collect(),
        }
    }

    fn titles<T>(group: &TagGroup<T>, title: impl Fn(&T) -> &str) -> Vec<String> {
        group.posts.iter().map(|p| title(p).to_string()).collect()
    }

    #[test]
    fn test_slugify() {
        assert_eq!("rust", slugify("Rust"));
        assert_eq!("web-development", slugify("  Web   Development "));
        assert_eq!("c", slugify("C++"));
        assert_eq!("node-js", slugify("node.js"));
        assert_eq!("café", slugify("Café"));
        assert_eq!("", slugify("+++"));
    }

    #[test]
    fn test_tag_link() {
        assert_eq!(
            TagLink {
                name: "Web Dev".into(),
                slug: "web-dev".into(),
                url: "/tags/web-dev/".into(),
            },
            TagLink::new(" Web Dev ")
        );
    }

    #[test]
    fn test_group_by_tag_keeps_post_order_and_sorts_tags() {
        let a = post("A", &["rust", "web"]);
        let b = post("B", &["web"]);
        let c = post("C", &["rust"]);
        let posts = vec![&a, &b, &c];

        let groups = group_by_tag(&posts, |p| &p.tags).unwrap();

        assert_eq!(
            vec!["rust", "web"],
            groups
                .iter()
                .map(|g| g.tag.slug.as_str())
                .collect::<Vec<_>>()
        );
        assert_eq!(vec!["A", "C"], titles(&groups[0], |p| p.title));
        assert_eq!(vec!["A", "B"], titles(&groups[1], |p| p.title));
    }

    #[test]
    fn test_group_by_tag_merges_case_and_whitespace_variants() {
        let a = post("A", &["Web Dev"]);
        let b = post("B", &["web  dev", "WEB DEV"]);
        let posts = vec![&a, &b];

        let groups = group_by_tag(&posts, |p| &p.tags).unwrap();

        assert_eq!(1, groups.len());
        assert_eq!("Web Dev", groups[0].tag.name);
        assert_eq!(vec!["A", "B"], titles(&groups[0], |p| p.title));
    }

    #[test]
    fn test_group_by_tag_rejects_colliding_tags() {
        let a = post("A", &["c"]);
        let b = post("B", &["c++"]);
        let posts = vec![&a, &b];

        let error = group_by_tag(&posts, |p| &p.tags).unwrap_err();

        assert!(
            error
                .to_string()
                .contains("Tags \"c\" and \"c++\" would both be published at /tags/c/"),
            "{}",
            error
        );
    }

    #[test]
    fn test_group_by_tag_rejects_tags_without_letters() {
        let a = post("A", &["+++"]);
        let posts = vec![&a];

        assert!(group_by_tag(&posts, |p| &p.tags).is_err());
    }
}
