use std::borrow::Cow;
use std::fs;
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, OnceLock, RwLock};

use flate2::read::GzDecoder;
use percent_encoding::percent_decode_str;
use wry::http::{header, Request, Response, StatusCode};

use crate::state::AppState;

const STYLE_CSS: &[u8] = include_bytes!("../assets/style.css");
const VIEWER_JS: &[u8] = include_bytes!("../assets/viewer.js");
const MERMAID_JS_GZ: &[u8] = include_bytes!("../assets/mermaid.min.js.gz");

const MAX_IMAGE_SIZE_BYTES: u64 = 32 * 1024 * 1024;

static MERMAID_JS_CACHE: OnceLock<Vec<u8>> = OnceLock::new();

fn mermaid_js_bytes() -> &'static [u8] {
    MERMAID_JS_CACHE
        .get_or_init(|| {
            let mut decoder = GzDecoder::new(MERMAID_JS_GZ);
            let mut out = Vec::with_capacity(3_600_000);
            decoder
                .read_to_end(&mut out)
                .expect("embedded mermaid.min.js.gz must decompress cleanly");
            out
        })
        .as_slice()
}

const INDEX_HTML: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>ironmark</title>
  <link rel="stylesheet" href="/assets/style.css">
  <script defer src="/assets/mermaid.min.js"></script>
  <script defer src="/assets/viewer.js"></script>
</head>
<body>
  <header class="toolbar" role="toolbar" aria-label="Document controls">
    <div class="toolbar-left">
      <button id="btn-open" class="tool-btn" type="button" title="Open Markdown file (Cmd/Ctrl+O)">
        <svg viewBox="0 0 16 16" width="14" height="14" fill="currentColor" aria-hidden="true">
          <path d="M1.75 2.5A1.75 1.75 0 0 0 0 4.25v8.5C0 13.716.784 14.5 1.75 14.5h12.5A1.75 1.75 0 0 0 16 12.75v-6.5A1.75 1.75 0 0 0 14.25 4.5H7.5a.25.25 0 0 1-.2-.1l-.9-1.2C6.07 2.76 5.55 2.5 5 2.5H1.75Z"/>
        </svg>
        <span>Open</span>
      </button>
      <span id="file-label" class="file-label" title="">No file open</span>
      <span id="blocked-badge" class="blocked-badge hidden" title="Remote images were blocked in offline mode"></span>
    </div>
    <div class="toolbar-right">
      <button id="btn-find" class="tool-btn icon-only" type="button" title="Find in document (Cmd/Ctrl+F)">
        <svg viewBox="0 0 16 16" width="14" height="14" fill="currentColor" aria-hidden="true">
          <path d="M10.68 11.74a6 6 0 0 1-7.922-8.982 6 6 0 0 1 8.982 7.922l3.04 3.04a.749.749 0 0 1-.326 1.275.749.749 0 0 1-.734-.215l-3.04-3.04ZM11.5 7a4.499 4.499 0 1 0-8.997 0A4.499 4.499 0 0 0 11.5 7Z"/>
        </svg>
      </button>
      <button id="btn-remote" class="tool-btn" type="button" title="Toggle remote images (default: blocked offline)">
        <span id="remote-icon" aria-hidden="true"></span>
        <span id="remote-label">Offline</span>
      </button>
      <button id="btn-theme" class="tool-btn" type="button" title="Cycle theme: Auto / Light / Dark">
        <span id="theme-label">Auto</span>
      </button>
      <div class="zoom-group" role="group" aria-label="Zoom">
        <button id="btn-zoom-out" class="tool-btn icon-only" type="button" title="Zoom out (Cmd/Ctrl+-)">&minus;</button>
        <button id="btn-zoom-reset" class="tool-btn zoom-readout" type="button" title="Reset zoom (Cmd/Ctrl+0)">100%</button>
        <button id="btn-zoom-in" class="tool-btn icon-only" type="button" title="Zoom in (Cmd/Ctrl+=)">+</button>
      </div>
    </div>
  </header>

  <div id="find-bar" class="find-bar hidden" role="search">
    <input id="find-input" type="search" placeholder="Find in document..." autocomplete="off" spellcheck="false">
    <span id="find-count" class="find-count"></span>
    <button id="find-prev" class="tool-btn icon-only" type="button" title="Previous match (Shift+Enter)">&uarr;</button>
    <button id="find-next" class="tool-btn icon-only" type="button" title="Next match (Enter)">&darr;</button>
    <button id="find-close" class="tool-btn icon-only" type="button" title="Close (Esc)">&times;</button>
  </div>

  <div id="drop-overlay" class="drop-overlay hidden" aria-hidden="true">
    <div class="drop-card">Drop Markdown file to view</div>
  </div>

  <div id="mermaid-modal" class="mermaid-modal hidden" role="dialog" aria-modal="true" aria-label="Expanded Mermaid diagram">
    <div class="mermaid-modal-bar">
      <span class="mermaid-modal-hint">Scroll to pan &middot; Esc or click outside to close</span>
      <div class="zoom-group">
        <button id="modal-zoom-out" class="tool-btn icon-only" type="button" title="Zoom out">&minus;</button>
        <button id="modal-zoom-reset" class="tool-btn zoom-readout" type="button" title="Reset">100%</button>
        <button id="modal-zoom-in" class="tool-btn icon-only" type="button" title="Zoom in">+</button>
        <button id="modal-close" class="tool-btn icon-only" type="button" title="Close">&times;</button>
      </div>
    </div>
    <div id="mermaid-modal-stage" class="mermaid-modal-stage"></div>
  </div>

  <main id="viewport" class="viewport">
    <article id="content" class="markdown-body"></article>
    <section id="empty-state" class="empty-state hidden">
      <div class="empty-card">
        <div class="empty-icon" aria-hidden="true">M&#8595;</div>
        <h1>ironmark</h1>
        <p>Lightweight, fast, read-only Markdown &amp; Mermaid viewer.</p>
        <p class="empty-sub">Drop any <code>.md</code> file here or click <strong>Open</strong> (<code>Cmd/Ctrl+O</code>).</p>
        <button id="btn-open-empty" class="primary-btn" type="button">Open Markdown File</button>
      </div>
    </section>
    <section id="error-state" class="error-state hidden">
      <div class="error-card">
        <h2>Unable to open document</h2>
        <p id="error-message"></p>
      </div>
    </section>
  </main>
</body>
</html>
"#;

pub fn handle_protocol_request(
    state: &Arc<RwLock<AppState>>,
    request: Request<Vec<u8>>,
) -> Response<Cow<'static, [u8]>> {
    let path = request.uri().path();

    match path {
        "/" | "/index.html" => {
            let allow_remote = state
                .read()
                .map(|s| s.allow_remote_images)
                .unwrap_or(false);
            let csp = build_csp(allow_remote);
            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
                .header(header::CONTENT_SECURITY_POLICY, csp)
                .header(header::X_CONTENT_TYPE_OPTIONS, "nosniff")
                .header(header::REFERRER_POLICY, "no-referrer")
                .header(header::CACHE_CONTROL, "no-store")
                .body(Cow::Borrowed(INDEX_HTML.as_bytes()))
                .unwrap()
        }
        "/api/document" => {
            let json = match state.read() {
                Ok(guard) => serde_json::to_vec(&guard.payload).unwrap_or_else(|_| b"{}".to_vec()),
                Err(_) => b"{}".to_vec(),
            };
            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, "application/json; charset=utf-8")
                .header(header::X_CONTENT_TYPE_OPTIONS, "nosniff")
                .header(header::CACHE_CONTROL, "no-store")
                .body(Cow::Owned(json))
                .unwrap()
        }
        "/assets/style.css" => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "text/css; charset=utf-8")
            .header(header::X_CONTENT_TYPE_OPTIONS, "nosniff")
            .header(header::CACHE_CONTROL, "public, max-age=3600")
            .body(Cow::Borrowed(STYLE_CSS))
            .unwrap(),
        "/assets/viewer.js" => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "application/javascript; charset=utf-8")
            .header(header::X_CONTENT_TYPE_OPTIONS, "nosniff")
            .header(header::CACHE_CONTROL, "public, max-age=3600")
            .body(Cow::Borrowed(VIEWER_JS))
            .unwrap(),
        "/assets/mermaid.min.js" => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "application/javascript; charset=utf-8")
            .header(header::X_CONTENT_TYPE_OPTIONS, "nosniff")
            .header(header::CACHE_CONTROL, "public, max-age=86400")
            .body(Cow::Borrowed(mermaid_js_bytes()))
            .unwrap(),
        _ if path.starts_with("/local/") => serve_local_image(state, &path["/local/".len()..]),
        _ => text_response(StatusCode::NOT_FOUND, "Not found"),
    }
}

fn build_csp(allow_remote_images: bool) -> &'static str {
    if allow_remote_images {
        "default-src 'none'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data: https: http:; font-src 'self'; connect-src 'self'; frame-src 'none'; object-src 'none'; base-uri 'none'; form-action 'none'"
    } else {
        "default-src 'none'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; font-src 'self'; connect-src 'self'; frame-src 'none'; object-src 'none'; base-uri 'none'; form-action 'none'"
    }
}

fn serve_local_image(
    state: &Arc<RwLock<AppState>>,
    raw_rel_path: &str,
) -> Response<Cow<'static, [u8]>> {
    let base_dir = {
        let Ok(guard) = state.read() else {
            return text_response(StatusCode::INTERNAL_SERVER_ERROR, "State unavailable");
        };
        match guard.current_dir.clone() {
            Some(dir) => dir,
            None => return text_response(StatusCode::NOT_FOUND, "No active document"),
        }
    };

    let Some(resolved) = resolve_jailed_image_path(&base_dir, raw_rel_path) else {
        return text_response(StatusCode::FORBIDDEN, "Forbidden or invalid local image path");
    };

    let Some(mime) = allowed_image_mime(&resolved) else {
        return text_response(StatusCode::FORBIDDEN, "Unsupported file type");
    };

    let Ok(meta) = fs::metadata(&resolved) else {
        return text_response(StatusCode::NOT_FOUND, "Image not found");
    };
    if !meta.is_file() || meta.len() > MAX_IMAGE_SIZE_BYTES {
        return text_response(StatusCode::FORBIDDEN, "Invalid image file");
    }

    let Ok(bytes) = fs::read(&resolved) else {
        return text_response(StatusCode::NOT_FOUND, "Unable to read image");
    };

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime)
        .header(header::X_CONTENT_TYPE_OPTIONS, "nosniff")
        .header(header::CONTENT_SECURITY_POLICY, "sandbox")
        .header(header::CACHE_CONTROL, "no-store")
        .body(Cow::Owned(bytes))
        .unwrap()
}

/// Strictly resolves a percent-encoded relative path inside `base_dir`.
/// Rejects path traversal (`..`), absolute paths, null bytes, and symlinks pointing outside `base_dir`.
pub fn resolve_jailed_image_path(base_dir: &Path, raw_rel_path: &str) -> Option<PathBuf> {
    let decoded = percent_decode_str(raw_rel_path).decode_utf8().ok()?;
    if decoded.is_empty() || decoded.contains('\0') {
        return None;
    }

    let rel = Path::new(decoded.as_ref());
    let mut clean_rel = PathBuf::new();
    for comp in rel.components() {
        match comp {
            Component::Normal(part) => clean_rel.push(part),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => return None,
        }
    }

    if clean_rel.as_os_str().is_empty() {
        return None;
    }

    let canonical_base = fs::canonicalize(base_dir).ok()?;
    let candidate = canonical_base.join(clean_rel);
    let canonical_target = fs::canonicalize(candidate).ok()?;

    if canonical_target.starts_with(&canonical_base) {
        Some(canonical_target)
    } else {
        None
    }
}

fn allowed_image_mime(path: &Path) -> Option<&'static str> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    match ext.as_str() {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "gif" => Some("image/gif"),
        "webp" => Some("image/webp"),
        "svg" => Some("image/svg+xml"),
        "avif" => Some("image/avif"),
        "bmp" => Some("image/bmp"),
        "ico" => Some("image/x-icon"),
        _ => None,
    }
}

fn text_response(status: StatusCode, msg: &'static str) -> Response<Cow<'static, [u8]>> {
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
        .header(header::X_CONTENT_TYPE_OPTIONS, "nosniff")
        .body(Cow::Borrowed(msg.as_bytes()))
        .unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_path_traversal_and_absolute_paths() {
        let dir = std::env::temp_dir();
        assert!(resolve_jailed_image_path(&dir, "../etc/passwd").is_none());
        assert!(resolve_jailed_image_path(&dir, "%2e%2e/etc/passwd").is_none());
        assert!(resolve_jailed_image_path(&dir, "/etc/passwd").is_none());
        assert!(resolve_jailed_image_path(&dir, "foo\0bar.png").is_none());
    }

    #[test]
    fn allows_only_image_extensions() {
        assert_eq!(allowed_image_mime(Path::new("a.PNG")), Some("image/png"));
        assert_eq!(allowed_image_mime(Path::new("a.svg")), Some("image/svg+xml"));
        assert_eq!(allowed_image_mime(Path::new("a.html")), None);
        assert_eq!(allowed_image_mime(Path::new("a.js")), None);
        assert_eq!(allowed_image_mime(Path::new("a.md")), None);
    }

    #[test]
    fn csp_enforces_offline_by_default() {
        let offline = build_csp(false);
        assert!(offline.contains("img-src 'self' data:;"));
        assert!(!offline.contains("https:"));
        assert!(offline.contains("script-src 'self';"));

        let remote = build_csp(true);
        assert!(remote.contains("img-src 'self' data: https: http:;"));
        assert!(remote.contains("script-src 'self';"));
    }
}
