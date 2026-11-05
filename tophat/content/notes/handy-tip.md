title = 'A Handy Tip'
date = '2024 02 01'
tags = ['notes']
image = 'images/silver.svg'
image_alt = 'A silver badge reading rgent'

#####

This post lives in `content/notes/`, so its URL should be `/notes/a-handy-tip/`.

The image below lives at `content/notes/images/silver.svg` and is referenced with a relative path, `images/silver.svg`. rgent copies it to the output and rewrites the URL, so it works even though this post is published under `/notes/a-handy-tip/`:

![Silver rgent badge](images/silver.svg)

Files can be linked the same way: [download the cheatsheet](cheatsheet.txt).

This post also uses the badge as its featured image via `image = 'images/silver.svg'` in the frontmatter, so it appears above the title and on the index.
