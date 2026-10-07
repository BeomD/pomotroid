//! Shared application state, stored in the axum router via `with_state`.

use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::broadcast;

use crate::controller::TimerController;
use crate::db;

pub struct AppState {
    /// Server-side timer engine + sequence. Single-user: one global instance.
    pub timer: Arc<TimerController>,
    /// Broadcast channel carrying pre-serialised `{ "event", "payload" }` JSON
    /// frames to every connected WebSocket client.
    pub events: broadcast::Sender<String>,
    /// SQLite handle (settings + session history).
    pub db: db::DbState,
    /// Directory where `pomotroid.db` and uploaded audio live.
    pub data_dir: PathBuf,
    /// Directory containing bundled theme JSON files (`static/themes`).
    pub themes_dir: PathBuf,
    /// Directory containing user-defined theme JSON files (`<data>/themes`).
    pub custom_themes_dir: PathBuf,
}
