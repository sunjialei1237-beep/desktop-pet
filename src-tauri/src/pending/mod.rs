//! Pending events engine: tracks future plans and triggers proactive care.
//! Design doc 9.2: the core mechanism for "she remembers me".

pub mod proactive;
pub mod tracker;

pub use proactive::{trigger_proactive, PerceptionState, ProactiveAction};
pub use tracker::{check_due, expire_stale, increment_followup, mark_triggered, resolve};
