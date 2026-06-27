
#[cfg(feature = "mlx")]
use mythrax_core::llm::{DynamicModelBroker, ModelTier};
#[cfg(feature = "mlx")]
use tempfile::tempdir;

#[tokio::test]
#[cfg(feature = "mlx")]
async fn test_model_broker_lifecycle_and_warmup_fallback() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let broker = DynamicModelBroker::new(temp_dir.path().to_path_buf()).await.unwrap();

    // 1. Preload pinned embedding model
    broker.preload_embedding_model("mlx-community/nomic-embed-text-v1.5-mlx").await.unwrap();
    assert!(broker.is_embedding_model_loaded());

    // 2. Load the default Coder LLM (7B)
    let coder_model = broker.acquire_llm(ModelTier::Tier2).await.unwrap();
    assert_eq!(coder_model.name(), "Qwen2.5-Coder-7B-Instruct-MLX-4bit");
    
    // Verify pre-inference shader warm-up was executed
    assert!(coder_model.is_warmed_up());

    // 3. Verify dynamic stop tokens are parsed from tokenizer_config.json
    let stop_tokens = coder_model.stop_tokens();
    assert!(stop_tokens.contains(&"<|eot_id|>".to_string()));

    // 4. Verify weak-pointer tracking: dropping the reference unloads the model from VRAM
    let weak_ref = broker.get_weak_llm_reference();
    drop(coder_model);
    
    broker.evict_unused_models().await;
    assert!(weak_ref.upgrade().is_none(), "Model must be evicted from VRAM when strong reference count drops to 0");

    // 5. Verify dynamic model selection: update config to 35B model and load
    broker.update_config_model("mlx-community/Qwen3.6-35B-A3B-4bit").await.unwrap();
    let model_35b = broker.acquire_llm(ModelTier::Tier2).await.unwrap();
    assert_eq!(model_35b.name(), "mlx-community/Qwen3.6-35B-A3B-4bit");
    drop(model_35b);
    broker.evict_unused_models().await;

    // 6. Simulate Metal shader cache corruption / warm-up panic
    let corrupt_broker = DynamicModelBroker::new_corrupt_mock().await.unwrap();
    let res = corrupt_broker.acquire_llm_with_warmup_fallback(ModelTier::Tier2).await;
    
    assert!(res.is_ok(), "Warmup fallback must catch shader cache panics and succeed");
    let fallback_model = res.unwrap();
    assert_eq!(fallback_model.execution_mode(), "cpu", "Must fallback to CPU execution mode");
}
