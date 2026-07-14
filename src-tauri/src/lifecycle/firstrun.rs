//! First run: seeds initial persona traits and checks if setup is needed.
//! Design doc 7.6: first impression is extremely important.

use crate::db::{persona as db_persona, DbState};

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
