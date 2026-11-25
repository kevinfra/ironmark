# Contributing to ironmark

Thanks for your interest in contributing to `ironmark`!

## Development Setup

1. Install the [Rust toolchain](https://rustup.rs/) (stable).
2. Install platform WebView dependencies listed in `README.md`.
3. Run the app on the demo document:
   ```bash
   cargo run -- examples/demo.md
   ```

## Checks Before Opening a PR

Please make sure tests and lints pass locally:

```bash
cargo test
cargo clippy --all-targets
cargo build --release
```

## Project Layout

- `src/main.rs` — CLI parsing, window/WebView lifecycle, IPC & navigation handling
- `src/protocol.rs` — `ironmark://` custom protocol, CSP headers, static & local image serving
- `src/markdown.rs` — Markdown rendering (`pulldown-cmark`) and HTML sanitization (`ammonia`)
- `src/state.rs` — Document state management and file loading
- `src/watcher.rs` — Read-only file watcher for auto-reload
- `assets/` — Viewer frontend (`style.css`, `viewer.js`) and gzipped offline Mermaid bundle (`mermaid.min.js.gz`)

## License

By contributing, you agree that your contributions will be licensed under the MIT License (see `LICENSE`).
