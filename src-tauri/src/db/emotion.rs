use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmotionState {
    pub mood: f64,
    pub mood_label: String,
    pub physical_energy: f64,
    pub social_battery: f64,
    pub stress: f64,
    pub loneliness: f64,
    pub rest_need: f64,
    pub bl_mood: f64,
    pub bl_energy: f64,
    pub bl_social: f64,
    pub bl_stress: f64,
    pub last_homeostasis_at: String,
    pub updated_at: String,
}

/// Gets the singleton emotion state.
pub fn get(conn: &Connection) -> Result<EmotionState, String> {
    conn.query_row(
        "SELECT mood, mood_label, physical_energy, social_battery, stress,
                loneliness, rest_need, bl_mood, bl_energy, bl_social, bl_stress,
                last_homeostasis_at, updated_at
         FROM emotion_state WHERE id = 1",
        [],
        |row| {
            Ok(EmotionState {
                mood: row.get(0)?,
                mood_label: row.get(1)?,
                physical_energy: row.get(2)?,
                social_battery: row.get(3)?,
                stress: row.get(4)?,
                loneliness: row.get(5)?,
                rest_need: row.get(6)?,
                bl_mood: row.get(7)?,
                bl_energy: row.get(8)?,
                bl_social: row.get(9)?,
                bl_stress: row.get(10)?,
                last_homeostasis_at: row.get(11)?,
                updated_at: row.get(12)?,
            })
        },
    )
    .map_err(|e| format!("Failed to get emotion state: {}", e))
}

/// Updates emotion fields. Only provided (Some) fields are changed.
pub fn update_fields(
    conn: &Connection,
    mood: Option<f64>,
    mood_label: Option<&str>,
    physical_energy: Option<f64>,
    social_battery: Option<f64>,
    stress: Option<f64>,
    loneliness: Option<f64>,
    rest_need: Option<f64>,
    now: &str,
) -> Result<(), String> {
    conn.execute(
        "UPDATE emotion_state SET
            mood = COALESCE(?1, mood),
            mood_label = COALESCE(?2, mood_label),
            physical_energy = COALESCE(?3, physical_energy),
            social_battery = COALESCE(?4, social_battery),
            stress = COALESCE(?5, stress),
            loneliness = COALESCE(?6, loneliness),
            rest_need = COALESCE(?7, rest_need),
            updated_at = ?8
         WHERE id = 1",
        params![
            mood, mood_label, physical_energy, social_battery,
            stress, loneliness, rest_need, now,
        ],
    )
    .map_err(|e| format!("Failed to update emotion: {}", e))?;
    Ok(())
}

/// Applies homeostasis: drifts all values toward their baselines.
/// rate is 0..1, where 1 = instantly snap to baseline, 0 = no movement.
pub fn apply_homeostasis(conn: &Connection, rate: f64, now: &str) -> Result<(), String> {
    conn.execute(
        "UPDATE emotion_state SET
            mood = mood + (bl_mood - mood) * ?1,
            physical_energy = physical_energy + (bl_energy - physical_energy) * ?1,
            social_battery = social_battery + (bl_social - social_battery) * ?1,
            stress = stress + (bl_stress - stress) * ?1,
            last_homeostasis_at = ?2,
            updated_at = ?2
         WHERE id = 1",
        params![rate, now],
    )
    .map_err(|e| format!("Failed to apply homeostasis: {}", e))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_utils::test_db;

    #[test]
    fn test_emotion_singleton() {
        let db = test_db();
        db.with_conn(|conn| {
            let emo = get(conn)?;
            assert!((emo.mood - 0.5).abs() < 0.001);
            assert!((emo.physical_energy - 0.7).abs() < 0.001);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn test_update_and_homeostasis() {
        let db = test_db();
        db.with_conn(|conn| {
            // Raise stress to 0.8
            update_fields(conn, None, None, None, None, Some(0.8), None, None, "now")?;
            let emo = get(conn)?;
            assert!((emo.stress - 0.8).abs() < 0.001);

            // Apply homeostasis: should move stress toward baseline 0.2
            apply_homeostasis(conn, 0.5, "now")?;
            let emo = get(conn)?;
            assert!(emo.stress < 0.8, "stress should have drifted toward baseline");
            assert!((emo.stress - 0.5).abs() < 0.001, "halfway between 0.8 and 0.2");
            Ok(())
        })
        .unwrap();
    }
}
