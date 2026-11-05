title = 'Testing Pagination'
date = '2024 02 20'
tags = ['tophat', 'rgent']

#####

The tophat script sets `postsperindex = 3`, so the index is split across two pages. It should be the first post on page 2.

This post has no `##more##` marker, so its excerpt on the index is the first 255 characters of its text, cut at a word boundary. This extra sentence makes the post long enough to be truncated, so the index should show an ellipsis and a "Read more" link for it.
