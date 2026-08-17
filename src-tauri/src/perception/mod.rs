//! Perception layer: local-only system awareness.
//!
//! Principle 5: Body runs independently. Perception feeds Body (circadian, attention)
//! and Mind (Behavior Planner) without depending on LLM.
//! Principle 6: Each perception layer can be disabled independently.
//! Privacy: window titles are never stored in the DB. They live in process
//! memory (environment observer) and may enter LLM context ONLY through the
//! relevance-gated [Environment] section (plan 2026-08-17 §2.4).

pub mod presence;
pub mod time;
pub mod window;
pub mod cursor;
pub mod focus;
pub mod title;
pub mod environment;

pub use presence::PresenceState;
pub use time::TimeOfDay;
pub use window::AppCategory;

/// Combined perception snapshot, consumed by Body and Mind.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PerceptionSnapshot {
    pub time_of_day: TimeOfDay,
    pub since_last_interaction_secs: u64,
    pub presence: PresenceState,
    pub active_app: Option<String>,
    pub app_category: AppCategory,
    pub continuous_work_secs: u64,
    pub is_deep_focus: bool,
    /// Raw foreground window title (environment observer, plan P1).
    pub window_title: Option<String>,
    /// Best-effort file being edited / page being viewed (title parse).
    pub active_file_hint: Option<String>,
    /// Best-effort project/folder name (editor titles only).
    pub active_project_hint: Option<String>,
}

impl Default for PerceptionSnapshot {
    fn default() -> Self {
        Self {
            time_of_day: TimeOfDay::Morning,
            since_last_interaction_secs: 0,
            presence: PresenceState::Active,
            active_app: None,
            app_category: AppCategory::Other,
            continuous_work_secs: 0,
            is_deep_focus: false,
            window_title: None,
            active_file_hint: None,
            active_project_hint: None,
        }
    }
}
