use anyhow::Result;
use mythrax_core::db::{SurrealBackend, StorageBackend};
use mythrax_core::contracts::EpisodeSave;

#[tokio::test]
async fn test_fts_cap_behavior() -> Result<()> {
    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;
    backend.set_search_mode("hybrid").await;

    // We insert 3 documents with the keyword "architecture"
    let content_arch = "This is a document about microservice architecture and service mesh design.";
    let titles_arch = vec![
        "Microservice Node Alpha",
        "Microservice Node Beta",
        "Microservice Node Gamma",
    ];

    for title in &titles_arch {
        let ep = EpisodeSave {
            title: title.to_string(),
            content: content_arch.to_string(),
            scope: Some("general".to_string()),
            ..Default::default()
        };
        backend.save_episode(&ep).await?;
    }

    // We insert 2 documents WITHOUT "architecture" (to ensure DF < N, yielding non-zero IDF)
    let content_other = "This is a recipe for baking delicious homemade pizza with cheese.";
    let titles_other = vec![
        "Pizza Node Delta",
        "Pizza Node Epsilon",
    ];

    for title in &titles_other {
        let ep = EpisodeSave {
            title: title.to_string(),
            content: content_other.to_string(),
            scope: Some("general".to_string()),
            ..Default::default()
        };
        backend.save_episode(&ep).await?;
    }

    // Allow SurrealDB FTS to index
    tokio::time::sleep(std::time::Duration::from_millis(2000)).await;

    // Query all episodes to verify what's in the DB
    let mut raw_eps_resp = backend.db.query("SELECT id, title, embedding FROM episode;").await?;
    let raw_eps: Vec<serde_json::Value> = raw_eps_resp.take(0)?;
    println!("Total episodes in DB: {}", raw_eps.len());
    for ep in &raw_eps {
        println!(" - DB Episode: {}", ep);
    }

    // Test case 1: Set MYTHRAX_FTS_CAP = 2 via env var
    unsafe {
        std::env::set_var("MYTHRAX_FTS_CAP", "2");
    }

    let resp_cap_2 = backend.search(
        "architecture",
        Some("general"),
        false,
        10,
        0,
        0.0,
        None,
        false,
        true,
        true,
    ).await?;

    println!("Search Results count (Cap 2): {}", resp_cap_2.results.len());
    for (i, r) in resp_cap_2.results.iter().enumerate() {
        println!(" [{}] Title: '{}', Sim: {}, BM25: {:?}", i, r.title, r.similarity, r.bm25_score);
    }

    // Since cap = 2, only 2 keyword candidates should be returned (as vector search returns 0)
    assert_eq!(
        resp_cap_2.results.len(),
        2,
        "With cap = 2, the number of returned results should be exactly 2"
    );

    // Test case 2: Clear MYTHRAX_FTS_CAP, set profile key search.fts_cap = 3
    unsafe {
        std::env::remove_var("MYTHRAX_FTS_CAP");
    }
    backend.save_profile_key("search.fts_cap", "3").await?;

    // We need 4 documents with "architecture" to test cap = 3!
    // Let's insert a 4th document with "architecture"
    let ep = EpisodeSave {
        title: "Microservice Node Zeta".to_string(),
        content: content_arch.to_string(),
        scope: Some("general".to_string()),
        ..Default::default()
    };
    backend.save_episode(&ep).await?;

    // Allow SurrealDB FTS to index
    tokio::time::sleep(std::time::Duration::from_millis(2000)).await;

    let resp_cap_3 = backend.search(
        "architecture",
        Some("general"),
        false,
        10,
        0,
        0.0,
        None,
        false,
        true,
        true,
    ).await?;

    println!("Search Results count (Cap 3): {}", resp_cap_3.results.len());
    for (i, r) in resp_cap_3.results.iter().enumerate() {
        println!(" [{}] Title: '{}', Sim: {}, BM25: {:?}", i, r.title, r.similarity, r.bm25_score);
    }

    // Since cap = 3, only 3 keyword candidates should be returned
    assert_eq!(
        resp_cap_3.results.len(),
        3,
        "With cap = 3, the number of returned results should be exactly 3"
    );

    Ok(())
}
