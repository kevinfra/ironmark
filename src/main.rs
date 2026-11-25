#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

mod markdown;
mod protocol;
mod state;
mod watcher;

use std::env;
use std::future::Future;
use std::path::{Component, Path, PathBuf};
use std::pin::pin;
use std::process;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::task::{Context, Poll, Wake, Waker};
use std::thread::{self, Thread};

use percent_encoding::percent_decode_str;
use serde::Deserialize;
use tao::dpi::LogicalSize;
use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder};
use tao::window::WindowBuilder;
use wry::{DragDropEvent, WebViewBuilder};

use crate::protocol::handle_protocol_request;
use crate::state::AppState;
use crate::watcher::FileWatcher;

#[derive(Debug)]
pub enum UserEvent {
    OpenFile(PathBuf),
    OpenDialog,
    FileChanged,
    ToggleRemoteImages,
    OpenExternalUrl(String),
    OpenRelativeLink(String),
    DragHover(bool),
    Quit,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum IpcMessage {
    OpenDialog,
    ToggleRemoteImages,
    Reload,
    OpenExternal { url: String },
    OpenRelative { href: String },
    Quit,
}

struct CliArgs {
    file: Option<PathBuf>,
    allow_remote_images: bool,
}

fn parse_cli_args() -> CliArgs {
    let mut file: Option<PathBuf> = None;
    let mut allow_remote_images = false;

    for arg in env::args().skip(1) {
        match arg.as_str() {
            "-h" | "--help" => {
                println!(
                    "ironmark {} — Lightweight, fast, secure Markdown & Mermaid viewer\n\n\
                     USAGE:\n    ironmark [OPTIONS] [FILE]\n\n\
                     OPTIONS:\n    \
                     -r, --allow-remote    Allow loading remote http(s) images (blocked offline by default)\n    \
                     -h, --help            Print help information\n    \
                     -V, --version         Print version",
                    env!("CARGO_PKG_VERSION")
                );
                process::exit(0);
            }
            "-V" | "--version" => {
                println!("ironmark {}", env!("CARGO_PKG_VERSION"));
                process::exit(0);
            }
            "-r" | "--allow-remote" => {
                allow_remote_images = true;
            }
            other if !other.starts_with('-') => {
                if file.is_none() {
                    file = Some(PathBuf::from(other));
                }
            }
            unknown => {
                eprintln!("Unknown option: {unknown}\nRun `ironmark --help` for usage.");
                process::exit(2);
            }
        }
    }

    CliArgs {
        file,
        allow_remote_images,
    }
}

fn is_internal_origin(url: &str) -> bool {
    let Ok(uri) = url.parse::<wry::http::Uri>() else {
        return false;
    };
    let (Some(scheme), Some(authority)) = (uri.scheme_str(), uri.authority()) else {
        return false;
    };
    let auth = authority.as_str();
    (scheme.eq_ignore_ascii_case("ironmark") && auth.eq_ignore_ascii_case("localhost"))
        || ((scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https"))
            && auth.eq_ignore_ascii_case("ironmark.localhost"))
}

fn is_safe_external_url(url: &str) -> bool {
    let trimmed = url.trim();
    if trimmed.chars().any(|c| c.is_control()) {
        return false;
    }
    let lower = trimmed.to_ascii_lowercase();
    lower.starts_with("https://")
        || lower.starts_with("http://")
        || lower.starts_with("mailto:")
}

fn is_markdown_like(path: &Path) -> bool {
    match path.extension().and_then(|e| e.to_str()) {
        Some(ext) => matches!(
            ext.to_ascii_lowercase().as_str(),
            "md" | "markdown" | "mdown" | "mkd" | "mkdn" | "mdx" | "txt"
        ),
        None => false,
    }
}

fn resolve_relative_markdown(base_dir: &Path, href: &str) -> Option<PathBuf> {
    let stripped = href.split(['#', '?']).next().unwrap_or("").trim();
    if stripped.is_empty() {
        return None;
    }

    let decoded = percent_decode_str(stripped).decode_utf8().ok()?;
    let rel = Path::new(decoded.as_ref());
    if rel.is_absolute()
        || decoded.starts_with('/')
        || decoded.starts_with('\\')
        || rel
            .components()
            .any(|c| matches!(c, Component::RootDir | Component::Prefix(_)))
    {
        return None;
    }

    // Intentionally allows `../` so relative links to sibling/parent Markdown docs work.
    let candidate = base_dir.join(rel);
    if candidate.is_file() && is_markdown_like(&candidate) {
        Some(candidate)
    } else {
        None
    }
}

fn main() -> wry::Result<()> {
    let cli = parse_cli_args();
    let initial_file = cli.file.clone();

    let state = Arc::new(RwLock::new(AppState::new(
        initial_file.clone(),
        cli.allow_remote_images,
    )));

    let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    let proxy = event_loop.create_proxy();

    let initial_title = state
        .read()
        .map(|s| s.window_title())
        .unwrap_or_else(|_| "ironmark".to_string());

    let window = WindowBuilder::new()
        .with_title(initial_title)
        .with_inner_size(LogicalSize::new(980.0, 760.0))
        .with_min_inner_size(LogicalSize::new(480.0, 360.0))
        .build(&event_loop)
        .expect("failed to create application window");

    let mut watcher = FileWatcher::new(proxy.clone());
    if let Ok(guard) = state.read() {
        watcher.watch_file(guard.current_file.as_deref());
    }

    let protocol_state = Arc::clone(&state);
    let ipc_proxy = proxy.clone();
    let drop_proxy = proxy.clone();

    let builder = WebViewBuilder::new()
        .with_custom_protocol("ironmark".into(), move |_webview_id, request| {
            handle_protocol_request(&protocol_state, request)
        })
        .with_url("ironmark://localhost/")
        .with_navigation_handler(|url| is_internal_origin(&url))
        .with_new_window_req_handler(|_url, _features| wry::NewWindowResponse::Deny)
        .with_ipc_handler(move |req| {
            if !is_internal_origin(&req.uri().to_string()) {
                return;
            }
            let Ok(msg) = serde_json::from_str::<IpcMessage>(req.body()) else {
                return;
            };
            match msg {
                IpcMessage::OpenDialog => {
                    let _ = ipc_proxy.send_event(UserEvent::OpenDialog);
                }
                IpcMessage::ToggleRemoteImages => {
                    let _ = ipc_proxy.send_event(UserEvent::ToggleRemoteImages);
                }
                IpcMessage::Reload => {
                    let _ = ipc_proxy.send_event(UserEvent::FileChanged);
                }
                IpcMessage::OpenExternal { url } => {
                    let _ = ipc_proxy.send_event(UserEvent::OpenExternalUrl(url));
                }
                IpcMessage::OpenRelative { href } => {
                    let _ = ipc_proxy.send_event(UserEvent::OpenRelativeLink(href));
                }
                IpcMessage::Quit => {
                    let _ = ipc_proxy.send_event(UserEvent::Quit);
                }
            }
        })
        .with_drag_drop_handler(move |event| match event {
            DragDropEvent::Enter { .. } => {
                let _ = drop_proxy.send_event(UserEvent::DragHover(true));
                true
            }
            DragDropEvent::Over { .. } => true,
            DragDropEvent::Leave => {
                let _ = drop_proxy.send_event(UserEvent::DragHover(false));
                true
            }
            DragDropEvent::Drop { paths, .. } => {
                let _ = drop_proxy.send_event(UserEvent::DragHover(false));
                if let Some(path) = paths.into_iter().next() {
                    let _ = drop_proxy.send_event(UserEvent::OpenFile(path));
                }
                true
            }
            _ => false,
        });

    #[cfg(any(
        target_os = "windows",
        target_os = "macos",
        target_os = "ios",
        target_os = "android"
    ))]
    let webview = builder.build(&window)?;

    #[cfg(not(any(
        target_os = "windows",
        target_os = "macos",
        target_os = "ios",
        target_os = "android"
    )))]
    let webview = {
        use tao::platform::unix::WindowExtUnix;
        use wry::WebViewBuilderExtUnix;
        let vbox = window
            .default_vbox()
            .expect("GTK window must have a default vbox container");
        builder.build_gtk(vbox)?
    };

    let event_proxy = proxy.clone();
    let dialog_open = Arc::new(AtomicBool::new(false));

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;

        match event {
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                *control_flow = ControlFlow::Exit;
            }
            Event::Opened { urls } => {
                for url in urls {
                    if let Ok(path) = url.to_file_path() {
                        let _ = event_proxy.send_event(UserEvent::OpenFile(path));
                        break;
                    }
                }
            }
            Event::UserEvent(user_event) => match user_event {
                UserEvent::OpenFile(path) => {
                    let (title, watched_path) = {
                        let mut guard = state.write().unwrap_or_else(|e| e.into_inner());
                        guard.load_file(&path);
                        (guard.window_title(), guard.current_file.clone())
                    };
                    watcher.watch_file(watched_path.as_deref());
                    window.set_title(&title);
                    let _ = webview.evaluate_script("window.__ironmarkReload && window.__ironmarkReload(false);");
                }
                UserEvent::OpenDialog => {
                    if dialog_open
                        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                        .is_err()
                    {
                        return;
                    }

                    let start_dir = state
                        .read()
                        .ok()
                        .and_then(|s| s.current_dir.clone())
                        .filter(|p| !p.as_os_str().is_empty() && p.is_dir());

                    let mut dialog = rfd::AsyncFileDialog::new()
                        .set_title("Open Markdown File")
                        .set_parent(&window)
                        .add_filter(
                            "Markdown",
                            &["md", "markdown", "mdown", "mkd", "mdx", "txt"],
                        );
                    if let Some(dir) = start_dir {
                        dialog = dialog.set_directory(dir);
                    }

                    // Initiates the native non-blocking sheet/dialog without blocking tao's event loop.
                    let future = dialog.pick_file();
                    let proxy_for_dialog = event_proxy.clone();
                    let dialog_flag = Arc::clone(&dialog_open);

                    thread::spawn(move || {
                        let picked = block_on_future(future);
                        dialog_flag.store(false, Ordering::SeqCst);
                        if let Some(handle) = picked {
                            let _ = proxy_for_dialog
                                .send_event(UserEvent::OpenFile(handle.path().to_path_buf()));
                        }
                    });
                }
                UserEvent::FileChanged => {
                    let title = {
                        let mut guard = state.write().unwrap_or_else(|e| e.into_inner());
                        guard.reload_current();
                        guard.window_title()
                    };
                    window.set_title(&title);
                    let _ = webview.evaluate_script("window.__ironmarkReload && window.__ironmarkReload(true);");
                }
                UserEvent::ToggleRemoteImages => {
                    {
                        let mut guard = state.write().unwrap_or_else(|e| e.into_inner());
                        let next = !guard.allow_remote_images;
                        guard.set_allow_remote_images(next);
                    }
                    // Hard reload so the browser applies the updated HTTP Content-Security-Policy header.
                    let _ = webview.evaluate_script("window.__ironmarkHardReload && window.__ironmarkHardReload();");
                }
                UserEvent::OpenExternalUrl(url) => {
                    if is_safe_external_url(&url) {
                        let _ = open::that_detached(url);
                    }
                }
                UserEvent::OpenRelativeLink(href) => {
                    let resolved = state
                        .read()
                        .ok()
                        .and_then(|s| s.current_dir.as_deref().and_then(|d| resolve_relative_markdown(d, &href)));
                    if let Some(path) = resolved {
                        let _ = event_proxy.send_event(UserEvent::OpenFile(path));
                    }
                }
                UserEvent::DragHover(active) => {
                    let script = if active {
                        "window.__ironmarkSetDragOver && window.__ironmarkSetDragOver(true);"
                    } else {
                        "window.__ironmarkSetDragOver && window.__ironmarkSetDragOver(false);"
                    };
                    let _ = webview.evaluate_script(script);
                }
                UserEvent::Quit => {
                    *control_flow = ControlFlow::Exit;
                }
            },
            _ => {}
        }
    });
}

struct ThreadWaker(Thread);

impl Wake for ThreadWaker {
    fn wake(self: Arc<Self>) {
        self.0.unpark();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.0.unpark();
    }
}

fn block_on_future<F: Future>(fut: F) -> F::Output {
    let mut fut = pin!(fut);
    let waker = Waker::from(Arc::new(ThreadWaker(thread::current())));
    let mut cx = Context::from_waker(&waker);
    loop {
        match fut.as_mut().poll(&mut cx) {
            Poll::Ready(val) => return val,
            Poll::Pending => thread::park(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn relative_markdown_allows_parent_dir_and_rejects_absolute_paths() {
        let root = std::env::temp_dir().join("ironmark_rel_md_test");
        let sub = root.join("docs");
        let _ = fs::create_dir_all(&sub);
        let parent_md = root.join("README.md");
        fs::write(&parent_md, "# Parent").unwrap();

        // `../` relative links are intentionally allowed for usability.
        assert!(resolve_relative_markdown(&sub, "../README.md").is_some());

        // Plain and percent-encoded absolute paths are rejected.
        let abs = parent_md.to_string_lossy().into_owned();
        assert!(resolve_relative_markdown(&sub, &abs).is_none());
        let encoded_abs = abs.replace('/', "%2F");
        assert!(resolve_relative_markdown(&sub, &encoded_abs).is_none());

        let _ = fs::remove_file(&parent_md);
        let _ = fs::remove_dir(&sub);
        let _ = fs::remove_dir(&root);
    }

    #[test]
    fn internal_origin_rejects_spoofed_hosts_and_userinfo() {
        assert!(is_internal_origin("ironmark://localhost/"));
        assert!(is_internal_origin("ironmark://localhost/api/document"));
        assert!(is_internal_origin("http://ironmark.localhost/"));
        assert!(is_internal_origin("https://ironmark.localhost/assets/style.css"));

        assert!(!is_internal_origin("http://ironmark.localhost@evil.com/"));
        assert!(!is_internal_origin("https://ironmark.localhost@example.com/"));
        assert!(!is_internal_origin("http://ironmark.localhost.evil.com/"));
        assert!(!is_internal_origin("https://ironmark.localhost:443@evil.com/"));
        assert!(!is_internal_origin("https://example.com/"));
        assert!(!is_internal_origin("javascript:alert(1)"));
    }

    #[test]
    fn external_url_rejects_control_characters() {
        assert!(is_safe_external_url("https://example.com/docs"));
        assert!(is_safe_external_url("mailto:user@example.com"));
        assert!(!is_safe_external_url("https://example.com/\nfoo"));
        assert!(!is_safe_external_url("https://example.com/\0foo"));
        assert!(!is_safe_external_url("file:///etc/passwd"));
    }
}

