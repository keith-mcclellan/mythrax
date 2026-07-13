use anyhow::Result;
use mythrax_core::cognitive::compactor::compact_hierarchical_dbscan;
use mythrax_core::cognitive::synthesis::InsightNote;

#[tokio::test]
async fn test_hierarchical_dbscan_clustering() -> Result<()> {
    let ins1 = InsightNote {
        title: "July Insight 1".to_string(),
        content: "Content 1".to_string(),
        scope: "scope1".to_string(),
        source_episodes: vec![],
        vault_path: "wiki/scope1/insights/insight_2026-07-01.md".to_string(),
    };
    
    let ins2 = InsightNote {
        title: "July Insight 2".to_string(),
        content: "Content 2".to_string(),
        scope: "scope1".to_string(),
        source_episodes: vec![],
        vault_path: "wiki/scope1/insights/insight_2026-07-05.md".to_string(),
    };

    let ins3 = InsightNote {
        title: "July Outlier".to_string(),
        content: "Content 3".to_string(),
        scope: "scope1".to_string(),
        source_episodes: vec![],
        vault_path: "wiki/scope1/insights/insight_2026-07-10.md".to_string(),
    };

    let ins4 = InsightNote {
        title: "August Insight 1".to_string(),
        content: "Content 4".to_string(),
        scope: "scope1".to_string(),
        source_episodes: vec![],
        vault_path: "wiki/scope1/insights/insight_2026-08-01.md".to_string(),
    };

    let ins5 = InsightNote {
        title: "August Insight 2".to_string(),
        content: "Content 5".to_string(),
        scope: "scope1".to_string(),
        source_episodes: vec![],
        vault_path: "wiki/scope1/insights/insight_2026-08-05.md".to_string(),
    };

    let emb1 = vec![1.0, 0.0, 0.0];
    let emb2 = vec![0.99, 0.01, 0.0];
    let emb3 = vec![0.0, 1.0, 0.0];
    let emb4 = vec![0.0, 0.0, 1.0];
    let emb5 = vec![0.0, 0.01, 0.99];

    let valid_insights = vec![
        (ins1.clone(), "id1".to_string(), Some(emb1)),
        (ins2.clone(), "id2".to_string(), Some(emb2)),
        (ins3.clone(), "id3".to_string(), Some(emb3)),
        (ins4.clone(), "id4".to_string(), Some(emb4)),
        (ins5.clone(), "id5".to_string(), Some(emb5)),
    ];

    let result = compact_hierarchical_dbscan(&valid_insights, 0.10, 2);

    assert!(!result.is_empty(), "Clusters should not be empty");
    
    let has_july_cluster = result.iter().any(|cluster| {
        cluster.iter().any(|(ins, _)| ins.title == "July Insight 1")
            && cluster.iter().any(|(ins, _)| ins.title == "July Insight 2")
    });
    assert!(has_july_cluster, "Should cluster July Insight 1 and 2");

    let has_august_cluster = result.iter().any(|cluster| {
        cluster.iter().any(|(ins, _)| ins.title == "August Insight 1")
            && cluster.iter().any(|(ins, _)| ins.title == "August Insight 2")
    });
    assert!(has_august_cluster, "Should cluster August Insight 1 and 2");

    Ok(())
}
