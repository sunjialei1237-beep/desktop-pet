//! BrainState: the unified per-turn snapshot (Architecture Principle #2
//! "BrainState 统一快照").
//!
//! One borrowed handle bundling everything that is true about the pet + user
//! *this turn* — the shared inputs to the pure decision functions. Instead of
//! threading five loose references that callers must keep in sync, the pipeline
//! constructs this once and passes `&BrainState` to each consumer.
//!
//! Task #9 (2026-08-07) introduced `ConverseCtx` for converse's *external*
//! inputs (the 9 params into the turn). `BrainState` is the complementary
//! *internal* snapshot — the per-turn context that converse computes (emotion /
//! relationship / retrieval / pending) and feeds to its decision steps. This is
//! plan §A1's "global BrainState". The adoption boundary is **planner only**
//! (the flagship pure decision): the system-prompt builder and budget allocator
//! consume `(retrieval, emotion, intent)` — and `intent` is the planner's
//! *output*, so it cannot live in BrainState without a circular dependency,
//! while `text`/`relationship`/`pending_due` would be bundled unused. Extending
//! BrainState into them is the speculative mega-state this project has rejected.
//! The follow-up was investigated and deliberately closed; see ADR
//! `docs/decisions/2026-08-08-brainstate-prompt-budget.md`.

use crate::db::pending::PendingEvent;
use crate::db::relationship::Relationship;
use crate::emotion::state::EmotionState;
use crate::mind::retrieval::RetrievalResult;

/// Per-turn read-only context (Architecture #2). Every field is a borrow, so
/// building a `BrainState` is cheap pointer-copying — no cloning of the
/// underlying data, and the snapshot can't drift out of sync with its sources.
pub struct BrainState<'a> {
    /// The user's message this turn.
    pub text: &'a str,
    /// The pet's current emotion vector.
    pub emotion: &'a EmotionState,
    /// The current relationship (None early on).
    pub relationship: Option<&'a Relationship>,
    /// Pending events that are due (drives proactive follow-up).
    pub pending_due: &'a [PendingEvent],
    /// Retrieved memories for this turn.
    pub retrieval: &'a RetrievalResult,
}

impl<'a> BrainState<'a> {
    /// Construct the per-turn snapshot. One call site (converse) builds this;
    /// every decision function then borrows it.
    pub fn new(
        text: &'a str,
        emotion: &'a EmotionState,
        relationship: Option<&'a Relationship>,
        pending_due: &'a [PendingEvent],
        retrieval: &'a RetrievalResult,
    ) -> Self {
        BrainState {
            text,
            emotion,
            relationship,
            pending_due,
            retrieval,
        }
    }
}
