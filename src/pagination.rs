use std::path::{Path, PathBuf};

use serde::Serialize;

const PAGE_DIR: &str = "page";
const INDEX_FILE: &str = "index.html";

#[derive(Serialize, Debug, PartialEq, Eq)]
pub struct PageLink {
    pub number: usize,
    pub url: String,
    pub current: bool,
}

#[derive(Serialize, Debug, PartialEq, Eq)]
pub struct IndexPage<'a, T> {
    pub posts: &'a [T],
    pub current_page: usize,
    pub total_pages: usize,
    pub previous_page: Option<String>,
    pub next_page: Option<String>,
    pub pages: Vec<PageLink>,
}

pub fn paginate<T>(items: &[T], per_page: u64) -> Vec<IndexPage<'_, T>> {
    let per_page = match usize::try_from(per_page) {
        Ok(0) | Err(_) => items.len().max(1),
        Ok(n) => n,
    };

    let chunks: Vec<&[T]> = if items.is_empty() {
        vec![items]
    } else {
        items.chunks(per_page).collect()
    };
    let total_pages = chunks.len();

    chunks
        .into_iter()
        .enumerate()
        .map(|(i, posts)| {
            let current_page = i + 1;
            IndexPage {
                posts,
                current_page,
                total_pages,
                previous_page: (current_page > 1).then(|| page_url(current_page - 1)),
                next_page: (current_page < total_pages).then(|| page_url(current_page + 1)),
                pages: (1..=total_pages)
                    .map(|number| PageLink {
                        number,
                        url: page_url(number),
                        current: number == current_page,
                    })
                    .collect(),
            }
        })
        .collect()
}

pub fn page_url(number: usize) -> String {
    if number <= 1 {
        "/".into()
    } else {
        format!("/{}/{}/", PAGE_DIR, number)
    }
}

pub fn page_output_path(output_dir: &Path, number: usize) -> PathBuf {
    if number <= 1 {
        output_dir.join(INDEX_FILE)
    } else {
        output_dir
            .join(PAGE_DIR)
            .join(number.to_string())
            .join(INDEX_FILE)
    }
}

#[cfg(test)]
mod test {
    use crate::pagination::*;

    #[test]
    fn test_paginate_splits_items_into_pages() {
        let items = [1, 2, 3, 4, 5];

        let pages = paginate(&items, 2);

        assert_eq!(3, pages.len());
        assert_eq!(&[1, 2], pages[0].posts);
        assert_eq!(&[3, 4], pages[1].posts);
        assert_eq!(&[5], pages[2].posts);
        assert!(pages.iter().all(|page| page.total_pages == 3));
        assert_eq!(
            vec![1, 2, 3],
            pages
                .iter()
                .map(|page| page.current_page)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_paginate_links_neighbouring_pages() {
        let items = [1, 2, 3, 4, 5];

        let pages = paginate(&items, 2);

        assert_eq!(None, pages[0].previous_page);
        assert_eq!(Some("/page/2/".into()), pages[0].next_page);
        assert_eq!(Some("/".into()), pages[1].previous_page);
        assert_eq!(Some("/page/3/".into()), pages[1].next_page);
        assert_eq!(Some("/page/2/".into()), pages[2].previous_page);
        assert_eq!(None, pages[2].next_page);

        assert_eq!(
            vec![
                PageLink {
                    number: 1,
                    url: "/".into(),
                    current: false
                },
                PageLink {
                    number: 2,
                    url: "/page/2/".into(),
                    current: true
                },
                PageLink {
                    number: 3,
                    url: "/page/3/".into(),
                    current: false
                },
            ],
            pages[1].pages
        );
    }

    #[test]
    fn test_paginate_without_items_yields_one_empty_page() {
        let items: [u8; 0] = [];

        let pages = paginate(&items, 10);

        assert_eq!(1, pages.len());
        assert!(pages[0].posts.is_empty());
        assert_eq!(1, pages[0].total_pages);
        assert_eq!(None, pages[0].previous_page);
        assert_eq!(None, pages[0].next_page);
    }

    #[test]
    fn test_paginate_with_zero_per_page_puts_everything_on_one_page() {
        let items = [1, 2, 3];

        let pages = paginate(&items, 0);

        assert_eq!(1, pages.len());
        assert_eq!(&[1, 2, 3], pages[0].posts);
    }

    #[test]
    fn test_paginate_with_exact_multiple() {
        let items = [1, 2, 3, 4];

        let pages = paginate(&items, 2);

        assert_eq!(2, pages.len());
        assert_eq!(None, pages[1].next_page);
    }

    #[test]
    fn test_page_output_path() {
        let output = Path::new("/site/output");

        assert_eq!(output.join("index.html"), page_output_path(output, 1));
        assert_eq!(
            output.join("page/2/index.html"),
            page_output_path(output, 2)
        );
    }
}
