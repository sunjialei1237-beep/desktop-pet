use serde::Serialize;
use tauri::{AppHandle, Emitter};

/// Emits a chat bubble message to the frontend.
pub fn emit_bubble_show(app: &AppHandle, text: &str) {
    let _ = app.emit("bubble-show", text);
}

/// Emits a bubble hide event.
pub fn emit_bubble_hide(app: &AppHandle) {
    let _ = app.emit("bubble-hide", ());
}

/// Emits an animation state change to the frontend.
pub fn emit_animation_command(app: &AppHandle, state: &str) {
    let _ = app.emit("animation-command", serde_json::json!({ "state": state }));
}

/// Emits an app status update (e.g. "thinking", "idle", "recovering").
pub fn emit_app_status(app: &AppHandle, status: &str) {
    let _ = app.emit("app-status", status);
}

/// Emits a download progress event (for model downloads).
pub fn emit_download_progress(app: &AppHandle, percent: f64) {
    #[derive(Clone, Serialize)]
    struct Payload {
        percent: f64,
    }
    let _ = app.emit("download-progress", Payload { percent });
}

/// Emits an emotion state update for the frontend to drive animations.
pub fn emit_emotion_update(app: &AppHandle, emotion_json: serde_json::Value) {
    let _ = app.emit("emotion-update", emotion_json);
}
