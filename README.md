# ironmark

Lightweight, fast, secure, **read-only** Markdown viewer (no editor), written in Rust, with full **100% offline Mermaid** support for **Linux, macOS, and Windows**.

## Features

- **Read-only, zero editor**: opens `.md` files instantly without bloat or Electron (uses the OS native WebView via `tao` + `wry`).
- **Secure rendering (defense in depth)**:
  - CommonMark + GFM parsing (tables, task lists, footnotes, strikethrough, heading slugs) with `pulldown-cmark`.
  - Strict HTML sanitization in Rust with `ammonia` (strips `<script>`, `<iframe>`, `<svg>`, `<style>`, `<form>`, `on*` attributes, and `javascript:` / `vbscript:` / `file:` / `data:` URLs).
  - Custom `ironmark://` protocol (no open local ports) with a strict `Content-Security-Policy` header (`script-src 'self'`, `object-src 'none'`, `frame-src 'none'`).
  - Relative local images served from `/local/*` with canonical path verification inside the document directory (no path traversal) and `Content-Security-Policy: sandbox`.
  - External navigation blocked inside the WebView; `http(s)://` / `mailto:` links open in the system browser and relative links to other `.md` files open directly in `ironmark`.
- **100% offline Mermaid**: `mermaid.min.js` embedded in the binary (gzip-compressed, no CDN) running with `securityLevel: 'strict'`. Click any diagram to expand and zoom.
- **Offline privacy by default (optional remote images)**: `http(s)://` images are blocked by default in both sanitization and CSP. You can optionally enable them with `--allow-remote` or from the **Offline** button in the top bar.
- **Auto-reload on disk**: watches the open file (including atomic saves from Vim, VS Code, Helix, etc.) and updates the DOM without flicker while preserving scroll position.
- **Drag & drop + Native dialog + CLI**: open files from the terminal, by dragging them onto the window, or with `Cmd/Ctrl+O`.
- **Light / Dark mode**: automatically follows the system theme with a manual selector (`Auto` / `Light` / `Dark`) that also re-renders Mermaid diagrams.

## Installation

### macOS (Homebrew)

```bash
brew install kevinfra/tap/ironmark
```

### Prebuilt Binaries

Download the latest release archive for macOS, Linux, or Windows from [GitHub Releases](https://github.com/kevinfra/ironmark/releases).

## Usage

```bash
# Open a Markdown file
ironmark README.md

# Open allowing remote http(s) images
ironmark --allow-remote README.md

# Open an empty window (to drag & drop or pick a file)
ironmark
```

### Keyboard Shortcuts

| Shortcut | Action |
| :--- | :--- |
| `Cmd/Ctrl + O` | Open `.md` file |
| `Cmd/Ctrl + F` | Find in document |
| `Cmd/Ctrl + R` | Reload manually |
| `Cmd/Ctrl + +` / `-` / `0` | Zoom in / out / 100% |
| `Cmd/Ctrl + Q` | Quit |
| `Esc` | Close search or diagram modal |

## Building

### System Requirements

- **macOS**: Xcode Command Line Tools (`WKWebView` included in the OS).
- **Windows**: MSVC toolchain + WebView2 Runtime (included in Windows 10/11).
- **Linux**: GTK 3 and WebKitGTK:
  - Debian / Ubuntu:
    ```bash
    sudo apt install libgtk-3-dev libwebkit2gtk-4.1-dev libsoup-3.0-dev
    ```
  - Fedora:
    ```bash
    sudo dnf install gtk3-devel webkit2gtk4.1-devel libsoup3-devel
    ```
  - Arch Linux:
    ```bash
    sudo pacman -S gtk3 webkit2gtk-4.1
    ```

### Optimized Build (Release)

```bash
cargo build --release
```

The compiled binary is placed at `target/release/ironmark` (`target\release\ironmark.exe` on Windows).

### Try with the Example File

```bash
cargo run -- examples/demo.md
```

## Contributing

Contributions are welcome. See [CONTRIBUTING.md](CONTRIBUTING.md) for instructions on building, running tests, and submitting pull requests.

## License and Third-Party Notices

`ironmark` is distributed under the [MIT](LICENSE) license.

Third-party license notices (including the offline Mermaid bundle with version and SHA-256 hash) are documented in [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
