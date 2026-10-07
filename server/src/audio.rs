//! Custom alert-sound management.
//!
//! Files are uploaded by the browser (the web equivalent of the desktop file
//! picker) and stored under `<data_dir>/audio/`. The browser plays the custom
//! sound by fetching `/media/audio/<cue>`, so this module only handles storage,
//! listing, deletion and serving.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Multipart, Path as AxumPath, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;

use crate::settings;
use crate::state::AppState;

const STEM_WORK: &str = "custom_work_alert";
const STEM_SHORT: &str = "custom_short_break_alert";
const STEM_LONG: &str = "custom_long_break_alert";

/// Serialisable snapshot of custom audio file names. `null` = built-in sound.
#[derive(serde::Serialize)]
pub struct CustomAudioInfo {
    pub work_alert: Option<String>,
    pub short_break_alert: Option<String>,
    pub long_break_alert: Option<String>,
}

fn cue_stem(cue: &str) -> Option<&'static str> {
    match cue {
        "work_alert" => Some(STEM_WORK),
        "short_break_alert" => Some(STEM_SHORT),
        "long_break_alert" => Some(STEM_LONG),
        _ => None,
    }
}

fn cue_name_key(cue: &str) -> Option<&'static str> {
    match cue {
        "work_alert" => Some("custom_work_alert_name"),
        "short_break_alert" => Some("custom_short_break_alert_name"),
        "long_break_alert" => Some("custom_long_break_alert_name"),
        _ => None,
    }
}

fn audio_dir(state: &AppState) -> PathBuf {
    state.data_dir.join("audio")
}

/// Find the stored file for a stem (there is at most one, regardless of ext).
fn find_file(dir: &Path, stem: &str) -> Option<PathBuf> {
    let entries = std::fs::read_dir(dir).ok()?;
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.file_stem().and_then(|s| s.to_str()) == Some(stem) {
            return Some(path);
        }
    }
    None
}

fn remove_files(dir: &Path, stem: &str) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.file_stem().and_then(|s| s.to_str()) == Some(stem) {
                let _ = std::fs::remove_file(&path);
            }
        }
    }
}

fn mime_for(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()) {
        Some("wav") => "audio/wav",
        Some("ogg") => "audio/ogg",
        Some("oga") => "audio/ogg",
        Some("flac") => "audio/flac",
        Some("m4a") => "audio/mp4",
        _ => "audio/mpeg",
    }
}

fn internal<E: std::fmt::Display>(e: E) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}

/// GET /api/audio — active custom sounds with their original filenames.
pub async fn info(State(state): State<Arc<AppState>>) -> Json<CustomAudioInfo> {
    let dir = audio_dir(&state);
    let names = |cue: &str, stem: &str| -> Option<String> {
        find_file(&dir, stem)?;
        let key = cue_name_key(cue)?;
        let stored = state
            .db
            .lock()
            .ok()
            .and_then(|conn| settings::get_setting(&conn, key));
        Some(stored.unwrap_or_else(|| stem.to_string()))
    };

    Json(CustomAudioInfo {
        work_alert: names("work_alert", STEM_WORK),
        short_break_alert: names("short_break_alert", STEM_SHORT),
        long_break_alert: names("long_break_alert", STEM_LONG),
    })
}

/// POST /api/audio/{cue} — upload a custom sound (multipart field `file`).
pub async fn upload(
    State(state): State<Arc<AppState>>,
    AxumPath(cue): AxumPath<String>,
    mut multipart: Multipart,
) -> Result<Json<String>, (StatusCode, String)> {
    let stem = cue_stem(&cue)
        .ok_or_else(|| (StatusCode::BAD_REQUEST, format!("unknown cue: {cue}")))?;

    let mut bytes: Option<Vec<u8>> = None;
    let mut filename = String::from("custom.mp3");

    while let Some(field) = multipart.next_field().await.map_err(internal)? {
        if field.name() == Some("file") {
            filename = field
                .file_name()
                .map(|s| s.to_string())
                .unwrap_or_else(|| "custom.mp3".to_string());
            bytes = Some(field.bytes().await.map_err(internal)?.to_vec());
        }
    }

    let bytes = bytes.ok_or_else(|| (StatusCode::BAD_REQUEST, "missing file".to_string()))?;
    let ext = Path::new(&filename)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("mp3");

    let dir = audio_dir(&state);
    std::fs::create_dir_all(&dir).map_err(internal)?;
    remove_files(&dir, stem);
    std::fs::write(dir.join(format!("{stem}.{ext}")), &bytes).map_err(internal)?;

    if let Some(key) = cue_name_key(&cue) {
        if let Ok(conn) = state.db.lock() {
            let _ = settings::save_setting(&conn, key, &filename);
        }
    }

    log::info!("[audio] custom sound set cue={cue} file={filename}");
    Ok(Json(filename))
}

/// DELETE /api/audio/{cue} — remove the custom sound (revert to built-in).
pub async fn clear(
    State(state): State<Arc<AppState>>,
    AxumPath(cue): AxumPath<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    let stem = cue_stem(&cue)
        .ok_or_else(|| (StatusCode::BAD_REQUEST, format!("unknown cue: {cue}")))?;

    remove_files(&audio_dir(&state), stem);

    if let Some(key) = cue_name_key(&cue) {
        if let Ok(conn) = state.db.lock() {
            let _ = conn.execute("DELETE FROM settings WHERE key = ?1", [key]);
        }
    }
    Ok(StatusCode::NO_CONTENT)
}

/// GET /media/audio/{cue} — stream the stored custom sound to the browser.
pub async fn serve(
    State(state): State<Arc<AppState>>,
    AxumPath(cue): AxumPath<String>,
) -> Response {
    let Some(stem) = cue_stem(&cue) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Some(path) = find_file(&audio_dir(&state), stem) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    match std::fs::read(&path) {
        Ok(bytes) => {
            let mime = mime_for(&path);
            ([(header::CONTENT_TYPE, mime)], Body::from(bytes)).into_response()
        }
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}
