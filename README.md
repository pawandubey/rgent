# 🜛 rgent

> Pronounced "argent", like silver.

🚧 WIP. A static site generator. Config-compatible with https://github.com/pawandubey/griffin.

## Status

Current focus is on bringing to feature-parity with Griffin. Tracked in https://github.com/pawandubey/rgent/issues/1.

## Development

To try out the latest changes on a throwaway tophat blog:

```sh
scripts/tophat.sh
```

This builds rgent, generates a blog from the sample content in `tophat/content/`, and previews it at http://localhost:9090. The blog is deleted when you stop the preview with Ctrl+C. Pass `--keep` to keep it around for inspection, or `--port PORT` to use a different port.

Edit the files in `tophat/content/` to exercise new features.

## License

Licensed under Apache 2.0. © Pawan Dubey.
