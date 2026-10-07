//! Pomotroid web server.
//!
//! A headless axum + tokio server that runs the Pomodoro timer on the backend,
//! persists settings and session history in SQLite, and pushes timer events to
//! the browser over WebSocket. The SvelteKit frontend (built as a static SPA)
//! is served from the same origin.
//!
//! Configuration via environment variables:
//!   POMOTROID_PORT         listen port                (default 6666)
//!   POMOTROID_DATA_DIR     SQLite + uploaded audio    (default ./data)
//!   POMOTROID_STATIC_DIR   built frontend assets      (default ./build)
//!   POMOTROID_THEMES_DIR   bundled theme JSON files    (default ./static/themes)

use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::extract::Request;
use axum::middleware::{self, Next};
use axum::response::Response;
use axum::routing::{get, post};
use axum::Router;
use tokio::sync::broadcast;
use tower_http::cors::CorsLayer;
use tower_http::services::{ServeDir, ServeFile};

mod api;
mod audio;
mod controller;
mod state;
mod themes;
mod ws;

// --- Shared, Tauri-free logic reused from the desktop crate via #[path] ---
#[path = "../../src-tauri/src/db/mod.rs"]
pub mod db;
#[path = "../../src-tauri/src/settings/mod.rs"]
pub mod settings;
#[path = "../../src-tauri/src/timer/engine.rs"]
pub mod engine;
#[path = "../../src-tauri/src/timer/sequence.rs"]
pub mod sequence;

use controller::TimerController;
use state::AppState;

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

#[tokio::main]
async fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let port: u16 = env_or("POMOTROID_PORT", "6666")
        .parse()
        .expect("POMOTROID_PORT must be a valid u16");
    let data_dir = PathBuf::from(env_or("POMOTROID_DATA_DIR", "./data"));
    let static_dir = PathBuf::from(env_or("POMOTROID_STATIC_DIR", "./build"));
    let themes_dir = PathBuf::from(env_or("POMOTROID_THEMES_DIR", "./static/themes"));

    std::fs::create_dir_all(&data_dir).expect("failed to create data dir");
    let custom_themes_dir = data_dir.join("themes");
    let _ = std::fs::create_dir_all(&custom_themes_dir);

    // --- Database ---
    let db = match db::open(&data_dir) {
        Ok(d) => d,
        Err(e) => {
            log::error!("failed to open database: {e}");
            std::process::exit(1);
        }
    };
    {
        let conn = db.lock().unwrap();
        settings::seed_defaults(&conn).expect("failed to seed default settings");
    }

    // --- Load settings once ---
    let initial_settings = {
        let conn = db.lock().unwrap();
        settings::load(&conn).expect("failed to load settings")
    };

    // --- Event broadcast channel + timer controller ---
    let (events, _rx) = broadcast::channel::<String>(256);
    let timer = Arc::new(TimerController::new(
        initial_settings,
        db.clone(),
        events.clone(),
    ));

    let state = Arc::new(AppState {
        timer,
        events,
        db,
        data_dir: data_dir.clone(),
        themes_dir: themes_dir.clone(),
        custom_themes_dir,
    });

    // --- Routes ---
    let index = static_dir.join("index.html");
    let app = Router::new()
        .route("/api/timer", get(api::get_timer))
        .route("/api/timer/toggle", post(api::timer_toggle))
        .route("/api/timer/reset", post(api::timer_reset))
        .route("/api/timer/restart", post(api::timer_restart_round))
        .route("/api/timer/skip", post(api::timer_skip))
        .route("/api/settings", get(api::get_settings).post(api::set_setting))
        .route("/api/settings/reset", post(api::reset_settings))
        .route("/api/themes", get(api::get_themes))
        .route("/api/sessions/clear", post(api::clear_sessions))
        .route("/api/stats/detailed", get(api::stats_detailed))
        .route("/api/stats/heatmap", get(api::stats_heatmap))
        .route("/api/version", get(api::version))
        .route("/api/audio", get(audio::info))
        .route(
            "/api/audio/{cue}",
            post(audio::upload).delete(audio::clear),
        )
        .route("/media/audio/{cue}", get(audio::serve))
        .route("/ws", get(ws::handler))
        // Static SPA with index.html fallback for client-side routes.
        .fallback_service(
            ServeDir::new(&static_dir).not_found_service(ServeFile::new(&index)),
        )
        .layer(CorsLayer::permissive())
        .layer(middleware::from_fn(log_requests))
        .with_state(state);

    let addr = format!("0.0.0.0:{port}");
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .unwrap_or_else(|e| panic!("failed to bind {addr}: {e}"));

    log::info!("Pomotroid web server listening on http://{addr}");
    log::info!("  data dir : {}", display(&data_dir));
    log::info!("  static   : {}", display(&static_dir));
    log::info!("  themes   : {}", display(&themes_dir));

    axum::serve(listener, app)
        .await
        .expect("server error");
}

fn display(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

/// Log every HTTP request path + method so frontend activity (button clicks)
/// is visible in the container logs.
async fn log_requests(req: Request, next: Next) -> Response {
    let method = req.method().clone();
    let path = req.uri().path().to_string();
    let response = next.run(req).await;
    log::info!("[http] {} {} -> {}", method, path, response.status().as_u16());
    response
}
