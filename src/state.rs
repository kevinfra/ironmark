use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::markdown::render_markdown_safe;

/// Safety cap for Markdown file size (32 MiB) to prevent memory exhaustion on accidental binary drops.
const MAX_FILE_SIZE_BYTES: u64 = 32 * 1024 * 1024;

#[derive(Debug, Clone, Serialize)]
pub struct DocumentPayload {
    pub has_file: bool,
    pub file_name: String,
    pub file_path: String,
    pub html: String,
    pub allow_remote_images: bool,
    pub blocked_remote_images: usize,
    pub mermaid_blocks: usize,
    pub error: Option<String>,
}

#[derive(Debug)]
pub struct AppState {
    pub current_file: Option<PathBuf>,
    pub current_dir: Option<PathBuf>,
    pub allow_remote_images: bool,
    pub payload: DocumentPayload,
}

impl AppState {
    pub fn new(initial_file: Option<PathBuf>, allow_remote_images: bool) -> Self {
        let mut state = Self {
            current_file: None,
            current_dir: None,
            allow_remote_images,
            payload: empty_payload(allow_remote_images),
        };
        if let Some(path) = initial_file {
            state.load_file(&path);
        }
        state
    }

    fn fail_load(&mut self, path: &Path, message: String) -> bool {
        self.current_file = None;
        self.current_dir = None;
        self.payload = error_payload(path, self.allow_remote_images, message);
        false
    }

    pub fn load_file(&mut self, path: &Path) -> bool {
        let canonical = match fs::canonicalize(path) {
            Ok(p) => p,
            Err(err) => {
                return self.fail_load(path, format!("File could not be read: {err}"));
            }
        };

        let meta = match fs::metadata(&canonical) {
            Ok(m) => m,
            Err(err) => {
                return self.fail_load(&canonical, format!("File could not be read: {err}"));
            }
        };

        if !meta.is_file() {
            return self.fail_load(
                &canonical,
                "File could not be read: selected path is not a regular file.".to_string(),
            );
        }

        if meta.len() > MAX_FILE_SIZE_BYTES {
            return self.fail_load(
                &canonical,
                format!(
                    "File could not be read: file is too large ({:.1} MiB). Maximum supported size is 32 MiB.",
                    meta.len() as f64 / (1024.0 * 1024.0)
                ),
            );
        }

        let content = match fs::read_to_string(&canonical) {
            Ok(c) => c,
            Err(err) => {
                return self.fail_load(&canonical, format!("File could not be read as UTF-8 text: {err}"));
            }
        };

        let rendered = render_markdown_safe(&content, self.allow_remote_images);
        let file_name = canonical
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Untitled.md".to_string());

        self.current_dir = canonical.parent().map(Path::to_path_buf);
        self.current_file = Some(canonical.clone());
        self.payload = DocumentPayload {
            has_file: true,
            file_name,
            file_path: canonical.to_string_lossy().into_owned(),
            html: rendered.html,
            allow_remote_images: self.allow_remote_images,
            blocked_remote_images: rendered.blocked_remote_images,
            mermaid_blocks: rendered.mermaid_blocks,
            error: None,
        };
        true
    }

    pub fn reload_current(&mut self) -> bool {
        if let Some(path) = self.current_file.clone() {
            self.load_file(&path)
        } else {
            self.payload = empty_payload(self.allow_remote_images);
            false
        }
    }

    pub fn set_allow_remote_images(&mut self, allow: bool) {
        self.allow_remote_images = allow;
        self.reload_current();
    }

    pub fn window_title(&self) -> String {
        if self.payload.has_file {
            format!("{} — ironmark", self.payload.file_name)
        } else {
            "ironmark".to_string()
        }
    }
}

fn empty_payload(allow_remote_images: bool) -> DocumentPayload {
    DocumentPayload {
        has_file: false,
        file_name: String::new(),
        file_path: String::new(),
        html: String::new(),
        allow_remote_images,
        blocked_remote_images: 0,
        mermaid_blocks: 0,
        error: None,
    }
}

fn error_payload(path: &Path, allow_remote_images: bool, message: String) -> DocumentPayload {
    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned());
    DocumentPayload {
        has_file: false,
        file_name,
        file_path: path.to_string_lossy().into_owned(),
        html: String::new(),
        allow_remote_images,
        blocked_remote_images: 0,
        mermaid_blocks: 0,
        error: Some(message),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_load_clears_previous_file_state() {
        let dir = std::env::temp_dir().join("ironmark_state_test");
        let _ = fs::create_dir_all(&dir);
        let valid_file = dir.join("valid.md");
        fs::write(&valid_file, "# Hello").unwrap();

        let mut state = AppState::new(Some(valid_file.clone()), false);
        assert!(state.current_file.is_some());
        assert!(state.current_dir.is_some());
        assert!(state.payload.has_file);

        let missing_file = dir.join("does_not_exist.md");
        assert!(!state.load_file(&missing_file));
        assert!(state.current_file.is_none());
        assert!(state.current_dir.is_none());
        assert!(!state.payload.has_file);
        assert!(state
            .payload
            .error
            .as_deref()
            .unwrap_or("")
            .contains("could not be read"));

        let _ = fs::remove_file(&valid_file);
        let _ = fs::remove_dir(&dir);
    }
}
