//! Server-side Pomodoro timer controller.
//!
//! Mirrors `src-tauri/src/timer/mod.rs` but replaces the Tauri event emitter,
//! tray, audio and window logic with a `tokio::sync::broadcast` channel that
//! feeds the WebSocket endpoint. The actual countdown runs on the same
//! drift-correcting engine thread used by the desktop app.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;
use serde_json::json;
use tokio::sync::broadcast;

use crate::db::{self, queries};
use crate::engine::{self, EngineHandle, TimerCommand, TimerEvent};
use crate::sequence::{RoundType, SequenceState};
use crate::settings::Settings;

/// Full timer state snapshot. Serialised as the WebSocket payload and returned
/// by `GET /api/timer`. Field names mirror the frontend `TimerState` type.
#[derive(Debug, Clone, Serialize)]
pub struct TimerSnapshot {
    /// "work" | "short-break" | "long-break"
    pub round_type: String,
    /// Round type active before this one. Empty string on the first round.
    pub previous_round_type: String,
    pub elapsed_secs: u32,
    pub total_secs: u32,
    pub is_running: bool,
    /// True if started then paused (elapsed > 0, not running).
    pub is_paused: bool,
    pub work_round_number: u32,
    pub work_rounds_total: u32,
    /// Monotonically-increasing focus round count since last reset.
    pub session_work_count: u32,
}

struct TimerShared {
    elapsed_secs: u32,
    is_running: bool,
}

pub struct TimerController {
    engine: EngineHandle,
    sequence: Arc<Mutex<SequenceState>>,
    settings: Arc<Mutex<Settings>>,
    shared: Arc<Mutex<TimerShared>>,
    events: broadcast::Sender<String>,
}

impl TimerController {
    pub fn new(settings: Settings, db: db::DbState, events: broadcast::Sender<String>) -> Self {
        let seq = SequenceState::new(settings.long_break_interval);
        let duration = seq.current_duration_secs(&settings);

        let (engine, event_rx) = engine::spawn(duration, Duration::from_secs(1));

        let sequence = Arc::new(Mutex::new(seq));
        let settings_arc = Arc::new(Mutex::new(settings));
        let shared = Arc::new(Mutex::new(TimerShared {
            elapsed_secs: 0,
            is_running: false,
        }));

        let seq_t = Arc::clone(&sequence);
        let settings_t = Arc::clone(&settings_arc);
        let shared_t = Arc::clone(&shared);
        let engine_t = engine.clone();
        let events_t = events.clone();

        std::thread::Builder::new()
            .name("timer-events".to_string())
            .spawn(move || {
                listen_events(event_rx, seq_t, settings_t, shared_t, engine_t, db, events_t);
            })
            .expect("failed to spawn timer event listener");

        Self {
            engine,
            sequence,
            settings: settings_arc,
            shared,
            events,
        }
    }

    // --- Commands ---

    /// Start a fresh timer if idle, resume if paused, pause if running.
    pub fn toggle(&self) {
        let s = self.shared.lock().unwrap();
        if s.is_running {
            self.engine.send(TimerCommand::Pause);
        } else if s.elapsed_secs > 0 {
            self.engine.send(TimerCommand::Resume);
        } else {
            self.engine.send(TimerCommand::Start);
        }
    }

    pub fn reset(&self) {
        self.sequence.lock().unwrap().reset();
        self.engine.send(TimerCommand::Reset);
    }

    /// Restart the current round from zero without advancing the sequence.
    pub fn restart_round(&self) {
        self.engine.send(TimerCommand::Reset);
    }

    pub fn skip(&self) {
        self.engine.send(TimerCommand::Skip);
    }

    /// Reconfigure the engine with the current round's duration.
    pub fn reconfigure(&self) {
        let duration = {
            let seq = self.sequence.lock().unwrap();
            let settings = self.settings.lock().unwrap();
            seq.current_duration_secs(&settings)
        };
        self.engine
            .send(TimerCommand::Reconfigure { duration_secs: duration });
    }

    // --- Query ---

    pub fn get_snapshot(&self) -> TimerSnapshot {
        build_snapshot(&self.sequence, &self.settings, &self.shared)
    }

    /// Broadcast a `timer:reset` frame carrying the current snapshot. Used after
    /// settings changes so every client reconciles durations immediately.
    pub fn broadcast_reset_snapshot(&self) {
        let snap = self.get_snapshot();
        emit(
            &self.events,
            "timer:reset",
            serde_json::to_value(&snap).unwrap_or(serde_json::Value::Null),
        );
    }

    /// Apply new settings. Updates the in-memory copy and, when idle, reconfigures
    /// the engine so the next Start uses the new duration.
    pub fn apply_settings(&self, new: Settings) {
        self.sequence.lock().unwrap().work_rounds_total = new.long_break_interval;
        *self.settings.lock().unwrap() = new;
        let is_idle = {
            let s = self.shared.lock().unwrap();
            !s.is_running && s.elapsed_secs == 0
        };
        if is_idle {
            self.reconfigure();
        }
    }
}

// ---------------------------------------------------------------------------
// Background event listener thread
// ---------------------------------------------------------------------------

fn listen_events(
    event_rx: std::sync::mpsc::Receiver<TimerEvent>,
    sequence: Arc<Mutex<SequenceState>>,
    settings: Arc<Mutex<Settings>>,
    shared: Arc<Mutex<TimerShared>>,
    engine: EngineHandle,
    db: db::DbState,
    events: broadcast::Sender<String>,
) {
    let mut current_session_id: Option<i64> = None;

    while let Ok(event) = event_rx.recv() {
        match event {
            TimerEvent::Started { total_secs } => {
                shared.lock().unwrap().is_running = true;
                emit(&events, "timer:started", json!({ "total_secs": total_secs }));
            }

            TimerEvent::Tick {
                elapsed_secs,
                total_secs,
            } => {
                {
                    let mut s = shared.lock().unwrap();
                    s.elapsed_secs = elapsed_secs;
                    s.is_running = true;
                }
                emit(
                    &events,
                    "timer:tick",
                    json!({ "elapsed_secs": elapsed_secs, "total_secs": total_secs }),
                );

                // Start a session row on the first tick of a new round.
                if elapsed_secs == 1 && current_session_id.is_none() {
                    let rt = sequence.lock().unwrap().current_round.as_str().to_string();
                    let total = {
                        let seq = sequence.lock().unwrap();
                        let s = settings.lock().unwrap();
                        seq.current_duration_secs(&s)
                    };
                    if let Ok(conn) = db.lock() {
                        match queries::insert_session(&conn, &rt, total) {
                            Ok(id) => current_session_id = Some(id),
                            Err(e) => log::error!("[timer] failed to record session: {e}"),
                        }
                    }
                }
            }

            TimerEvent::Complete { skipped } => {
                // Mark the completed round.
                if let Some(session_id) = current_session_id.take() {
                    if let Ok(conn) = db.lock() {
                        let _ = queries::complete_session(&conn, session_id, !skipped);
                    }
                }

                let (next_round, next_duration, auto_start_work, auto_start_break) = {
                    let mut seq = sequence.lock().unwrap();
                    let s = settings.lock().unwrap();
                    let (rt, dur) = seq.advance(&s);
                    (rt, dur, s.auto_start_work, s.auto_start_break)
                };

                {
                    let mut s = shared.lock().unwrap();
                    s.elapsed_secs = 0;
                    s.is_running = false;
                }

                engine.send(TimerCommand::Prime {
                    duration_secs: next_duration,
                });

                let snapshot = build_snapshot(&sequence, &settings, &shared);
                emit(
                    &events,
                    "timer:round-change",
                    serde_json::to_value(&snapshot).unwrap_or(serde_json::Value::Null),
                );

                let should_auto = match next_round {
                    RoundType::Work => auto_start_work,
                    _ => auto_start_break,
                };
                if should_auto {
                    engine.send(TimerCommand::Start);
                }
            }

            TimerEvent::Paused { elapsed_secs } => {
                shared.lock().unwrap().is_running = false;
                emit(&events, "timer:paused", json!({ "elapsed_secs": elapsed_secs }));
            }

            TimerEvent::Resumed { elapsed_secs } => {
                shared.lock().unwrap().is_running = true;
                emit(&events, "timer:resumed", json!({ "elapsed_secs": elapsed_secs }));
            }

            TimerEvent::Reset => {
                current_session_id = None;
                {
                    let mut s = shared.lock().unwrap();
                    s.elapsed_secs = 0;
                    s.is_running = false;
                }
                let snapshot = build_snapshot(&sequence, &settings, &shared);
                emit(
                    &events,
                    "timer:reset",
                    serde_json::to_value(&snapshot).unwrap_or(serde_json::Value::Null),
                );

                let duration = {
                    let seq = sequence.lock().unwrap();
                    let s = settings.lock().unwrap();
                    seq.current_duration_secs(&s)
                };
                engine.send(TimerCommand::Prime {
                    duration_secs: duration,
                });
            }

            TimerEvent::Suspended { elapsed_secs } => {
                shared.lock().unwrap().is_running = false;
                emit(
                    &events,
                    "timer:suspended",
                    json!({ "elapsed_secs": elapsed_secs }),
                );
            }
        }
    }
}

fn build_snapshot(
    sequence: &Arc<Mutex<SequenceState>>,
    settings: &Arc<Mutex<Settings>>,
    shared: &Arc<Mutex<TimerShared>>,
) -> TimerSnapshot {
    let seq = sequence.lock().unwrap();
    let s = settings.lock().unwrap();
    let sh = shared.lock().unwrap();

    TimerSnapshot {
        round_type: seq.current_round.as_str().to_string(),
        previous_round_type: seq
            .previous_round
            .map(|r| r.as_str().to_string())
            .unwrap_or_default(),
        elapsed_secs: sh.elapsed_secs,
        total_secs: seq.current_duration_secs(&s),
        is_running: sh.is_running,
        is_paused: !sh.is_running && sh.elapsed_secs > 0,
        work_round_number: seq.work_round_number,
        work_rounds_total: seq.work_rounds_total,
        session_work_count: seq.session_work_count,
    }
}

/// Serialise `{ "event", "payload" }` and fan it out to all WebSocket clients.
pub fn emit(events: &broadcast::Sender<String>, event: &str, payload: serde_json::Value) {
    let frame = json!({ "event": event, "payload": payload }).to_string();
    // Err just means there are currently no subscribers.
    let _ = events.send(frame);
}
