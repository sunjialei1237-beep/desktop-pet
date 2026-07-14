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
            Some("wo...ganggang you dian zou shen...".to_string()),
        )
    } else if lower.contains("network") || lower.contains("connection") {
        (
            "dazed",
            Some("xin hao bu tai hao ne...".to_string()),
        )
    } else if lower.contains("auth") || lower.contains("401") || lower.contains("403") {
        (
            "confused",
            Some("(mi yue hao xiang bu dui...)".to_string()),
        )
    } else if lower.contains("rate") || lower.contains("429") {
        (
            "tired",
            Some("shuo le hao duo hua, rang wo chuan kou qi ba~".to_string()),
        )
    } else if lower.contains("not configured") {
        (
            "confused",
            Some("(hai mei you peizhi hao lian jie...)".to_string()),
        )
    } else {
        ("dazed", Some("e...wo zan shi mei fan ying guo lai".to_string()))
    };

    let _ = app.emit("animation-command", serde_json::json!({ "state": animation }));

    if let Some(ref msg) = message {
        let _ = app.emit("bubble-show", msg.clone());
    }

    message
}
