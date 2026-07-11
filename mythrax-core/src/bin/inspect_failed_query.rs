use anyhow::{Result, Context};
use serde::Deserialize;
use std::fs::File;
use std::io::Read;
use std::collections::{HashSet, HashMap};
use mythrax_core::db::{SurrealBackend, StorageBackend};
use mythrax_core::contracts::EpisodeSave;
use surrealdb_types::SurrealValue;

#[derive(Debug, Clone, Deserialize)]
struct QuestionEntry {
    question_id: String,
    question: String,
    question_type: String,
    category: Option<String>,
    haystack_session_ids: Vec<String>,
    haystack_sessions: Vec<Vec<TurnEntry>>,
    answer_session_ids: Vec<String>,
    #[serde(default)]
    gold_corpus_ids: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct TurnEntry {
    role: String,
    content: String,
    #[serde(default)]
    has_answer: bool,
}

fn extract_entities(content: &str) -> Vec<String> {
    let re = regex::Regex::new(r"\b[A-Z][a-zA-Z]+(?:\s+[A-Z][a-zA-Z]+)+\b").unwrap();
    re.find_iter(content)
        .map(|m| m.as_str().to_string())
        .collect()
}

#[tokio::main]
async fn main() -> Result<()> {
    unsafe { std::env::set_var("MYTHRAX_SESSION_ISOLATION", "false"); }
    let _ = mythrax_core::embeddings::load_embedding_cache_from_disk(std::path::Path::new("bench_data/embedding_cache.bin"));
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        println!("Usage: cargo run --bin inspect_failed_query <question_id>");
        return Ok(());
    }
    let target_id = &args[1];

    let dataset_path = std::path::Path::new("bench_data/official/longmemeval_s_cleaned.json");
    let mut file = File::open(dataset_path).context("Failed to open dataset file")?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;
    let questions: Vec<QuestionEntry> = serde_json::from_str(&contents)?;

    let q = questions.iter().find(|qe| &qe.question_id == target_id)
        .context(format!("Question ID {} not found", target_id))?;

    println!("\n========================================================");
    println!("Question ID:   {}", q.question_id);
    println!("Question:      \"{}\"", q.question);
    println!("Type:          {}", q.question_type);
    println!("Category:      {:?}", q.category);
    println!("Classified As: {:?}", mythrax_core::db::backend::classify_query(&q.question));
    println!("========================================================\n");

    let backend = SurrealBackend::new_in_memory()
        .await
        .context("Failed to create in-memory backend")?;
    backend.init().await.context("Failed to initialize database schema")?;
    backend.set_search_mode("hybrid").await;

    // Load tuned parameters
    let tuned_path = std::path::Path::new("bench_data/tuned_params.json");
    if tuned_path.exists() {
        let mut f = File::open(tuned_path)?;
        let mut c = String::new();
        f.read_to_string(&mut c)?;
        let params: HashMap<String, String> = serde_json::from_str(&c)?;
        for (k, v) in params {
            let _ = backend.save_profile_key(&k, &v).await;
        }
        println!("Loaded parameter overrides from tuned_params.json.");
    }

    // Ingest haystack
    let mut episodes_to_ingest = Vec::new();
    let mut session_user_inputs = HashSet::new();
    let mut turn_entities = HashMap::new();
    let mut all_entities_set = HashSet::new();

    for (sess_idx, session_id) in q.haystack_session_ids.iter().enumerate() {
        if let Some(session_turns) = q.haystack_sessions.get(sess_idx) {
            for (turn_idx, turn) in session_turns.iter().enumerate() {
                let corpus_id = format!("{}_turn_{}", session_id, turn_idx);
                let role_lower = turn.role.to_lowercase();
                
                if role_lower == "user" {
                    let norm_content = turn.content.to_lowercase().replace("favourite", "favorite");
                    let clean_stm_value = |val: &str| -> String {
                        let trimmed = val.trim();
                        let mut cleaned = trimmed;
                        if cleaned.starts_with("the ") {
                            cleaned = &cleaned[4..];
                        } else if cleaned.starts_with("a ") {
                            cleaned = &cleaned[2..];
                        } else if cleaned.starts_with("an ") {
                            cleaned = &cleaned[3..];
                        }
                        cleaned.trim().to_string()
                    };

                    for sentence in norm_content.split('.') {
                        let sentence = sentence.trim();
                        if sentence.is_empty() { continue; }

                        if let Some(idx) = sentence.find("degree in") {
                            let val = clean_stm_value(&sentence[idx + "degree in".len()..]);
                            if !val.is_empty() { let _ = backend.save_stm(session_id, "degree", &val).await; }
                        } else if let Some(idx) = sentence.find("majored in") {
                            let val = clean_stm_value(&sentence[idx + "majored in".len()..]);
                            if !val.is_empty() { let _ = backend.save_stm(session_id, "degree", &val).await; }
                        }

                        if let Some(fav_idx) = sentence.find("favorite ") {
                            let remaining = &sentence[fav_idx + "favorite ".len()..];
                            if let Some(is_idx) = remaining.find(" is ") {
                                let word = remaining[..is_idx].trim();
                                if !word.is_empty() && word.chars().all(|c| c.is_alphanumeric() || c == '_') {
                                    let val = clean_stm_value(&remaining[is_idx + " is ".len()..]);
                                    if !val.is_empty() {
                                        let key = format!("favorite_{}", word);
                                        let _ = backend.save_stm(session_id, &key, &val).await;
                                    }
                                }
                            }
                        }

                        if let Some(idx) = sentence.find("prefer") {
                            let val = clean_stm_value(&sentence[idx + "prefer".len()..]);
                            if !val.is_empty() { let _ = backend.save_stm(session_id, "preference", &val).await; }
                        }

                        let mut booked_idx = None;
                        if let Some(idx) = sentence.find("chose") { booked_idx = Some((idx, "chose".len())); }
                        else if let Some(idx) = sentence.find("selected") { booked_idx = Some((idx, "selected".len())); }
                        else if let Some(idx) = sentence.find("booked") { booked_idx = Some((idx, "booked".len())); }
                        if let Some((idx, len)) = booked_idx {
                            let val = clean_stm_value(&sentence[idx + len..]);
                            if !val.is_empty() { let _ = backend.save_stm(session_id, "booked", &val).await; }
                        }

                        let mut occ_idx = None;
                        if let Some(idx) = sentence.find("work as a") { occ_idx = Some((idx, "work as a".len())); }
                        else if let Some(idx) = sentence.find("work at") { occ_idx = Some((idx, "work at".len())); }
                        if let Some((idx, len)) = occ_idx {
                            let val = clean_stm_value(&sentence[idx + len..]);
                            if !val.is_empty() { let _ = backend.save_stm(session_id, "occupation", &val).await; }
                        }
                    }
                }

                let ents = extract_entities(&turn.content);
                if !ents.is_empty() {
                    for ent in &ents { all_entities_set.insert(ent.clone()); }
                    turn_entities.insert(corpus_id.clone(), ents);
                }

                let node_type = match role_lower.as_str() {
                    "user" => {
                        if session_user_inputs.insert(session_id.clone()) {
                            "user_input".to_string()
                        } else {
                            "user_feedback".to_string()
                        }
                    }
                    "assistant" => "agent_thought".to_string(),
                    _ => "agent_thought".to_string(),
                };
                let ep = EpisodeSave {
        created_at: None,
                    title: format!("Session {} - Turn {}", session_id, turn_idx),
                    content: format!("{}: {}", turn.role, turn.content),
                    scope: Some("general".to_string()),
                    vault_path: Some(corpus_id.clone()),
                    session_id: Some(session_id.clone()),
                    node_type: Some(node_type),
                    ..Default::default()
                };
                episodes_to_ingest.push(ep);
            }
        }
    }

    println!("haystack_session_ids len: {}", q.haystack_session_ids.len());
    println!("haystack_sessions len: {}", q.haystack_sessions.len());
    println!("episodes_to_ingest len: {}", episodes_to_ingest.len());

    backend.save_episodes_batch(&episodes_to_ingest).await?;

    let surreal_backend = backend.as_any().downcast_ref::<SurrealBackend>().unwrap();
    let db = &surreal_backend.db;

    let mut corpus_to_ep_id = HashMap::new();
    let mut ep_response = db.query("SELECT id, vault_path FROM episode;").await?;
    #[derive(serde::Deserialize, surrealdb_types::SurrealValue, Debug)]
    struct EpResult {
        id: surrealdb::types::RecordId,
        vault_path: Option<String>,
    }
    let ep_results: Vec<EpResult> = ep_response.take(0).unwrap_or_default();
    for r in ep_results {
        if let Some(vp) = r.vault_path {
            corpus_to_ep_id.insert(vp, mythrax_core::db::backend::format_record_id(&r.id));
        }
    }

    let mut transaction_sql = "BEGIN TRANSACTION;".to_string();
    for ent in all_entities_set {
        let escaped = ent.replace("'", "\\'");
        transaction_sql.push_str(&format!(
            "UPSERT entity:⟨{}⟩ CONTENT {{ name: '{}', entity_type: 'concept', summary: '', labels: ['concept'], scope: 'general' }}; ",
            escaped, escaped
        ));
    }

    for (corpus_id, entities) in &turn_entities {
        if let Some(ep_id) = corpus_to_ep_id.get(corpus_id) {
            let ep_uuid = ep_id.strip_prefix("episode:").unwrap_or(ep_id);
            for ent in entities {
                let escaped = ent.replace("'", "\\'");
                transaction_sql.push_str(&format!(
                    "RELATE episode:⟨{}⟩ -> mentions -> entity:⟨{}⟩ CONTENT {{ created_at: time::now() }}; ",
                    ep_uuid, escaped
                ));
            }
        }
    }

    for sess_idx in 0..q.haystack_session_ids.len() {
        let session_id = &q.haystack_session_ids[sess_idx];
        if let Some(turns) = q.haystack_sessions.get(sess_idx) {
            for i in 0..turns.len() {
                let corpus_id_a = format!("{}_turn_{}", session_id, i);
                let ents_a = match turn_entities.get(&corpus_id_a) {
                    Some(v) => v,
                    None => continue,
                };
                for j in (i + 1)..turns.len() {
                    let corpus_id_b = format!("{}_turn_{}", session_id, j);
                    let ents_b = match turn_entities.get(&corpus_id_b) {
                        Some(v) => v,
                        None => continue,
                    };
                    let has_intersection = ents_a.iter().any(|e| ents_b.contains(e));
                    if has_intersection {
                        if let (Some(ep_a), Some(ep_b)) = (corpus_to_ep_id.get(&corpus_id_a), corpus_to_ep_id.get(&corpus_id_b)) {
                            let ep_a_uuid = ep_a.strip_prefix("episode:").unwrap_or(ep_a);
                            let ep_b_uuid = ep_b.strip_prefix("episode:").unwrap_or(ep_b);
                            transaction_sql.push_str(&format!(
                                "RELATE episode:⟨{}⟩ -> relates_to -> episode:⟨{}⟩ CONTENT {{ confidence: 0.85 }}; ",
                                ep_a_uuid, ep_b_uuid
                            ));
                        }
                    }
                }
            }
        }
    }
    transaction_sql.push_str("COMMIT TRANSACTION;");
    let _ = db.query(&transaction_sql).await?.check();

    let mut count_res = db.query("SELECT VALUE count(id) FROM episode;").await?;
    let count_list: Vec<usize> = count_res.take(0)?;
    let count = count_list.first().cloned().unwrap_or(0);
    println!("Total episodes in DB: {}", count);

    // Build gold_corpus_ids on the fly from turns
    let mut gold_corpus_ids = Vec::new();
    for (sess_idx, session_id) in q.haystack_session_ids.iter().enumerate() {
        if let Some(session_turns) = q.haystack_sessions.get(sess_idx) {
            for (turn_idx, turn) in session_turns.iter().enumerate() {
                if turn.has_answer {
                    gold_corpus_ids.push(format!("{}_turn_{}", session_id, turn_idx));
                }
            }
        }
    }
    println!("Gold IDs: {:?}", gold_corpus_ids);

    // Execute Search
    let last_sess_id = q.answer_session_ids.first().cloned();
    println!("Executing search for query with active session: {:?}", last_sess_id);
    let results = backend.search(
        &q.question,
        Some("general"),
        false,
        50,
        0,
        0.0,
        None,
        false,
        true,
        false,
        last_sess_id.as_deref(),
        false,
        None,
    ).await?;

    println!("\n--- Retrieved Results (Top 50) ---");
    for (idx, r) in results.results.iter().enumerate() {
        let id_str = r.vault_path.as_ref().unwrap_or(&r.id);
        let is_gold = gold_corpus_ids.contains(id_str);
        let gold_marker = if is_gold { "  [GOLD]  " } else { "          " };
        println!(
            "{}{}. ID: {:<30} | Score: {:.4} | VecSim: {:<6} | BM25: {:<6} | Session: {:?}",
            gold_marker,
            idx + 1,
            id_str,
            r.similarity,
            r.raw_vector_sim.map_or("N/A".to_string(), |v| format!("{:.4}", v)),
            r.bm25_score.map_or("N/A".to_string(), |v| format!("{:.2}", v)),
            r.session_id
        );
        println!("     Content: \"{}\"\n", r.content);
    }

    println!("--- Gold Documents Status ---");
    for gold_id in &gold_corpus_ids {
        if let Some(r) = results.results.iter().find(|res| res.vault_path.as_ref() == Some(gold_id)) {
            println!(
                "FOUND:     ID: {:<30} | Score: {:.4} | VecSim: {:<6} | BM25: {:<6} | Session: {:?}",
                gold_id,
                r.similarity,
                r.raw_vector_sim.map_or("N/A".to_string(), |v| format!("{:.4}", v)),
                r.bm25_score.map_or("N/A".to_string(), |v| format!("{:.2}", v)),
                r.session_id
            );
        } else {
            println!("MISSING:   ID: {:<30}", gold_id);
            let select_sql = "SELECT id, content, session_id, vault_path FROM episode WHERE vault_path = $vp;";
            let mut select_res = db.query(select_sql).bind(("vp", gold_id.clone())).await?;
            #[derive(serde::Deserialize, surrealdb_types::SurrealValue, Debug)]
            struct DbEp {
                content: String,
                session_id: Option<String>,
            }
            let list: Vec<DbEp> = select_res.take(0).unwrap_or_default();
            if let Some(ep) = list.first() {
                println!("  --> Text in DB: \"{}\"", ep.content);
                println!("  --> Session ID: {:?}", ep.session_id);
            } else {
                println!("  --> NOT FOUND IN DATABASE!");
            }
        }
    }

    Ok(())
}
