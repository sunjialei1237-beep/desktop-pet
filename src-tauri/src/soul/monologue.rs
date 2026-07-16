//! Internal Monologue: surfaces internal thoughts at the right moment.
//!
//! Design doc 7.1: She thinks between conversations. These thoughts were
//! generated during Reflection (soul/reflection.rs), not during the live
//! conversation. The key: she REALLY thought of it last night, timestamp proves it.
//!
//! Surfacing conditions:
//! - next_interaction: surface when user comes back (default)
//! - emotion_match: surface when current emotion matches (future)
//! - time_based: surface at a specific time (future)

use crate::db::reflections::InternalThought;
use crate::db::DbState;

/// Checks for unsurfaced internal thoughts that should be expressed now.
/// Returns thoughts matching the `next_interaction` surfacing type,
/// and marks them as surfaced so they are not repeated.
pub fn surface_thoughts(db: &DbState) -> Result<Vec<InternalThought>, String> {
    let now = chrono::Utc::now().to_rfc3339();

    db.with_conn(|conn| {
        let mut unsurfaced = crate::db::reflections::get_unsurfaced(conn)?;
        // Only surface next_interaction type for now.
        unsurfaced.retain(|t| t.surfacing_type == "next_interaction");

        // Mark them as surfaced so they don't repeat.
        for t in &unsurfaced {
            crate::db::reflections::mark_surfaced(conn, &t.id, &now)?;
        }

        // Limit to 1 per interaction (don't dump all thoughts at once).
        if unsurfaced.len() > 1 {
            unsurfaced.truncate(1);
        }

        Ok(unsurfaced)
    })
}

/// Returns the count of unsurfaced thoughts (for debug panel).
pub fn unsurfaced_count(db: &DbState) -> Result<usize, String> {
    db.with_conn(|conn| {
        let thoughts = crate::db::reflections::get_unsurfaced(conn)?;
        Ok(thoughts.len())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_utils::test_db;
    use crate::db::reflections::{insert_thought, InternalThought};

    fn make_thought(id: &str, st: &str) -> InternalThought {
        InternalThought {
            id: id.to_string(),
            content: "test thought".to_string(),
            emotion: Some("happy".to_string()),
            source_reflection: None,
            surfacing_type: st.to_string(),
            created_at: "2026-07-14T22:00:00".to_string(),
            surfaced_at: None,
        }
    }

    #[test]
    fn test_surface_next_interaction() {
        let db = test_db();
        db.with_conn(|conn| {
            insert_thought(conn, &make_thought("t1", "next_interaction"))?;
            insert_thought(conn, &make_thought("t2", "emotion_match"))?;
            Ok(())
        }).unwrap();

        let surfaced = surface_thoughts(&db).unwrap();
        assert_eq!(surfaced.len(), 1);
        assert_eq!(surfaced[0].id, "t1");

        // Second call should return nothing (already surfaced).
        let again = surface_thoughts(&db).unwrap();
        assert_eq!(again.len(), 0);
    }

    #[test]
    fn test_unsurfaced_count() {
        let db = test_db();
        db.with_conn(|conn| {
            insert_thought(conn, &make_thought("t1", "next_interaction"))?;
            insert_thought(conn, &make_thought("t2", "next_interaction"))?;
            Ok(())
        }).unwrap();

        assert_eq!(unsurfaced_count(&db).unwrap(), 2);
    }
}
