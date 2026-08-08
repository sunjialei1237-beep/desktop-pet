//! Life Loop: the pet's heartbeat.
//! Three timer frequencies per design doc 7.12:
//!   Medium (30s): emotion homeostasis + pending event check
//!   Slow (hourly): memory decay + relationship drift
//! The fast (1s) loop is handled by the frontend animation layer.

pub mod firstrun;
pub mod loop_runner;
pub mod recovery;
pub mod scheduler;

pub use firstrun::run_firstrun_checks;
pub use loop_runner::start_life_loop;
pub use recovery::handle_llm_error;
pub use scheduler::{snapshot as scheduler_snapshot, JobStat};
