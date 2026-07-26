//! Follow-up question frequency control.
//!
//! A credit bucket + a back-to-back cooldown + light randomness. Goal: keep the
//! pet's "engage" follow-up questions from firing on every shared statement
//! (which felt like an interrogation), while NEVER asking twice in a row.
//!
//! Architecture principle #8: the planner stays a pure function. The throttling
//! decision lives here (also pure) and is APPLIED by the converse orchestration
//! layer, which owns the mutable pacing state and the RNG roll.
//!
//! == Frequency math (steady state, every turn is a "share") ==
//! State machine over (credit, last_turn_was_question). With FOLLOWUP_PROB = p:
//! With ASK_COST = 1 the only hard cap is the "no two questions in a row" rule
//! (ceiling ~50% as p -> 1). Empirically (deterministic 300k-turn LCG, see the
//! steady_state_rate_measured test): at p = 0.6 the rate is ~0.375 (target ~40%).
//! To make follow-ups DENSER, raise FOLLOWUP_PROB (toward the ~0.50 ceiling).
//! To make them RARER, lower it. The previous ASK_COST = 2 variant capped at
//! ~33% (credit rebuild added a third dead turn per cycle) and undershot the
//! user's ~40% target; ASK_COST = 1 was chosen to actually reach it.

/// Per-share probability of asking *when inside the available window*
/// (credit >= ASK_THRESHOLD and the previous turn was not a question).
/// This is the single knob for tuning question density.
pub const FOLLOWUP_PROB: f64 = 0.6;
/// Maximum credit the bucket can hold.
pub const CREDIT_CAP: u8 = 3;
/// Credit spent each time we actually ask a follow-up.
pub const ASK_COST: u8 = 1;
/// Minimum credit required before asking is even considered.
pub const ASK_THRESHOLD: u8 = 2;

/// In-memory pacing state. Resets on restart (cold-start pet is mildly curious),
/// which is the desired UX. Not persisted by design.
#[derive(Debug, Clone, Default)]
pub struct QuestionPacing {
    pub credit: u8,
    pub last_turn_was_question: bool,
}

impl QuestionPacing {
    /// Pure decision: given a pre-rolled random float in [0,1), may this turn ask
    /// a follow-up? Requires enough credit, no back-to-back, and a winning roll.
    pub fn allows(&self, roll: f64) -> bool {
        self.credit >= ASK_THRESHOLD && !self.last_turn_was_question && roll < FOLLOWUP_PROB
    }
}

/// Pure throttle applied AFTER the planner emits an Intent, BEFORE the system
/// prompt is built. Returns the (possibly overridden) goal string and the next
/// pacing state. Only the goal is returned, not the whole Intent, so this module
/// stays decoupled from Intent's other fields.
///
/// - goal != "engage": untouched; only clears last_turn_was_question (credit held).
/// - goal == "engage", allowed: stays "engage"; spends ASK_COST credit, marks last.
/// - goal == "engage", throttled: downgrades to "react"; banks 1 credit (capped).
pub fn throttle(goal: &str, pacing: &QuestionPacing, roll: f64) -> (String, QuestionPacing) {
    if goal != "engage" {
        let mut next = pacing.clone();
        next.last_turn_was_question = false;
        return (goal.to_string(), next);
    }
    let allow = pacing.allows(roll);
    let mut next = pacing.clone();
    if allow {
        next.credit = next.credit.saturating_sub(ASK_COST);
        next.last_turn_was_question = true;
        ("engage".to_string(), next)
    } else {
        // Throttle: respond warmly without a question, and bank some credit.
        next.credit = next.credit.saturating_add(1).min(CREDIT_CAP);
        next.last_turn_was_question = false;
        ("react".to_string(), next)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn charged() -> QuestionPacing {
        QuestionPacing { credit: 3, last_turn_was_question: false }
    }

    #[test]
    fn allows_when_charged_no_back_to_back_and_winning_roll() {
        let p = charged();
        assert!(p.allows(0.0));
        assert!(p.allows(0.59));
    }

    #[test]
    fn rejects_when_credit_below_threshold() {
        let p = QuestionPacing { credit: 1, last_turn_was_question: false };
        assert!(!p.allows(0.0), "no credit, should not ask");
        assert!(!p.allows(0.59));
    }

    #[test]
    fn rejects_when_last_turn_was_question() {
        let p = QuestionPacing { credit: 3, last_turn_was_question: true };
        assert!(!p.allows(0.0), "never two questions in a row");
        assert!(!p.allows(0.1));
    }

    #[test]
    fn rejects_when_roll_at_or_above_threshold() {
        let p = charged();
        assert!(!p.allows(0.6), "roll == threshold is a miss (strict <)");
        assert!(!p.allows(0.9));
    }

    #[test]
    fn throttle_engage_allowed_spends_credit_and_marks_last() {
        let p = charged();
        let (goal, next) = throttle("engage", &p, 0.1);
        assert_eq!(goal, "engage");
        assert_eq!(next.credit, 2);
        assert!(next.last_turn_was_question);
    }

    #[test]
    fn throttle_engage_throttled_downgrades_to_react_and_banks_credit() {
        let p = QuestionPacing { credit: 0, last_turn_was_question: false };
        let (goal, next) = throttle("engage", &p, 0.0);
        assert_eq!(goal, "react");
        assert_eq!(next.credit, 1);
        assert!(!next.last_turn_was_question);
    }

    #[test]
    fn throttle_back_to_back_engage_forces_react() {
        let p = QuestionPacing { credit: 3, last_turn_was_question: true };
        let (goal, next) = throttle("engage", &p, 0.0);
        assert_eq!(goal, "react");
        assert!(!next.last_turn_was_question);
        assert_eq!(next.credit, 3);
    }

    #[test]
    fn throttle_non_engage_resets_last_flag_credit_unchanged() {
        let p = QuestionPacing { credit: 2, last_turn_was_question: true };
        let (goal, next) = throttle("converse", &p, 0.99);
        assert_eq!(goal, "converse");
        assert!(!next.last_turn_was_question);
        assert_eq!(next.credit, 2, "credit untouched for non-engage goals");
    }

    #[test]
    fn steady_state_rate_measured() {
        // Drive many "engage" turns from a cold start with a deterministic LCG.
        // Documents the REAL steady-state rate (see module-level math comment):
        // Target ~40% at FOLLOWUP_PROB=0.6, ASK_COST=1 (ceiling ~50% imposed
        // only by the hard "no two questions in a row" rule). Band widened
        // temporarily for first measurement; will be tightened to the real value.
        // Measured: 0.3754 at FOLLOWUP_PROB=0.6, ASK_COST=1.
        let mut p = QuestionPacing::default();
        let mut asks = 0;
        const N: usize = 300_000;
        let mut state: u64 = 0x9E3779B97F4A7C15;
        for _ in 0..N {
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let roll = (state >> 11) as f64 / (1u64 << 53) as f64;
            let (goal, next) = throttle("engage", &p, roll);
            if goal == "engage" {
                asks += 1;
            }
            p = next;
        }
        let rate = asks as f64 / N as f64;
        println!("[pacing] measured steady-state ask rate = {:.4}", rate);
        assert!(
            rate > 0.35 && rate < 0.40,
            "steady-state rate out of expected band: {:.4}",
            rate
        );
    }
}
