
#[cfg(feature = "mlx")]
use mythrax_core::llm::{DynamicModelBroker, ModelTier};
#[cfg(feature = "mlx")]
use tempfile::tempdir;

#[tokio::test]
#[cfg(feature = "mlx")]
async fn test_model_broker_lifecycle_and_warmup_fallback() {
    println!("DEBUG BROKER TEST: Start");
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let broker = DynamicModelBroker::new(temp_dir.path().to_path_buf()).await.unwrap();

    // 1. Preload pinned embedding model
    println!("DEBUG BROKER TEST: Preloading embedding model");
    broker.preload_embedding_model("mlx-community/nomic-embed-text-v1.5-mlx").await.unwrap();
    assert!(broker.is_embedding_model_loaded());

    // 2. Load the default Coder/MoE LLM (Qwen3.6-35B-A3B)
    println!("DEBUG BROKER TEST: Acquiring coder_model Tier2");
    if std::env::var("MYTHRAX_TEST_MOCK").is_err() {
        broker.update_config_model("mlx-community/Qwen2.5-0.5B-Instruct-4bit").await.unwrap();
    }
    let coder_model = broker.acquire_llm(ModelTier::Tier2).await.unwrap();
    if std::env::var("MYTHRAX_TEST_MOCK").is_ok() {
        assert_eq!(coder_model.name(), "mlx-community/Qwen3.6-35B-A3B-4bit");
    } else {
        assert_eq!(coder_model.name(), "mlx-community/Qwen2.5-0.5B-Instruct-4bit");
    }
    
    // Verify pre-inference shader warm-up was executed
    assert!(coder_model.is_warmed_up());

    // 3. Verify dynamic stop tokens are parsed from tokenizer_config.json
    println!("DEBUG BROKER TEST: Checking stop tokens");
    let stop_tokens = coder_model.stop_tokens();
    assert!(stop_tokens.contains(&"<|eot_id|>".to_string()));

    // 4. Verify weak-pointer tracking: dropping the reference unloads the model from VRAM
    println!("DEBUG BROKER TEST: Testing weak reference eviction");
    let weak_ref = broker.get_weak_llm_reference();
    drop(coder_model);
    
    broker.evict_unused_models().await;
    assert!(weak_ref.upgrade().is_none(), "Model must be evicted from VRAM when strong reference count drops to 0");

    // 5. Verify dynamic model selection: update config to another model and load
    println!("DEBUG BROKER TEST: Testing alternative model acquisition");
    broker.update_config_model("mlx-community/Qwen2.5-0.5B-Instruct-4bit").await.unwrap();
    let model_alt = broker.acquire_llm(ModelTier::Tier2).await.unwrap();
    assert_eq!(model_alt.name(), "mlx-community/Qwen2.5-0.5B-Instruct-4bit");
    drop(model_alt);
    broker.evict_unused_models().await;

    // 6. Simulate Metal shader cache corruption / warm-up panic
    println!("DEBUG BROKER TEST: Testing corrupt broker fallback");
    let corrupt_broker = DynamicModelBroker::new_corrupt_mock().await.unwrap();
    let res = corrupt_broker.acquire_llm_with_warmup_fallback(ModelTier::Tier2).await;
    
    assert!(res.is_ok(), "Warmup fallback must catch shader cache panics and succeed");
    let fallback_model = res.unwrap();
    assert_eq!(fallback_model.execution_mode(), "cpu", "Must fallback to CPU execution mode");
    println!("DEBUG BROKER TEST: End");
}
