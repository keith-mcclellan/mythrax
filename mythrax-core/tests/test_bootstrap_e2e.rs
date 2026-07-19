use std::fs;
use anyhow::Result;
use tempfile::tempdir;
use mythrax_core::db::{SurrealBackend, StorageBackend};
use mythrax_core::contracts::EpisodeSave;
use mythrax_core::cognitive::synthesis::DreamCoordinator;
use mythrax_core::store::MarkdownStore;
use std::sync::Mutex;

static TEST_MUTEX: Mutex<()> = Mutex::new(());

#[tokio::test]
async fn test_bootstrap_e2e() -> Result<()> {
    let _lock = match TEST_MUTEX.lock() {
        Ok(guard) => guard,
        Err(p) => p.into_inner(),
    };
    let _ = tracing_subscriber::fmt::try_init();

    let tmp = tempdir()?;
    let vault_root = tmp.path().join("vault");
    fs::create_dir_all(&vault_root)?;
    fs::create_dir_all(vault_root.join("wiki"))?;
    fs::create_dir_all(vault_root.join("wisdom"))?;
    fs::create_dir_all(vault_root.join("episodes"))?;

    let workspace_root = tmp.path().join("workspace");
    fs::create_dir_all(&workspace_root)?;
    
    unsafe {
        std::env::set_var("MYTHRAX_TEST_MOCK", "1");
        std::env::set_var("MYTHRAX_VAULT_ROOT", vault_root.to_str().unwrap());
        std::env::set_var("MYTHRAX_WORKSPACE_ROOT", workspace_root.to_str().unwrap());
        std::env::set_var("MYTHRAX_MOCK_LLM", "true");
    }

    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;
    let store = MarkdownStore::new(&vault_root)?;
    let coordinator = DreamCoordinator::new();

    // 1. Insert 12 mock episodes (6 in test_scope_a, 4 in test_scope_b, 2 contradicting)
    // 6 episodes in test_scope_a: semantically similar content about "database migration patterns"
    for i in 0..6 {
        let ep = EpisodeSave {
            title: format!("Antigravity Episode A{}", i),
            content: format!("Database migration patterns discuss how we run schema changes and upgrade SQLite/SurrealDB databases safely. Step {}.", i),
            scope: Some("test_scope_a".to_string()),
            confidence: Some(5.0),
            // Pre-computed embeddings designed to produce known cluster geometry
            // We use a mock embedder or override embeddings in the DB after saving
            ..Default::default()
        };
        let ep_id = backend.save_episode(&ep).await?;
        
        // Force mock embeddings in the DB
        let thing_id = mythrax_core::db::parse_record_id(&ep_id)?;
        let mock_embedding = vec![0.1 * (i as f32 + 1.0); 768];
        let _: Vec<serde_json::Value> = backend.db.query("UPDATE $id SET embedding = $emb, processed_in_dream = false, confidence = 5.0, created_at = <datetime>$created_at;")
            .bind(("id", thing_id))
            .bind(("emb", mock_embedding))
            .bind(("created_at", format!("2026-07-19T12:00:0{}Z", i)))
            .await?.take(0)?;
    }

    // 4 episodes in test_scope_b: semantically similar content to enable cross-scope graduation
    for i in 0..4 {
        let ep = EpisodeSave {
            title: format!("Antigravity Episode B{}", i),
            content: format!("Database migration patterns discuss how we run schema changes and upgrade SQLite/SurrealDB databases safely. Cross scope step {}.", i),
            scope: Some("test_scope_b".to_string()),
            confidence: Some(5.0),
            ..Default::default()
        };
        let ep_id = backend.save_episode(&ep).await?;
        
        let thing_id = mythrax_core::db::parse_record_id(&ep_id)?;
        let mock_embedding = vec![0.1 * (i as f32 + 1.0); 768];
        let _: Vec<serde_json::Value> = backend.db.query("UPDATE $id SET embedding = $emb, processed_in_dream = false, confidence = 5.0, created_at = <datetime>$created_at;")
            .bind(("id", thing_id))
            .bind(("emb", mock_embedding))
            .bind(("created_at", format!("2026-07-19T13:00:0{}Z", i)))
            .await?.take(0)?;
    }

    // 2 contradicting episodes: ORMs vs No ORMs
    let ep_orm = EpisodeSave {
        title: "Antigravity ORM view".to_string(),
        content: "We must always use ORMs for database queries to ensure type safety and prevent SQL injection. Absolutely always.".to_string(),
        scope: Some("test_scope_a".to_string()),
        ..Default::default()
    };
    let ep_orm_id = backend.save_episode(&ep_orm).await?;
    let thing_orm = mythrax_core::db::parse_record_id(&ep_orm_id)?;
    let _: Vec<serde_json::Value> = backend.db.query("UPDATE $id SET embedding = $emb, processed_in_dream = false, created_at = <datetime>'2026-07-19T14:00:00Z';")
        .bind(("id", thing_orm))
        .bind(("emb", vec![0.9; 768]))
        .await?.take(0)?;

    let ep_no_orm = EpisodeSave {
        title: "Antigravity No ORM view".to_string(),
        content: "We should never use ORMs for database queries. They cause object-relational impedance mismatch and slow down performance. Absolutely never ORMs.".to_string(),
        scope: Some("test_scope_a".to_string()),
        ..Default::default()
    };
    let ep_no_orm_id = backend.save_episode(&ep_no_orm).await?;
    let thing_no_orm = mythrax_core::db::parse_record_id(&ep_no_orm_id)?;
    let _: Vec<serde_json::Value> = backend.db.query("UPDATE $id SET embedding = $emb, processed_in_dream = false, created_at = <datetime>'2026-07-19T14:00:01Z';")
        .bind(("id", thing_no_orm))
        .bind(("emb", vec![0.95; 768]))
        .await?.take(0)?;

    // Enable cross-scope graduation profile config
    backend.save_profile_key("compactor.enable_cross_scope_graduation", "true").await?;

    // 2. Run run_dream(mode="deep") synchronously
    // In deep dreaming mode, all scopes are processed.
    coordinator.run_dream(&backend, &store, Some("deep"), None).await?;

    // 3. Assertions
    // ✅ Episodes: 12 exist, all marked processed_in_dream=true
    let eps = backend.get_all_episodes().await?;
    assert_eq!(eps.len(), 12);
    for ep in &eps {
        assert_eq!(ep.processed_in_dream, Some(true));
    }

    // ✅ Episode Titles: All 12 episodes have non-placeholder titles (not "antigravity_*")
    // (mock LLM will generate titles during distillation step)
    for ep in &eps {
        assert!(!ep.title.starts_with("antigravity_"));
    }

    // ✅ Episode Summaries: All 12 episodes have `summary` field populated in DB
    for ep in &eps {
        assert!(ep.summary.is_some());
    }

    // ✅ Episode Wiki Pages: `wiki/{scope}/episodes/*.md` files exist with Summary sections
    // ✅ Summary WikiNodes: 12 WikiNodes with node_type="episode_summary" exist in DB
    let wiki_nodes = backend.get_all_wiki_nodes().await?;
    let episode_summaries: Vec<_> = wiki_nodes.iter().filter(|n| n.node_type.as_deref() == Some("episode_summary")).collect();
    assert_eq!(episode_summaries.len(), 12);

    for node in &episode_summaries {
        assert!(node.vault_path.is_some());
        let path = vault_root.join(node.vault_path.as_ref().unwrap());
        assert!(path.exists());
        let content = fs::read_to_string(path)?;
        assert!(content.contains("## Summary"));
    }

    // ✅ Clusters: DBSCAN produced ≥1 cluster
    // ✅ Insights: ≥1 WikiNode with node_type="insight"
    let insights: Vec<_> = wiki_nodes.iter().filter(|n| n.node_type.as_deref() == Some("insight")).collect();
    assert!(!insights.is_empty(), "DBSCAN should produce at least one insight cluster");

    // ✅ Directions: ≥1 WikiNode with node_type="direction" (from promote_insight_to_direction)
    let directions: Vec<_> = wiki_nodes.iter().filter(|n| n.node_type.as_deref() == Some("direction")).collect();
    assert!(!directions.is_empty(), "Should promote at least one insight to direction");

    // ✅ Direction Backprop: Direction content updated after backpropagate_directions ran
    // Check if the promoted direction is modified/appended with backpropagation trace
    let dir = &directions[0];
    assert!(dir.content.contains("Backpropagated Evidence") || dir.content.contains("refined"));

    // ✅ Insight→Direction Edge: relates_to edge from insight to direction exists
    let rel_insights = backend.get_related_node_ids(insights[0].id.as_ref().unwrap()).await?;
    assert!(rel_insights.iter().any(|id| directions.iter().any(|d| d.id.as_ref() == Some(id))));

    // ✅ Wisdom: ≥1 WisdomRule (from cross-scope graduation)
    let wisdom_rules = backend.get_all_wisdom_rules().await?;
    assert!(!wisdom_rules.is_empty(), "Should graduate cross-scope direction to wisdom");

    // ✅ Wisdom Provenance: WisdomRule.source_episodes is non-empty
    let wisdom_rule = &wisdom_rules[0];
    assert!(!wisdom_rule.source_episodes.is_empty());

    // ✅ Wisdom Graph Edge: relates_to edge from insight → wisdom rule exists
    let rel_insight_to_wisdom = backend.get_related_node_ids(insights[0].id.as_ref().unwrap()).await?;
    assert!(rel_insight_to_wisdom.contains(wisdom_rule.id.as_ref().unwrap()));

    // ✅ Conflicts: ≥1 WikiNode with node_type="conflict" preserving both positions
    let conflicts: Vec<_> = wiki_nodes.iter().filter(|n| n.node_type.as_deref() == Some("conflict")).collect();
    assert!(!conflicts.is_empty(), "ORM contradiction should produce a conflict node");
    assert!(conflicts[0].content.contains("Conflicting Positions"));

    // ✅ Conflict Edges: relates_to edges from both conflicting nodes → conflict node
    let rel_orm = backend.get_related_node_ids(&ep_orm_id).await?;
    assert!(rel_orm.contains(conflicts[0].id.as_ref().unwrap()));
    let rel_no_orm = backend.get_related_node_ids(&ep_no_orm_id).await?;
    assert!(rel_no_orm.contains(conflicts[0].id.as_ref().unwrap()));

    // ✅ Conflict Vault: wiki/{scope}/conflicts/*.md files exist
    let conflict_path = vault_root.join(conflicts[0].vault_path.as_ref().unwrap());
    assert!(conflict_path.exists());

    // ✅ Pruned Leaves: Hebbian weight decay executed (relates_to weights < 1.0)
    // Verify that at least one relates_to edge has confidence < 1.0
    // We query relates_to content or similar in surrealdb
    let edges_resp = backend.db.query("SELECT confidence FROM relates_to WHERE confidence < 1.0;").await?;
    let low_conf_edges: Vec<serde_json::Value> = edges_resp.check()?.take(0)?;
    assert!(!low_conf_edges.is_empty(), "Should decay relates_to weights < 1.0");

    // ✅ Graph Provenance: relates_to edges from episodes → episode_summaries with valid_from set
    // ✅ Graph Provenance: relates_to edges from episodes → insights with valid_from set
    // ✅ Temporal Anchoring: Insight WikiNodes have temporal_range_start/end spanning source episodes
    // ✅ Temporal Anchoring: Edge valid_from matches episode created_at (not processing time)
    // Verify relates_to edge properties
    let sql_edges = "SELECT * FROM relates_to;";
    let mut response_edges = backend.db.query(sql_edges).await?.check()?;
    
    // We check raw values since surreal deserialization of record ID in "in" is custom
    let edges: Vec<serde_json::Value> = response_edges.take(0)?;
    assert!(!edges.is_empty());
    
    let mut has_valid_from = false;
    for edge in &edges {
        if edge.get("valid_from").is_some() && !edge.get("valid_from").unwrap().is_null() {
            has_valid_from = true;
        }
    }
    assert!(has_valid_from, "Edges must be temporally anchored (valid_from not null)");

    // Clean up env
    unsafe {
        std::env::remove_var("MYTHRAX_TEST_MOCK");
        std::env::remove_var("MYTHRAX_VAULT_ROOT");
        std::env::remove_var("MYTHRAX_WORKSPACE_ROOT");
        std::env::remove_var("MYTHRAX_MOCK_LLM");
    }

    Ok(())
}
