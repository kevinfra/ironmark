use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tao::event_loop::EventLoopProxy;

use crate::UserEvent;

const DEBOUNCE_WINDOW: Duration = Duration::from_millis(120);

/// Read-only file watcher that watches the parent directory of the active Markdown file.
/// Watching the parent directory ensures atomic saves (write temp file + rename, used by
/// vim, VS Code, Helix, etc.) reliably trigger a reload on macOS, Linux, and Windows.
pub struct FileWatcher {
    watcher: Option<RecommendedWatcher>,
    watched_dir: Option<PathBuf>,
    target_file: Arc<Mutex<Option<PathBuf>>>,
}

impl FileWatcher {
    pub fn new(proxy: EventLoopProxy<UserEvent>) -> Self {
        let target_file: Arc<Mutex<Option<PathBuf>>> = Arc::new(Mutex::new(None));
        let target_clone = Arc::clone(&target_file);
        let last_sent = Arc::new(Mutex::new(Instant::now() - DEBOUNCE_WINDOW));
        let last_sent_clone = Arc::clone(&last_sent);

        let watcher = notify::recommended_watcher(move |res: notify::Result<Event>| {
            let Ok(event) = res else { return };

            // Ignore pure metadata/access events to avoid noisy reloads.
            let is_relevant_kind = matches!(
                event.kind,
                EventKind::Modify(_) | EventKind::Create(_) | EventKind::Any
            );
            if !is_relevant_kind {
                return;
            }

            let target = {
                let guard = target_clone.lock().unwrap_or_else(|e| e.into_inner());
                match guard.as_ref() {
                    Some(p) => p.clone(),
                    None => return,
                }
            };

            let target_name = target.file_name();
            let matches_target = event.paths.iter().any(|p| {
                if p == &target {
                    return true;
                }
                // Handle canonicalization differences or atomic rename targets in the same folder.
                if let (Some(p_name), Some(t_name)) = (p.file_name(), target_name) {
                    if p_name == t_name {
                        return p.parent().and_then(|d| std::fs::canonicalize(d).ok())
                            == target.parent().and_then(|d| std::fs::canonicalize(d).ok());
                    }
                }
                false
            });

            if !matches_target {
                return;
            }

            let now = Instant::now();
            {
                let mut last = last_sent_clone.lock().unwrap_or_else(|e| e.into_inner());
                if now.duration_since(*last) < DEBOUNCE_WINDOW {
                    return;
                }
                *last = now;
            }

            let _ = proxy.send_event(UserEvent::FileChanged);
        })
        .ok();

        Self {
            watcher,
            watched_dir: None,
            target_file,
        }
    }

    pub fn watch_file(&mut self, path: Option<&Path>) {
        if let Some(prev_dir) = self.watched_dir.take() {
            if let Some(ref mut watcher) = self.watcher {
                let _ = watcher.unwatch(&prev_dir);
            }
        }

        let canonical = path.and_then(|p| std::fs::canonicalize(p).ok());
        {
            let mut guard = self.target_file.lock().unwrap_or_else(|e| e.into_inner());
            *guard = canonical.clone();
        }

        if let Some(file_path) = canonical {
            if let Some(parent) = file_path.parent() {
                if let Some(ref mut watcher) = self.watcher {
                    if watcher.watch(parent, RecursiveMode::NonRecursive).is_ok() {
                        self.watched_dir = Some(parent.to_path_buf());
                    }
                }
            }
        }
    }
}
