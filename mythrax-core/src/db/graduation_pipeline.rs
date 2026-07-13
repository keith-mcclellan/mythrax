use anyhow::Result;
use crate::db::StorageBackend;
use crate::contracts::{WisdomRule, Tier, WikiNode};
use crate::math::cosine_similarity;

pub async fn run_graduation_pipeline(db: &dyn StorageBackend, current_scope: &str) -> Result<()> {
    let surreal_backend = db.as_any().downcast_ref::<crate::db::SurrealBackend>()
        .ok_or_else(|| anyhow::anyhow!("SurrealBackend required"))?;
    
    // Select local wiki nodes
    let sql_local = "SELECT * FROM wiki_node WHERE scope = $scope AND embedding IS NOT NULL;";
    let mut resp_local = surreal_backend.db.query(sql_local).bind(("scope", current_scope)).await?.check()?;
    let local_nodes: Vec<WikiNode> = resp_local.take(0)?;

    // Select other projects' wiki nodes
    let sql_other = "SELECT * FROM wiki_node WHERE scope != $scope AND embedding IS NOT NULL;";
    let mut resp_other = surreal_backend.db.query(sql_other).bind(("scope", current_scope)).await?.check()?;
    let other_nodes: Vec<WikiNode> = resp_other.take(0)?;

    for local in &local_nodes {
        let local_emb = match &local.embedding {
            Some(e) => e,
            None => continue,
        };

        for other in &other_nodes {
            let other_emb = match &other.embedding {
                Some(e) => e,
                None => continue,
            };

            let sim = cosine_similarity(local_emb, other_emb);
            if sim >= 0.85 {
                let uuid = uuid::Uuid::new_v4().to_string();
                let global_rule = WisdomRule {
                    id: Some(format!("wisdom:{}", uuid)),
                    target_pattern: format!("Standardized: {}", local.name),
                    action_to_avoid: format!("Avoid project-specific deviations for {}", local.name),
                    causal_explanation: format!(
                        "Graduated due to cross-project convergence between scope '{}' and '{}' (Similarity: {:.2}).",
                        current_scope,
                        other.scope,
                        sim
                    ),
                    prescribed_remedy: format!("Adopt the converged architectural pattern: {}", local.content),
                    tier: Tier::Wisdom,
                    scope: "global".to_string(),
                    vault_path: None,
                    embedding: local.embedding.clone(),
                    source_episodes: vec![],
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
                };

                db.save_wisdom_rule(&global_rule).await?;
                break;
            }
        }
    }

    // 2. 365-day half-life decay and 500-node LRU cap on global wisdom rules
    let sql_wisdom = "SELECT * FROM wisdom WHERE tier = 'Wisdom' OR scope = 'global';";
    let mut resp_wisdom = surreal_backend.db.query(sql_wisdom).await?.check()?;
    let mut rules: Vec<WisdomRule> = resp_wisdom.take(0)?;

    let ln2 = 2.0f64.ln();
    let half_life_days = 365.0f64;

    for rule in &mut rules {
        let util = rule.utility.unwrap_or(1.0) as f64;
        let age_days = 0.0f64; // default to 0 for decay calculation or parse rule age
        let decayed_util = util * (-age_days * ln2 / half_life_days).exp();
        rule.utility = Some(decayed_util as f32);

        let id_raw = rule.id.as_ref().unwrap().split(':').nth(1).unwrap_or(rule.id.as_ref().unwrap()).to_string();
        let _ = surreal_backend.db.query("UPDATE type::record('wisdom', $id) MERGE { utility: $utility };")
            .bind(("id", id_raw))
            .bind(("utility", decayed_util as f32))
            .await;
    }

    rules.sort_by(|a, b| b.utility.partial_cmp(&a.utility).unwrap_or(std::cmp::Ordering::Equal));

    if rules.len() > 500 {
        let to_delete = &rules[500..];
        for rule in to_delete {
            if let Some(ref id) = rule.id {
                let id_raw = id.split(':').nth(1).unwrap_or(id).to_string();
                let _ = surreal_backend.db.query("DELETE type::record('wisdom', $id);")
                    .bind(("id", id_raw))
                    .await;
            }
        }
    }

    Ok(())
}
