use mythrax_core::contracts::{ModelTier, TaskArchetype, TaskProfile};
use mythrax_core::llm::router::route_task;
use mythrax_core::db::{StorageBackend, SurrealBackend};

#[test]
fn test_routing_types_exist() {
    let profile = TaskProfile::new(TaskArchetype::Summarization)
        .with_tokens(100)
        .with_latency_sensitive(true);

    assert_eq!(profile.archetype, TaskArchetype::Summarization);
    assert_eq!(profile.estimated_tokens, Some(100));
    assert!(profile.latency_sensitive);

    let tier = ModelTier::Micro;
    assert_eq!(tier, ModelTier::Micro);
}

#[tokio::test]
async fn test_routing_heuristics() {
    let db = SurrealBackend::new_in_memory().await.unwrap();
    db.init().await.unwrap();

    // Summarization, latency sensitive, few tokens -> Micro
    let profile = TaskProfile::new(TaskArchetype::Summarization)
        .with_tokens(100)
        .with_latency_sensitive(true);
    let tier = route_task(&db, &profile).await;
    let (total_swap, _) = mythrax_core::llm::router::get_swap_usage().unwrap_or((0.0, 0.0));
    if total_swap >= 4000.0 {
        assert_eq!(tier, ModelTier::Cloud);
    } else {
        assert_eq!(tier, ModelTier::Micro);
    }

    // Code, heavy tokens -> Cloud
    let profile_code = TaskProfile::new(TaskArchetype::Code)
        .with_tokens(10000)
        .with_latency_sensitive(false);
    let tier_code = route_task(&db, &profile_code).await;
    if total_swap >= 4000.0 {
        assert_eq!(tier_code, ModelTier::Cloud);
    } else {
        assert_eq!(tier_code, ModelTier::Cloud);
    }

    // Reasoning, medium tokens -> Large or Cloud
    let profile_reason = TaskProfile::new(TaskArchetype::Reasoning)
        .with_tokens(1500)
        .with_latency_sensitive(false);
    let tier_reason = route_task(&db, &profile_reason).await;
    if total_swap >= 4000.0 {
        assert_eq!(tier_reason, ModelTier::Cloud);
    } else {
        assert_eq!(tier_reason, ModelTier::Large);
    }
}
