title = 'Syntax Highlighting'
date = '2024 03 12'
tags = ['code']

#####

Code blocks are highlighted at build time using the giallo themes named in the theme's `theme.toml`. Switch your OS between light and dark mode to see both themes.

A short example appears in the excerpt on the index, because it comes before the `##more##` marker:

```rust
fn main() {
    println!("Shown in the excerpt");
}
```

##more##

## Rust

```rust
use std::collections::HashMap;

fn main() {
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for word in "the quick brown fox jumps over the lazy dog the end".split_whitespace() {
        *counts.entry(word).or_default() += 1;
    }
    println!("{:?}", counts.get("the"));
}
```

## TOML with line numbers and highlighted lines

```toml,linenos,hl_lines=2-3
[highlight]
light = "github-light"
dark = "github-dark"
```

## Shell

```sh
cargo build && scripts/tophat.sh --port 4000
```

## HTML escaping

```html
<script>alert("this should be shown, not run");</script>
```

## Unknown language

This falls back to plain text and prints a warning during publish:

```nosuchlang
just some text
```

## Indented code block

    indented blocks are rendered as plain text
