use anyhow::Result;
use mythrax_core::db::{StorageBackend, SurrealBackend};
use std::time::Instant;

#[tokio::test]
async fn test_hnsw_index_parameters_and_rebuild() -> Result<()> {
    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;

    // 1. Query table info
    let sql_info = "INFO FOR TABLE episode;";
    let mut response = backend.db.query(sql_info).await?.check()?;

    // The response is a Value. Let's serialize/print it or extract the indexes field
    let info_val: Option<serde_json::Value> = response.take(0)?;
    let info_val = info_val.expect("Table info should not be empty");
    println!("DEBUG TABLE INFO: {:?}", info_val);

    let indexes = info_val
        .get("indexes")
        .expect("Table info should contain indexes");
    let hnsw_index_def = indexes
        .get("episode_hnsw")
        .expect("Should find episode_hnsw index");
    let hnsw_def_str = hnsw_index_def
        .as_str()
        .expect("Index definition should be a string");
    println!("HNSW INDEX DEF: {}", hnsw_def_str);

    // Verify it contains the optimized parameters
    assert!(
        hnsw_def_str.contains("M 16") || hnsw_def_str.contains("m=16"),
        "HNSW index must use M=16"
    );
    assert!(
        hnsw_def_str.contains("EFC 200") || hnsw_def_str.contains("efc=200"),
        "HNSW index must use EFC=200"
    );
    assert!(
        hnsw_def_str.contains("TYPE F32")
            || hnsw_def_str.contains("type=f32")
            || hnsw_def_str.contains("type F32"),
        "HNSW index must use TYPE F32"
    );

    // 2. Measure rebuild duration
    let start = Instant::now();
    backend
        .db
        .query("REBUILD INDEX episode_hnsw ON TABLE episode;")
        .await?
        .check()?;
    let duration = start.elapsed();

    println!("SUCCESS: Rebuilt episode_hnsw index in {:?}", duration);

    Ok(())
}
