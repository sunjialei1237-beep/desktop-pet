use rusqlite::{params, Connection};

use crate::embedding::cosine_similarity;

/// Serializes a float slice into a little-endian byte blob.
fn vec_to_blob(vec: &[f32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(vec.len() * 4);
    for &f in vec {
        bytes.extend_from_slice(&f.to_le_bytes());
    }
    bytes
}

/// Deserializes a little-endian byte blob back into a float vector.
fn blob_to_vec(blob: &[u8]) -> Vec<f32> {
    assert!(blob.len() % 4 == 0, "corrupt embedding blob");
    (0..blob.len())
        .step_by(4)
        .map(|i| {
            let arr: [u8; 4] = blob[i..i + 4].try_into().unwrap();
            f32::from_le_bytes(arr)
        })
        .collect()
}

/// Stores or replaces the embedding for an episode.
pub fn insert(conn: &Connection, episode_id: &str, embedding: &[f32]) -> Result<(), String> {
    let blob = vec_to_blob(embedding);
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT OR REPLACE INTO episode_vectors (episode_id, embedding, created_at) VALUES (?1, ?2, ?3)",
        params![episode_id, blob, now],
    )
    .map_err(|e| format!("Failed to insert vector: {}", e))?;
    Ok(())
}

/// Retrieves the embedding for an episode, if one exists.
pub fn get(conn: &Connection, episode_id: &str) -> Result<Option<Vec<f32>>, String> {
    let result: rusqlite::Result<Vec<u8>> = conn.query_row(
        "SELECT embedding FROM episode_vectors WHERE episode_id = ?1",
        [episode_id],
        |row| row.get(0),
    );
    match result {
        Ok(blob) => Ok(Some(blob_to_vec(&blob))),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(format!("Failed to query vector: {}", e)),
    }
}

/// Brute-force cosine similarity search over all stored vectors.
/// Returns (episode_id, similarity -1..1) pairs sorted by descending similarity.
pub fn search(conn: &Connection, query: &[f32], limit: usize) -> Result<Vec<(String, f64)>, String> {
    let mut stmt = conn
        .prepare("SELECT episode_id, embedding FROM episode_vectors")
        .map_err(|e| format!("Failed to prepare vector search: {}", e))?;

    let rows = stmt
        .query_map([], |row| {
            let id: String = row.get(0)?;
            let blob: Vec<u8> = row.get(1)?;
            let embedding = blob_to_vec(&blob);
            let sim = cosine_similarity(query, &embedding) as f64;
            Ok((id, sim))
        })
        .map_err(|e| format!("Failed to query vectors: {}", e))?;

    let mut results: Vec<(String, f64)> = rows.filter_map(|r| r.ok()).collect();
    results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    results.truncate(limit);
    Ok(results)
}

/// Deletes the embedding for an episode (used during lifecycle cleanup).
pub fn delete(conn: &Connection, episode_id: &str) -> Result<(), String> {
    conn.execute(
        "DELETE FROM episode_vectors WHERE episode_id = ?1",
        [episode_id],
    )
    .map_err(|e| format!("Failed to delete vector: {}", e))?;
    Ok(())
}

/// Returns the count of stored vectors.
pub fn count(conn: &Connection) -> Result<i64, String> {
    conn.query_row("SELECT COUNT(*) FROM episode_vectors", [], |row| row.get(0))
        .map_err(|e| format!("Failed to count vectors: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_utils::test_db;
    use crate::db::episodes as db_episodes;
    use rusqlite::Connection;

    fn make_episode(conn: &Connection, summary: &str) -> String {
        let id = format!("ep_vec_{}", uuid::Uuid::new_v4().simple());
        let now = chrono::Utc::now().to_rfc3339();
        let ep = db_episodes::Episode {
            id: id.clone(),
            time: now.clone(),
            summary: summary.to_string(),
            emotion: None,
            importance: 0.5,
            is_landmark: false,
            subject: "user".to_string(),
            participants: None,
            topics: None,
            source_type: "conversation".to_string(),
            source_conversation_id: None,
            source_turn: None,
            memory_strength: 0.5,
            recall_count: 0,
            last_recalled_at: None,
            consolidated: false,
            created_at: now,
        };
        db_episodes::insert(conn, &ep).unwrap();
        id
    }

    fn dummy_vec(seed: f32) -> Vec<f32> {
        (0..8).map(|i| (seed + i as f32).sin()).collect()
    }

    #[test]
    fn test_insert_and_get() {
        let db = test_db();
        db.with_conn(|conn| {
            let ep_id = make_episode(conn, "test");
            let vec = dummy_vec(1.0);
            insert(conn, &ep_id, &vec)?;
            let retrieved = get(conn, &ep_id)?;
            assert!(retrieved.is_some());
            let got = retrieved.unwrap();
            assert_eq!(got.len(), vec.len());
            for (a, b) in got.iter().zip(vec.iter()) {
                assert!((a - b).abs() < 1e-6);
            }
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn test_get_missing() {
        let db = test_db();
        db.with_conn(|conn| {
            let result = get(conn, "nonexistent")?;
            assert!(result.is_none());
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn test_search_orders_by_similarity() {
        let db = test_db();
        db.with_conn(|conn| {
            let ep1 = make_episode(conn, "hotpot");
            let ep2 = make_episode(conn, "coding");
            let ep3 = make_episode(conn, "movie");

            insert(conn, &ep1, &dummy_vec(1.0))?;
            insert(conn, &ep2, &dummy_vec(5.0))?;
            insert(conn, &ep3, &dummy_vec(10.0))?;

            let results = search(conn, &dummy_vec(1.0), 3)?;
            assert_eq!(results.len(), 3);
            assert!(results[0].1 > 0.999);
            assert_eq!(results[0].0, ep1);
            assert!(results[0].1 >= results[1].1);
            assert!(results[1].1 >= results[2].1);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn test_search_empty() {
        let db = test_db();
        db.with_conn(|conn| {
            let results = search(conn, &dummy_vec(1.0), 5)?;
            assert!(results.is_empty());
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn test_search_limit() {
        let db = test_db();
        db.with_conn(|conn| {
            for _ in 0..10 {
                let ep_id = make_episode(conn, "bulk");
                insert(conn, &ep_id, &dummy_vec(1.0))?;
            }
            let results = search(conn, &dummy_vec(1.0), 3)?;
            assert_eq!(results.len(), 3);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn test_delete() {
        let db = test_db();
        db.with_conn(|conn| {
            let ep_id = make_episode(conn, "delete me");
            insert(conn, &ep_id, &dummy_vec(1.0))?;
            assert_eq!(count(conn)?, 1);
            delete(conn, &ep_id)?;
            assert_eq!(count(conn)?, 0);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn test_insert_replaces() {
        let db = test_db();
        db.with_conn(|conn| {
            let ep_id = make_episode(conn, "replace");
            insert(conn, &ep_id, &dummy_vec(1.0))?;
            insert(conn, &ep_id, &dummy_vec(5.0))?;
            assert_eq!(count(conn)?, 1);
            let got = get(conn, &ep_id)?.unwrap();
            let expected = dummy_vec(5.0);
            for (a, b) in got.iter().zip(expected.iter()) {
                assert!((a - b).abs() < 1e-6);
            }
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn test_blob_roundtrip() {
        let original: Vec<f32> = vec![0.1, -0.2, 0.3, -0.4, 0.5, 1.0, 0.0, -1.0];
        let blob = vec_to_blob(&original);
        assert_eq!(blob.len(), original.len() * 4);
        let recovered = blob_to_vec(&blob);
        for (a, b) in original.iter().zip(recovered.iter()) {
            assert!((a - b).abs() < 1e-7);
        }
    }
}
