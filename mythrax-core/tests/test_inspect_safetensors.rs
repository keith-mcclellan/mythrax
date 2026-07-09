#[cfg(feature = "mlx")]
#[test]
fn test_inspect_calibrated_score_pairs() {
    use mlx_rs::ops::indexing::TryIndexOp;
    use mlx_rs::{Array, StreamOrDevice};
    use mythrax_core::llm::MxbaiReranker;
    use std::path::Path;

    let home = std::env::var("HOME").unwrap();
    let model_dir = Path::new(&home).join(".mythrax/models/mxbai-rerank-large-v2");
    let mut reranker = MxbaiReranker::load(&model_dir).expect("Failed to load reranker");

    let query = "Who wrote 'To Kill a Mockingbird'?";
    let relevant_passage = "To Kill a Mockingbird is a novel by Harper Lee published in 1960. It was immediately successful.";
    let irrelevant_passage =
        "The President of the United States is the head of state and head of government.";

    // Passages to evaluate (prepend empty string as null passage)
    let passages = vec!["", relevant_passage, irrelevant_passage];

    let mut tokenized_sequences = Vec::new();
    let mut max_seq_len = 0;

    for passage in &passages {
        let text = format!(
            "query: {}\ndocument: {}\nYou are a search relevance expert who evaluates how well documents match search queries. For each query-document pair, carefully analyze the semantic relationship between them, then provide your binary relevance judgment (0 for not relevant, 1 for relevant).\nRelevance:",
            query, passage
        );
        let encoding = reranker.tokenizer.encode(text, false).unwrap();
        let ids = encoding.get_ids();
        let ids_i32: Vec<i32> = ids.iter().map(|&x| x as i32).collect();
        if ids_i32.len() > max_seq_len {
            max_seq_len = ids_i32.len();
        }
        tokenized_sequences.push(ids_i32);
    }

    let batch_size = passages.len();
    let pad_token_id = 151643;

    // Bucket max_seq_len to the nearest multiple of 128
    let max_seq_len_bucketed = ((max_seq_len + 127) / 128) * 128;
    println!(
        "Original max_seq_len: {}, Bucketed to: {}",
        max_seq_len, max_seq_len_bucketed
    );

    let mut flat_ids = Vec::with_capacity(batch_size * max_seq_len_bucketed);
    let mut item_masks = Vec::with_capacity(batch_size);

    let max_seq_len_i32 = max_seq_len_bucketed as i32;
    let causal_mask = mlx_rs::ops::full::<f32>(
        &[max_seq_len_i32, max_seq_len_i32],
        &Array::from(f32::NEG_INFINITY),
    )
    .unwrap();
    let causal_mask = mlx_rs::ops::triu_device(&causal_mask, 1, StreamOrDevice::gpu()).unwrap();

    let indices = mlx_rs::ops::arange::<i32, i32>(0, max_seq_len_i32, 1).unwrap();
    let r_indices = indices.reshape(&[max_seq_len_i32, 1]).unwrap();
    let c_indices = indices.reshape(&[1, max_seq_len_i32]).unwrap();
    let is_diagonal = r_indices.eq(&c_indices).unwrap();
    let not_diagonal = is_diagonal.logical_not().unwrap();

    for seq in &tokenized_sequences {
        let pad_len = max_seq_len_bucketed - seq.len();
        for _ in 0..pad_len {
            flat_ids.push(pad_token_id);
        }
        flat_ids.extend(seq);

        let is_pad = indices.lt(&Array::from(pad_len as i32)).unwrap();
        let is_pad_2d = is_pad.reshape(&[1, max_seq_len_i32]).unwrap();
        let mask_cond = is_pad_2d.logical_and(&not_diagonal).unwrap();

        let neg_inf = Array::from(f32::NEG_INFINITY);
        let zero = Array::from(0.0f32);
        let padding_mask = mlx_rs::ops::which(&mask_cond, &neg_inf, &zero).unwrap();

        let item_mask = causal_mask.add(&padding_mask).unwrap();
        item_masks.push(item_mask);
    }

    let ids_array = Array::from_slice(&flat_ids, &[batch_size as i32, max_seq_len_i32]);
    let mask = mlx_rs::ops::stack(&item_masks).unwrap();
    let mask = mask
        .reshape(&[batch_size as i32, 1, max_seq_len_i32, max_seq_len_i32])
        .unwrap();

    let out = reranker.model.forward(&ids_array, Some(&mask)).unwrap();
    let last_hidden = out.try_index((.., max_seq_len_i32 - 1, ..)).unwrap();

    let embed_w = reranker.model.embed_tokens.weight.value.clone();
    let w_no_tok = embed_w.try_index((2152, ..)).unwrap();
    let w_yes_tok = embed_w.try_index((9693, ..)).unwrap();

    let logit_no = last_hidden
        .multiply(&w_no_tok)
        .unwrap()
        .sum_axes_device(&[-1], false, StreamOrDevice::gpu())
        .unwrap();
    let logit_yes = last_hidden
        .multiply(&w_yes_tok)
        .unwrap()
        .sum_axes_device(&[-1], false, StreamOrDevice::gpu())
        .unwrap();

    // Slice null logits (item 0)
    let null_logit_no = logit_no.try_index(0).unwrap();
    let null_logit_yes = logit_yes.try_index(0).unwrap();

    // Calibrate logits for items 1..batch_size
    let real_logit_no = logit_no.try_index(1..).unwrap();
    let real_logit_yes = logit_yes.try_index(1..).unwrap();

    let calibrated_logit_no = real_logit_no.subtract(&null_logit_no).unwrap();
    let calibrated_logit_yes = real_logit_yes.subtract(&null_logit_yes).unwrap();

    let max_logit = mlx_rs::ops::maximum(&calibrated_logit_no, &calibrated_logit_yes).unwrap();
    let exp_no = calibrated_logit_no
        .subtract(&max_logit)
        .unwrap()
        .exp()
        .unwrap();
    let exp_yes = calibrated_logit_yes
        .subtract(&max_logit)
        .unwrap()
        .exp()
        .unwrap();
    let sum_exp = exp_no.add(&exp_yes).unwrap();
    let scores_array = exp_yes.divide(&sum_exp).unwrap();
    let scores_array = scores_array.as_dtype(mlx_rs::Dtype::Float32).unwrap();

    let scores = scores_array.as_slice::<f32>().to_vec();
    println!("CALIBRATED BATCHED SCORES: {:?}", scores);
    assert_eq!(scores.len(), 2);
    assert!(
        scores[0] > scores[1],
        "Relevant passage must score higher than irrelevant passage"
    );
}
