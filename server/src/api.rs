//! REST API. Endpoint names map 1:1 to the Tauri commands used by the desktop
//! frontend (`timer_toggle`, `settings_get`, …) so the web IPC shim can stay a
//! thin translation layer.

use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;

use crate::controller::emit;
use crate::controller::TimerSnapshot;
use crate::db::queries;
use crate::settings::{self, Settings};
use crate::state::AppState;
use crate::themes::{self, Theme};

type ApiError = (StatusCode, String);
type ApiResult<T> = Result<Json<T>, ApiError>;

fn internal<E: std::fmt::Display>(e: E) -> ApiError {
    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}

// ---------------------------------------------------------------------------
// Timer
// ---------------------------------------------------------------------------

pub async fn get_timer(State(s): State<Arc<AppState>>) -> Json<TimerSnapshot> {
    Json(s.timer.get_snapshot())
}

pub async fn timer_toggle(State(s): State<Arc<AppState>>) -> StatusCode {
    s.timer.toggle();
    StatusCode::NO_CONTENT
}

pub async fn timer_reset(State(s): State<Arc<AppState>>) -> StatusCode {
    s.timer.reset();
    StatusCode::NO_CONTENT
}

pub async fn timer_restart_round(State(s): State<Arc<AppState>>) -> StatusCode {
    s.timer.restart_round();
    StatusCode::NO_CONTENT
}

pub async fn timer_skip(State(s): State<Arc<AppState>>) -> StatusCode {
    s.timer.skip();
    StatusCode::NO_CONTENT
}

// ---------------------------------------------------------------------------
// Settings
// ---------------------------------------------------------------------------

pub async fn get_settings(State(s): State<Arc<AppState>>) -> ApiResult<Settings> {
    let conn = s.db.lock().map_err(internal)?;
    settings::load(&conn).map(Json).map_err(internal)
}

#[derive(Deserialize)]
pub struct SetSetting {
    pub key: String,
    pub value: String,
}

pub async fn set_setting(
    State(s): State<Arc<AppState>>,
    Json(body): Json<SetSetting>,
) -> ApiResult<Settings> {
    let new = {
        let conn = s.db.lock().map_err(internal)?;
        settings::save_setting(&conn, &body.key, &body.value).map_err(internal)?;
        // Turning off the tray must cascade-reset dependent flags (desktop parity).
        if body.key == "tray_icon_enabled" && body.value == "false" {
            let _ = settings::save_setting(&conn, "min_to_tray", "false");
            let _ = settings::save_setting(&conn, "min_to_tray_on_close", "false");
        }
        settings::load(&conn).map_err(internal)?
    };

    s.timer.apply_settings(new.clone());
    s.timer.broadcast_reset_snapshot();
    emit(
        &s.events,
        "settings:changed",
        serde_json::to_value(&new).unwrap_or(serde_json::Value::Null),
    );
    Ok(Json(new))
}

pub async fn reset_settings(State(s): State<Arc<AppState>>) -> ApiResult<Settings> {
    let new = {
        let conn = s.db.lock().map_err(internal)?;
        conn.execute("DELETE FROM settings", [])
            .map_err(internal)?;
        settings::seed_defaults(&conn).map_err(internal)?;
        settings::load(&conn).map_err(internal)?
    };

    s.timer.apply_settings(new.clone());
    s.timer.broadcast_reset_snapshot();
    emit(
        &s.events,
        "settings:changed",
        serde_json::to_value(&new).unwrap_or(serde_json::Value::Null),
    );
    Ok(Json(new))
}

// ---------------------------------------------------------------------------
// Themes
// ---------------------------------------------------------------------------

pub async fn get_themes(State(s): State<Arc<AppState>>) -> Json<Vec<Theme>> {
    Json(themes::list_all(&s.themes_dir, &s.custom_themes_dir))
}

// ---------------------------------------------------------------------------
// Sessions + stats
// ---------------------------------------------------------------------------

pub async fn clear_sessions(State(s): State<Arc<AppState>>) -> Result<StatusCode, ApiError> {
    {
        let conn = s.db.lock().map_err(internal)?;
        conn.execute("DELETE FROM sessions", []).map_err(internal)?;
    }
    emit(&s.events, "sessions:cleared", serde_json::Value::Null);
    Ok(StatusCode::NO_CONTENT)
}

#[derive(serde::Serialize)]
pub struct DetailedStats {
    today: queries::DailyStats,
    week: Vec<queries::DayStat>,
    streak: queries::StreakInfo,
}

pub async fn stats_detailed(State(s): State<Arc<AppState>>) -> ApiResult<DetailedStats> {
    let conn = s.db.lock().map_err(internal)?;
    Ok(Json(DetailedStats {
        today: queries::get_daily_stats(&conn).map_err(internal)?,
        week: queries::get_weekly_stats(&conn).map_err(internal)?,
        streak: queries::get_streak(&conn).map_err(internal)?,
    }))
}

#[derive(serde::Serialize)]
pub struct HeatmapStats {
    entries: Vec<queries::HeatmapEntry>,
    total_rounds: u32,
    total_hours: u32,
    longest_streak: u32,
}

pub async fn stats_heatmap(State(s): State<Arc<AppState>>) -> ApiResult<HeatmapStats> {
    let conn = s.db.lock().map_err(internal)?;
    let raw = queries::get_all_time_stats(&conn).map_err(internal)?;
    let streak = queries::get_streak(&conn).map_err(internal)?;
    Ok(Json(HeatmapStats {
        entries: queries::get_heatmap_data(&conn).map_err(internal)?,
        total_rounds: raw.completed_work_sessions as u32,
        total_hours: (raw.total_work_secs / 3600) as u32,
        longest_streak: streak.longest,
    }))
}

// ---------------------------------------------------------------------------
// Misc
// ---------------------------------------------------------------------------

pub async fn version() -> Json<&'static str> {
    Json(env!("CARGO_PKG_VERSION"))
}
