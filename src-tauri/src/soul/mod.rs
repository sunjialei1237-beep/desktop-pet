//! Soul layer: Reflection, Internal Monologue, and Memory Consolidation.
//!
//! Design principles:
//! - Principle 1: LLM proposes persona updates, Rust writes them. LLM never touches DB directly.
//! - Principle 8: Cost-conscious. Uses reflection_model (cheapest), runs at most daily.
//! - Principle 6: Can be disabled without breaking the app.
//! - Principle 11: Every reflection records trigger + reason + thought for explainability.

pub mod consolidation;
pub mod monologue;
pub mod reflection;
pub mod review;
pub mod ritual;

pub use consolidation::{consolidate, lifecycle_cleanup};
pub use monologue::surface_thoughts;
pub use reflection::{run_reflection, ReflectionResult, ReflectionTrigger};
pub use review::{maybe_run_review_if_due, run_review, ReviewResult};
pub use ritual::{generate_goodmorning, goodmorning_canned, mark_goodmorning_done, should_run_goodmorning};
