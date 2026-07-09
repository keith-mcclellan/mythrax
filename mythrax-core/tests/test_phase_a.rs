#![cfg(feature = "bench")]

use anyhow::Result;
use mythrax_core::db::{SurrealBackend, StorageBackend};
use mythrax_core::db::backend::QueryCategory;
use mythrax_core::contracts::{EpisodeSave};
use mythrax_core::bench::metrics::parse_haystack_date;
use mythrax_core::db::backend::get_decay_factor;

#[test]
fn test_t1_parse_haystack_date() {
    assert_eq!(parse_haystack_date("2023/05/20 (Sat) 02:21"), Some("2023-05-20T02:21:00Z".to_string()));
    assert_eq!(parse_haystack_date(""), None);
    assert_eq!(parse_haystack_date("invalid"), None);
}

#[tokio::test]
async fn test_t2_temporal_decay_with_anchor() -> Result<()> {
    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;

    // Ingest Episode A: created 10 days ago relative to anchor
    let ep_a = EpisodeSave {
        title: "Rust database locks".to_string(),
        content: "We should understand database locking mechanisms in Rust.".to_string(),
        scope: Some("general".to_string()),
        created_at: Some("2023-05-20T23:40:00Z".to_string()),
        ..Default::default()
    };
    let id_a = backend.save_episode(&ep_a).await?;

    // Ingest Episode B: created 1 day ago relative to anchor
    let ep_b = EpisodeSave {
        title: "Rust database locks".to_string(),
        content: "Rust database locking mechanisms are very useful.".to_string(),
        scope: Some("general".to_string()),
        created_at: Some("2023-05-29T23:40:00Z".to_string()),
        ..Default::default()
    };
    let id_b = backend.save_episode(&ep_b).await?;

    // Search with temporal_anchor matching a day after Episode B (2023-05-30T23:40:00Z)
    let resp = backend.search(
        "database locks",
        Some("general"),
        false,
        10,
        0,
        0.0,
        None,
        false,
        true,
        true,
        None,
        true,
        Some("2023-05-30T23:40:00Z"),
    ).await?;

    let results = resp.results;
    assert!(results.len() >= 2);
    let pos_a = results.iter().position(|r| r.id == id_a).expect("Episode A not found");
    let pos_b = results.iter().position(|r| r.id == id_b).expect("Episode B not found");
    assert!(pos_b < pos_a, "Episode B (recent) must rank higher than Episode A (old) when temporal_anchor is used");

    Ok(())
}

#[test]
fn test_t2b_decay_factor_per_category() {
    // Temporal: decay < 1.0 for delta_t=10 days, sigma=720h
    let decay_temp = get_decay_factor(QueryCategory::Temporal, 10.0 * 86400.0, 720.0, 0.10);
    assert!(decay_temp < 1.0, "Temporal category must decay: {}", decay_temp);

    // Default: decay < 1.0 for delta_t=10 days, sigma=168h
    let decay_def = get_decay_factor(QueryCategory::Default, 10.0 * 86400.0, 168.0, 0.10);
    assert!(decay_def < 1.0, "Default category must decay: {}", decay_def);

    // Preference/User: decay == 1.0
    let decay_pref = get_decay_factor(QueryCategory::Preference, 10.0 * 86400.0, 168.0, 0.10);
    assert_eq!(decay_pref, 1.0, "Preference category must not decay");
    
    let decay_user = get_decay_factor(QueryCategory::User, 10.0 * 86400.0, 168.0, 0.10);
    assert_eq!(decay_user, 1.0, "User category must not decay");
}

#[tokio::test]
async fn test_t3_classify_query_regression() -> Result<()> {
    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;
    assert_eq!(backend.classify_query_db("what is the weather in Tokyo").await, QueryCategory::Default);
    assert_eq!(backend.classify_query_db("my next mtg").await, QueryCategory::Temporal);
    assert_eq!(backend.classify_query_db("my favourite lodging").await, QueryCategory::Preference);
    assert_eq!(backend.classify_query_db("what is job salary").await, QueryCategory::User);
    Ok(())
}

#[tokio::test]
async fn test_t6_bench_ingestion_sets_created_at() -> Result<()> {
    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;

    let ep = EpisodeSave {
        title: "Test Ingestion Timestamp".to_string(),
        content: "Test content".to_string(),
        scope: Some("general".to_string()),
        created_at: Some("2023-05-20T23:40:00Z".to_string()),
        ..Default::default()
    };
    let id = backend.save_episode(&ep).await?;
    let uuid = id.split(':').nth(1).unwrap();

    let mut res = backend.db.query("SELECT VALUE created_at FROM type::record('episode', $id);")
        .bind(("id", uuid))
        .await?;
    let created_at_opt: Option<chrono::DateTime<chrono::Utc>> = res.take(0)?;
    let expected = "2023-05-20T23:40:00Z".parse::<chrono::DateTime<chrono::Utc>>()?;
    assert_eq!(created_at_opt, Some(expected));

    Ok(())
}
