use crate::contracts::{Tier, WikiNode, WisdomRule};
use crate::db::StorageBackend;
use anyhow::Result;
use surrealdb_types::SurrealValue;

pub async fn run_graduation_pipeline(db: &dyn StorageBackend, current_scope: &str) -> Result<()> {
    let surreal_backend = db
        .as_any()
        .downcast_ref::<crate::db::SurrealBackend>()
        .ok_or_else(|| anyhow::anyhow!("SurrealBackend required"))?;

    // Select local wiki nodes
    let sql_local = "SELECT *, type::string(id) AS id FROM wiki_node WHERE scope = $scope AND embedding IS NOT NULL;";
    let mut resp_local = surreal_backend
        .db
        .query(sql_local)
        .bind(("scope", current_scope))
        .await?
        .check()?;
    let local_nodes: Vec<WikiNode> = resp_local.take(0)?;

    // Select other projects' wiki nodes
    let sql_other = "SELECT *, type::string(id) AS id FROM wiki_node WHERE scope != $scope AND embedding IS NOT NULL;";
    let mut resp_other = surreal_backend
        .db
        .query(sql_other)
        .bind(("scope", current_scope))
        .await?
        .check()?;
    let other_nodes: Vec<WikiNode> = resp_other.take(0)?;

    // ⚡ Bolt Optimization:
    // Pre-calculate embedding norms to avoid redundant $O(N^2)$ recalculations in the double loop.
    // This reduces redundant math for 1536d embeddings significantly and accelerates the pipeline.
    struct GradCandidate<'a> {
        node: &'a WikiNode,
        embedding: &'a std::vec::Vec<f32>,
        embedding_norm: f32,
    }

    let local_cands: Vec<GradCandidate> = local_nodes
        .iter()
        .filter_map(|node| {
            node.embedding.as_ref().map(|emb| GradCandidate {
                node,
                embedding: emb,
                embedding_norm: emb.iter().map(|&x| x * x).sum::<f32>().sqrt(),
            })
        })
        .collect();

    let other_cands: Vec<GradCandidate> = other_nodes
        .iter()
        .filter_map(|node| {
            node.embedding.as_ref().map(|emb| GradCandidate {
                node,
                embedding: emb,
                embedding_norm: emb.iter().map(|&x| x * x).sum::<f32>().sqrt(),
            })
        })
        .collect();

    for local_cand in &local_cands {
        let local = local_cand.node;
        let local_emb = local_cand.embedding;
        let local_norm = local_cand.embedding_norm;

        for other_cand in &other_cands {
            let other = other_cand.node;
            let other_emb = other_cand.embedding;
            let other_norm = other_cand.embedding_norm;

            if local_emb.len() != other_emb.len() || local_emb.is_empty() {
                continue;
            }

            let mut dot = 0.0;
            for i in 0..local_emb.len() {
                dot += local_emb[i] * other_emb[i];
            }

            let sim = if local_norm == 0.0 || other_norm == 0.0 {
                0.0
            } else {
                dot / (local_norm * other_norm)
            };

            if sim >= 0.85 {
                // Block graduation if either source node is flagged as contradicted
                if local.node_type.as_deref() == Some("conflict")
                    || other.node_type.as_deref() == Some("conflict")
                {
                    tracing::info!(
                        "Graduation blocked: source node flagged as conflict (local='{}', other='{}')",
                        local.name, other.name
                    );
                    continue;
                }

                let uuid = uuid::Uuid::new_v4().to_string();
                let global_rule = WisdomRule {
                    id: Some(format!("wisdom:{}", uuid)),
                    target_pattern: format!("Standardized: {}", local.name),
                    action_to_avoid: format!(
                        "Avoid project-specific deviations for {}",
                        local.name
                    ),
                    causal_explanation: format!(
                        "Graduated due to cross-project convergence between scope '{}' and '{}' (Similarity: {:.2}).",
                        current_scope, other.scope, sim
                    ),
                    prescribed_remedy: format!(
                        "Adopt the converged architectural pattern: {}",
                        local.content
                    ),
                    tier: Tier::Wisdom,
                    scope: "global".to_string(),
                    vault_path: None,
                    embedding: local.embedding.clone(),
                    source_episodes: vec![
                        local.id.clone().unwrap_or_default(),
                        other.id.clone().unwrap_or_default(),
                    ],
                    generator_name: "GraduationPipeline".to_string(),
                    similarity: Some(sim),
                    utility: Some(1.0),
                    status: Some("active".to_string()),
                    superseded_at: None,
                    superseded_by: None,
                    rule_type: Some("graduated_insight".to_string()),
                    severity: Some("info".to_string()),
                    blocking: Some(false),
                    importance: Some(6.0),
                    content_hash: None,
                };

                let wisdom_id = db.save_wisdom_rule(&global_rule).await?;

                if let Some(ref local_id) = local.id {
                    let _ = db
                        .relate_nodes(
                            local_id,
                            &wisdom_id,
                            local.temporal_range_start,
                            local.temporal_range_end,
                            Some(sim as f32),
                        )
                        .await;
                }
                if let Some(ref other_id) = other.id {
                    let _ = db
                        .relate_nodes(
                            other_id,
                            &wisdom_id,
                            other.temporal_range_start,
                            other.temporal_range_end,
                            Some(sim as f32),
                        )
                        .await;
                }
                break;
            }
        }
    }

    // 2. 365-day half-life decay and 500-node LRU cap on global wisdom rules
    #[derive(serde::Deserialize, SurrealValue, Debug)]
    struct WisdomRuleDecayInfo {
        id: Option<String>,
        utility: Option<f32>,
        #[serde(default)]
        created_at: Option<chrono::DateTime<chrono::Utc>>,
    }

    let sql_wisdom = "SELECT type::string(id) as id, created_at, (utility ?? 1.0) AS utility FROM wisdom WHERE tier = 'Wisdom' OR scope = 'global';";
    let mut resp_wisdom = surreal_backend.db.query(sql_wisdom).await?.check()?;
    let mut rules: Vec<WisdomRuleDecayInfo> = resp_wisdom.take(0)?;

    let ln2 = 2.0f64.ln();
    let half_life_days = 365.0f64;

    for rule in &mut rules {
        let util = rule.utility.unwrap_or(1.0) as f64;
        let age_days = rule
            .created_at
            .map(|dt| (chrono::Utc::now() - dt).num_hours() as f64 / 24.0)
            .unwrap_or(0.0);
        let decayed_util = util * (-age_days * ln2 / half_life_days).exp();
        rule.utility = Some(decayed_util as f32);

        let Some(ref full_id) = rule.id else { continue; };
        let id_raw = full_id
            .split(':')
            .nth(1)
            .unwrap_or(full_id)
            .to_string();
        let _ = surreal_backend.db.query("UPDATE metrics SET utility_score = $utility WHERE target_id = type::record('wisdom', $id);")
            .bind(("id", id_raw))
            .bind(("utility", decayed_util as f32))
            .await;
    }

    rules.sort_by(|a, b| {
        b.utility
            .partial_cmp(&a.utility)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    if rules.len() > 500 {
        let to_delete = &rules[500..];
        for rule in to_delete {
            if let Some(ref id) = rule.id {
                let id_raw = id.split(':').nth(1).unwrap_or(id).to_string();
                let cascade_sql = "
                    BEGIN TRANSACTION;
                    DELETE relates_to WHERE in = type::record('wisdom', $id) OR out = type::record('wisdom', $id);
                    DELETE followed_by WHERE in = type::record('wisdom', $id) OR out = type::record('wisdom', $id);
                    DELETE mentions WHERE in = type::record('wisdom', $id) OR out = type::record('wisdom', $id);
                    DELETE superseded_by WHERE in = type::record('wisdom', $id) OR out = type::record('wisdom', $id);
                    DELETE metrics WHERE target_id = type::record('wisdom', $id);
                    DELETE type::record('wisdom', $id);
                    COMMIT TRANSACTION;
                ";
                let _ = surreal_backend
                    .db
                    .query(cascade_sql)
                    .bind(("id", id_raw))
                    .await;
            }
        }
    }

    Ok(())
}
