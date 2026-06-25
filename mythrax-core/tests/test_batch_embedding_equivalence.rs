use std::sync::Mutex;
use mythrax_core::embeddings::LocalEmbedder;

static TEST_MUTEX: Mutex<()> = Mutex::new(());

#[test]
fn test_batch_embedding_equivalence() {
    let _lock = match TEST_MUTEX.lock() {
        Ok(guard) => guard,
        Err(p) => p.into_inner(),
    };

    let embedder = match LocalEmbedder::new() {
        Ok(e) => e,
        Err(e) => {
            eprintln!(
                "Warning: Could not initialize LocalEmbedder for batch equivalence test: {}. Skipping.",
                e
            );
            return;
        }
    };

    let texts: Vec<String> = (0..35)
        .map(|i| format!("This is test sentence number {} for batch embedding equivalence testing.", i))
        .collect();

    let sequential_embeddings: Vec<Vec<f32>> = texts
        .iter()
        .map(|text| embedder.embed(text).expect("Failed to embed text"))
        .collect();

    let batch_embeddings = embedder
        .embed_batch(&texts)
        .expect("Failed to get batch embeddings");

    assert_eq!(
        sequential_embeddings.len(),
        batch_embeddings.len(),
        "Number of sequential embeddings does not match batch embeddings"
    );

    let delta = 1e-4;
    for (i, (seq_emb, batch_emb)) in sequential_embeddings.iter().zip(batch_embeddings.iter()).enumerate() {
        assert_eq!(
            seq_emb.len(),
            batch_emb.len(),
            "Embedding dimension mismatch at index {}: sequential has {} dims, batch has {} dims",
            i,
            seq_emb.len(),
            batch_emb.len()
        );

        for (j, (seq_val, batch_val)) in seq_emb.iter().zip(batch_emb.iter()).enumerate() {
            assert!(
                (seq_val - batch_val).abs() < delta,
                "Embedding values differ at index {}, dimension {}: sequential={}, batch={}, diff={}",
                i,
                j,
                seq_val,
                batch_val,
                (seq_val - batch_val).abs()
            );
        }
    }

    assert_eq!(
        batch_embeddings.len(),
        35,
        "Expected 35 batch embeddings, got {}",
        batch_embeddings.len()
    );
}
