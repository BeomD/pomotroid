//! Theme loading. Themes are plain JSON files (`{ "name", "colors" }`) read
//! from the bundled themes directory and a user-writable custom directory.
//! Unlike the desktop app, files are read from disk at runtime rather than
//! embedded with `include_str!`, so new themes can be dropped in without a
//! rebuild.

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Theme {
    pub name: String,
    /// CSS custom property values keyed by full property name (e.g. "--color-background").
    pub colors: HashMap<String, String>,
    /// True for user-created themes.
    #[serde(default)]
    pub is_custom: bool,
}

/// Read every `*.json` file in `dir` as a theme. Bad files are skipped.
pub fn load_dir(dir: &Path, is_custom: bool) -> Vec<Theme> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };

    let mut themes = Vec::new();
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        match std::fs::read_to_string(&path) {
            Ok(raw) => match serde_json::from_str::<Theme>(&raw) {
                Ok(mut theme) => {
                    theme.is_custom = is_custom;
                    themes.push(theme);
                }
                Err(e) => log::warn!("[themes] invalid theme {path:?}: {e}"),
            },
            Err(e) => log::warn!("[themes] cannot read {path:?}: {e}"),
        }
    }
    themes
}

/// Bundled themes first, then custom themes. A custom theme with the same name
/// as a bundled one overrides it.
pub fn list_all(bundled_dir: &Path, custom_dir: &Path) -> Vec<Theme> {
    let mut themes = load_dir(bundled_dir, false);
    for custom in load_dir(custom_dir, true) {
        if let Some(existing) = themes.iter_mut().find(|t| t.name == custom.name) {
            *existing = custom;
        } else {
            themes.push(custom);
        }
    }
    themes
}
