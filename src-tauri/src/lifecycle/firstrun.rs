//! First run: seeds initial persona traits and checks if setup is needed.
//! Design doc 7.6: first impression is extremely important.

use crate::db::{pending as db_pending, persona as db_persona, DbState};

/// Checks if this is the first run and performs initialization if so.
/// Seeds core persona traits and returns true if first-run actions were taken.
pub fn run_firstrun_checks(db: &DbState) -> Result<bool, String> {
    let has_traits = db.with_conn(|conn| {
        let core = db_persona::get_traits_by_type(conn, "core")?;
        Ok(!core.is_empty())
    })?;

    if has_traits {
        return Ok(false); // Not first run.
    }

    log::info!("First run detected: seeding initial persona traits");
    seed_persona(db)?;
    seed_cold_start_interviews(db)?;
    Ok(true)
}

/// Seeds the core personality traits.
fn seed_persona(db: &DbState) -> Result<(), String> {
    let now = chrono::Utc::now().to_rfc3339();

    let core_traits = [
        ("gentle", 0.95),
        ("patient", 0.90),
        ("curious", 0.85),
        ("playful", 0.80),
        ("caring", 0.92),
    ];

    db.with_conn(|conn| {
        for (key, confidence) in &core_traits {
            let trait_id = format!("trait_core_{}", key);
            db_persona::upsert_trait(
                conn,
                &db_persona::PersonaTrait {
                    id: trait_id,
                    trait_type: "core".to_string(),
                    trait_key: key.to_string(),
                    confidence: *confidence,
                    source: "seed".to_string(),
                    created_at: now.clone(),
                    updated_at: now.clone(),
                },
            )?;
        }
        Ok(())
    })?;

    log::info!("Seeded {} core persona traits", core_traits.len());
    Ok(())
}

/// Seeds cold-start interview questions for the first 3 days.
/// Design doc 7.6: first few days she actively interviews the user
/// to build the relationship. These bypass the closeness gate.
fn seed_cold_start_interviews(db: &DbState) -> Result<(), String> {
    let now = chrono::Utc::now();
    let now_str = now.to_rfc3339();

    // Questions spread across the first 3 days.
    // First question triggers 5 minutes after first launch (after welcome animation).
    let interviews = [
        ("你平时喜欢做什么呀？", chrono::Duration::minutes(5)),
        ("你有什么梦想或者特别想做的事吗？", chrono::Duration::hours(2)),
        ("最近有什么开心的事吗？", chrono::Duration::hours(24)),
        ("你平时工作或者学习忙不忙呀？", chrono::Duration::hours(48)),
    ];

    db.with_conn(|conn| {
        for (question, delay) in &interviews {
            let remind = now + *delay;
            let id = format!("pe_interview_{}", uuid::Uuid::new_v4().simple());
            let event = db_pending::PendingEvent {
                id,
                title: question.to_string(),
                event_date: now_str.clone(),
                remind_date: Some(remind.to_rfc3339()),
                source_episode: None,
                status: "interview".to_string(),
                importance: 1.0,
                followup_count: 0,
                created_at: now_str.clone(),
                triggered_at: None,
                resolved_at: None,
            };
            db_pending::insert(conn, &event)?;
        }
        Ok(())
    })?;

    log::info!("Seeded {} cold-start interview questions", interviews.len());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_utils::test_db;

    #[test]
    fn test_firstrun_seeds_persona() {
        let db = test_db();
        let is_first = run_firstrun_checks(&db).unwrap();
        assert!(is_first);

        // Second call should be idempotent.
        let is_first_again = run_firstrun_checks(&db).unwrap();
        assert!(!is_first_again);
    }

    #[test]
    fn test_firstrun_seeds_interviews() {
        let db = test_db();
        run_firstrun_checks(&db).unwrap();

        let count: i64 = db
            .with_conn(|conn| {
                Ok(conn
                    .query_row(
                        "SELECT COUNT(*) FROM pending_events WHERE status = 'interview'",
                        [],
                        |row| row.get(0),
                    )
                    .unwrap_or(0))
            })
            .unwrap();
        assert_eq!(count, 4, "should seed 4 interview questions");
    }
}
