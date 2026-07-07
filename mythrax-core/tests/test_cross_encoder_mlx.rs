#![cfg(feature = "mlx")]

use std::path::Path;
use mythrax_core::llm::MxbaiReranker;
use mlx_rs::ops::indexing::TryIndexOp;

#[test]
fn test_cross_encoder_mlx_loading_and_scoring() {
    let home = std::env::var("HOME").unwrap();
    let model_dir = Path::new(&home).join(".mythrax/models/mxbai-rerank-large-v2");
    if !model_dir.exists() {
        return;
    }

    let mut reranker = MxbaiReranker::load(&model_dir).expect("Failed to load MxbaiReranker");

    let query = "Who wrote 'To Kill a Mockingbird'?";
    let passages = vec![
        "To Kill a Mockingbird is a novel by Harper Lee published in 1960. It was immediately successful.",
        "Moby-Dick; or, The Whale is an 1851 novel by American writer Herman Melville.",
        "The President of the United States is the head of state and head of government.",
    ];

    let start = std::time::Instant::now();
    
    // Test the sequential logic
    let mut scores = Vec::new();
    
    // 1. Compute null logits
    let null_text = format!("query: {} document: ", query);
    let null_encoding = reranker.tokenizer.encode(null_text, false).unwrap();
    let null_ids: Vec<i32> = null_encoding.get_ids().iter().map(|&x| x as i32).collect();
    let null_seq_len = null_ids.len();
    let null_ids_array = mlx_rs::Array::from_slice(&null_ids, &[1, null_seq_len as i32]);
    let null_out = reranker.model.forward(&null_ids_array, None).unwrap();
    let null_last_hidden = null_out.try_index((0, (null_seq_len - 1) as i32, ..)).unwrap();

    let embed_w = reranker.model.embed_tokens.weight.value.clone();
    let w_0 = embed_w.try_index((15, ..)).unwrap();
    let w_1 = embed_w.try_index((16, ..)).unwrap();

    let null_logit_0 = null_last_hidden.multiply(&w_0).unwrap().sum_axes(&[-1], false).unwrap();
    let null_logit_1 = null_last_hidden.multiply(&w_1).unwrap().sum_axes(&[-1], false).unwrap();
    let nl0 = null_logit_0.as_dtype(mlx_rs::Dtype::Float32).unwrap().as_slice::<f32>()[0];
    let nl1 = null_logit_1.as_dtype(mlx_rs::Dtype::Float32).unwrap().as_slice::<f32>()[0];

    // 2. Loop over passages
    for passage in &passages {
        let text = format!("query: {} document: {}", query, passage);
        let encoding = reranker.tokenizer.encode(text, false).unwrap();
        let ids: Vec<i32> = encoding.get_ids().iter().map(|&x| x as i32).collect();
        let seq_len = ids.len();
        let ids_array = mlx_rs::Array::from_slice(&ids, &[1, seq_len as i32]);

        let out = reranker.model.forward(&ids_array, None).unwrap();
        let last_hidden = out.try_index((0, (seq_len - 1) as i32, ..)).unwrap();

        let logit_0 = last_hidden.multiply(&w_0).unwrap().sum_axes(&[-1], false).unwrap();
        let logit_1 = last_hidden.multiply(&w_1).unwrap().sum_axes(&[-1], false).unwrap();

        let raw_l0 = logit_0.as_dtype(mlx_rs::Dtype::Float32).unwrap().as_slice::<f32>()[0];
        let raw_l1 = logit_1.as_dtype(mlx_rs::Dtype::Float32).unwrap().as_slice::<f32>()[0];

        let l0 = raw_l0 - nl0;
        let l1 = raw_l1 - nl1;

        let max_l = l0.max(l1);
        let exp_l0 = (l0 - max_l).exp();
        let exp_l1 = (l1 - max_l).exp();
        let prob_1 = exp_l1 / (exp_l0 + exp_l1);
        scores.push(prob_1);
    }

    println!("Sequential scoring took: {:?}", start.elapsed());
    println!("SCORES: {:?}", scores);

    assert_eq!(scores.len(), 3);
    assert!(scores[0] > scores[1], "Relevant passage must score higher than Moby-Dick");
    assert!(scores[0] > scores[2], "Relevant passage must score higher than President passage");
}
