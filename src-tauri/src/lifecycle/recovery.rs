//! Error recovery: LLM failures are characterized as pet behaviors.
//! Design doc 7.11: the user should never see system errors.

use tauri::{AppHandle, Emitter};

/// Handles an LLM error by producing a character-appropriate response.
/// Returns the message to display (if any).
pub fn handle_llm_error(err: &str, app: &AppHandle) -> Option<String> {
    let lower = err.to_lowercase();

    let (animation, message) = if lower.contains("timeout") {
        (
            "dazed",
            Some("我……刚刚有点走神……".to_string()),
        )
    } else if lower.contains("network") || lower.contains("connection") {
        (
            "dazed",
            Some("信号不太好呢……".to_string()),
        )
    } else if lower.contains("auth") || lower.contains("401") || lower.contains("403") {
        (
            "confused",
            Some("（密钥好像不对……）".to_string()),
        )
    } else if lower.contains("rate") || lower.contains("429") {
        (
            "tired",
            Some("说了好多话，让我喘口气吧~".to_string()),
        )
    } else if lower.contains("not configured") {
        (
            "confused",
            Some("（还没有配置好连接……）".to_string()),
        )
    } else {
        ("dazed", Some("额……我暂时没反应过来".to_string()))
    };

    let _ = app.emit("animation-command", serde_json::json!({ "state": animation }));

    if let Some(ref msg) = message {
        let _ = app.emit("bubble-show", msg.clone());
    }

    message
}
