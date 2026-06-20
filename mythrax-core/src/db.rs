use axum::async_trait;
use crate::contracts::{EpisodeSave, SearchResult, WisdomRule};
use anyhow::{Result, Context};
use surrealdb::engine::local::{Db, Mem};
use surrealdb::Surreal;
use uuid::Uuid;

#[async_trait]
pub trait StorageBackend: Send + Sync {
    async fn init(&self) -> Result<()>;
    async fn save_episode(&self, episode: &EpisodeSave) -> Result<String>;
    async fn save_wisdom_rule(&self, rule: &WisdomRule) -> Result<String>;
    async fn search(&self, query: &str, scope: Option<&str>, limit: usize) -> Result<Vec<SearchResult>>;
    async fn get_wisdom(&self, query: &str, tier: &str, limit: usize) -> Result<Vec<WisdomRule>>;
    async fn record_feedback(&self, id: &str, success: bool) -> Result<()>;
    async fn apply_migrations(&self) -> Result<()>;
}

pub struct SurrealBackend {
    db: Surreal<Db>,
}

impl SurrealBackend {
    pub async fn new_in_memory() -> Result<Self> {
        let db = Surreal::new::<Mem>(()).await
            .context("Failed to initialize in-memory SurrealDB")?;
        db.use_ns("mythrax").use_db("memory").await?;
        Ok(Self { db })
    }
}

#[derive(serde::Deserialize)]
struct SearchRaw {
    id: surrealdb::sql::Thing,
    title: String,
    content: String,
}

#[async_trait]
impl StorageBackend for SurrealBackend {
    async fn init(&self) -> Result<()> {
        let schema_queries = "
            DEFINE TABLE entity SCHEMAFULL;
            DEFINE FIELD name ON entity TYPE string;
            DEFINE FIELD entity_type ON entity TYPE string;
            DEFINE FIELD summary ON entity TYPE string;
            DEFINE FIELD labels ON entity TYPE array<string>;
            DEFINE FIELD scope ON entity TYPE string DEFAULT 'general';
            DEFINE FIELD vault_path ON entity TYPE string DEFAULT '';
            DEFINE INDEX entity_name ON entity FIELDS name UNIQUE;
            DEFINE INDEX entity_scope ON entity FIELDS scope;

            DEFINE TABLE episode SCHEMAFULL;
            DEFINE FIELD title ON episode TYPE string;
            DEFINE FIELD content ON episode TYPE string;
            DEFINE FIELD source ON episode TYPE string DEFAULT '';
            DEFINE FIELD scope ON episode TYPE string DEFAULT 'general';
            DEFINE FIELD vault_path ON episode TYPE string DEFAULT '';
            DEFINE FIELD processed_in_dream ON episode TYPE bool DEFAULT false;
            DEFINE INDEX episode_scope ON episode FIELDS scope;

            DEFINE TABLE wisdom SCHEMAFULL;
            DEFINE FIELD target_pattern ON wisdom TYPE string;
            DEFINE FIELD action_to_avoid ON wisdom TYPE string;
            DEFINE FIELD causal_explanation ON wisdom TYPE string;
            DEFINE FIELD prescribed_remedy ON wisdom TYPE string;
            DEFINE FIELD tier ON wisdom TYPE string DEFAULT 'dynamic';
            DEFINE FIELD scope ON wisdom TYPE string DEFAULT 'general';
            DEFINE FIELD vault_path ON wisdom TYPE string DEFAULT '';
            DEFINE FIELD source_episodes ON wisdom TYPE array<string>;
            DEFINE FIELD generator_name ON wisdom TYPE string;
            DEFINE INDEX wisdom_scope ON wisdom FIELDS scope;
            DEFINE INDEX wisdom_tier ON wisdom FIELDS tier;

            DEFINE TABLE metrics SCHEMAFULL;
            DEFINE FIELD target_id ON metrics TYPE record;
            DEFINE FIELD utility_score ON metrics TYPE float DEFAULT 1.0;
            DEFINE FIELD access_count ON metrics TYPE int DEFAULT 0;
            DEFINE FIELD last_accessed ON metrics TYPE datetime DEFAULT time::now();
            DEFINE INDEX metrics_target ON metrics FIELDS target_id UNIQUE;
        ";
        self.db.query(schema_queries).await?
            .check().context("Applying schemas failed")?;
        Ok(())
    }

    async fn save_episode(&self, episode: &EpisodeSave) -> Result<String> {
        let mut ep_uuid = Uuid::new_v4().to_string();
        let mut is_update = false;

        if let Some(ref vp) = episode.vault_path {
            let check_query = "SELECT VALUE id FROM episode WHERE vault_path = $vault_path LIMIT 1;";
            let mut response = self.db.query(check_query).bind(("vault_path", vp)).await?;
            let ids: Option<surrealdb::sql::Thing> = response.take(0)?;
            if let Some(thing) = ids {
                ep_uuid = thing.id.to_string();
                is_update = true;
            }
        }

        let query_str = if is_update {
            "
                BEGIN TRANSACTION;
                LET $ep = type::thing('episode', $ep_uuid);
                UPDATE $ep CONTENT {
                    title: $title,
                    content: $content,
                    scope: $target_scope,
                    vault_path: $vault_path,
                    processed_in_dream: false
                };
                DELETE FROM mentions WHERE in = $ep;
                COMMIT TRANSACTION;
            "
        } else {
            "
                BEGIN TRANSACTION;
                LET $ep = type::thing('episode', $ep_uuid);
                LET $met = type::thing('metrics', $metrics_uuid);
                
                CREATE $ep CONTENT {
                    title: $title,
                    content: $content,
                    scope: $target_scope,
                    vault_path: $vault_path,
                    processed_in_dream: false
                };
                
                CREATE $met CONTENT {
                    target_id: $ep,
                    utility_score: 1.0,
                    access_count: 0
                };
                
                COMMIT TRANSACTION;
            "
        };

        let metrics_uuid = Uuid::new_v4().to_string();
        let scope_val = episode.scope.clone().unwrap_or_else(|| "general".to_string());
        let vp_val = episode.vault_path.clone().unwrap_or_else(|| "".to_string());

        let _ = self.db.query(query_str)
            .bind(("ep_uuid", &ep_uuid))
            .bind(("metrics_uuid", &metrics_uuid))
            .bind(("title", &episode.title))
            .bind(("content", &episode.content))
            .bind(("target_scope", &scope_val))
            .bind(("vault_path", &vp_val))
            .await?
            .check().context("SurrealDB save_episode transaction failed")?;

        for entity in &episode.entities {
            let entity_query = "
                BEGIN TRANSACTION;
                LET $ent_id = type::thing('entity', $name);
                INSERT INTO entity (id, name, entity_type, summary, labels, scope)
                VALUES ($ent_id, $name, $entity_type, $summary, $labels, $target_scope)
                ON DUPLICATE KEY UPDATE
                    summary = $summary,
                    labels = $labels,
                    scope = $target_scope;
                
                -- Relate episode to entity
                LET $ep = type::thing('episode', $ep_uuid);
                RELATE $ep -> mentions -> $ent_id CONTENT {
                    created_at: time::now()
                };
                COMMIT TRANSACTION;
            ";
            let _ = self.db.query(entity_query)
                .bind(("name", &entity.name))
                .bind(("entity_type", &entity.entity_type))
                .bind(("summary", &entity.summary))
                .bind(("labels", &entity.labels))
                .bind(("target_scope", &scope_val))
                .bind(("ep_uuid", &ep_uuid))
                .await?
                .check().context("Entity relation query failed")?;
        }

        Ok(format!("episode:{}", ep_uuid))
    }

    async fn save_wisdom_rule(&self, rule: &WisdomRule) -> Result<String> {
        let mut rule_uuid = Uuid::new_v4().to_string();
        let mut is_update = false;

        if let Some(ref vp) = rule.vault_path {
            let check_query = "SELECT VALUE id FROM wisdom WHERE vault_path = $vault_path LIMIT 1;";
            let mut response = self.db.query(check_query).bind(("vault_path", vp)).await?;
            let ids: Option<surrealdb::sql::Thing> = response.take(0)?;
            if let Some(thing) = ids {
                rule_uuid = thing.id.to_string();
                is_update = true;
            }
        }

        let query_str = if is_update {
            "
                BEGIN TRANSACTION;
                LET $rule = type::thing('wisdom', $rule_uuid);
                UPDATE $rule CONTENT {
                    target_pattern: $target_pattern,
                    action_to_avoid: $action_to_avoid,
                    causal_explanation: $causal_explanation,
                    prescribed_remedy: $prescribed_remedy,
                    tier: $tier,
                    scope: $scope,
                    vault_path: $vault_path,
                    source_episodes: $source_episodes,
                    generator_name: $generator_name
                };
                COMMIT TRANSACTION;
            "
        } else {
            "
                BEGIN TRANSACTION;
                LET $rule = type::thing('wisdom', $rule_uuid);
                LET $met = type::thing('metrics', $metrics_uuid);
                
                CREATE $rule CONTENT {
                    target_pattern: $target_pattern,
                    action_to_avoid: $action_to_avoid,
                    causal_explanation: $causal_explanation,
                    prescribed_remedy: $prescribed_remedy,
                    tier: $tier,
                    scope: $scope,
                    vault_path: $vault_path,
                    source_episodes: $source_episodes,
                    generator_name: $generator_name
                };
                
                CREATE $met CONTENT {
                    target_id: $rule,
                    utility_score: 1.0,
                    access_count: 0
                };
                
                COMMIT TRANSACTION;
            "
        };

        let metrics_uuid = Uuid::new_v4().to_string();
        let vp_val = rule.vault_path.clone().unwrap_or_else(|| "".to_string());

        let _ = self.db.query(query_str)
            .bind(("rule_uuid", &rule_uuid))
            .bind(("metrics_uuid", &metrics_uuid))
            .bind(("target_pattern", &rule.target_pattern))
            .bind(("action_to_avoid", &rule.action_to_avoid))
            .bind(("causal_explanation", &rule.causal_explanation))
            .bind(("prescribed_remedy", &rule.prescribed_remedy))
            .bind(("tier", &rule.tier))
            .bind(("scope", &rule.scope))
            .bind(("vault_path", &vp_val))
            .bind(("source_episodes", &rule.source_episodes))
            .bind(("generator_name", &rule.generator_name))
            .await?
            .check().context("SurrealDB save_wisdom_rule transaction failed")?;

        Ok(format!("wisdom:{}", rule_uuid))
    }

    async fn search(&self, query: &str, scope: Option<&str>, limit: usize) -> Result<Vec<SearchResult>> {
        let sql = "
            SELECT id, title, content 
            FROM episode 
            WHERE (string::contains(title, $query) OR string::contains(content, $query)) 
              AND (scope = $target_scope OR $target_scope = NONE)
            LIMIT $limit;
        ";
        let scope_val = scope.map(|s| s.to_string());
        let mut response = self.db.query(sql)
            .bind(("query", query))
            .bind(("target_scope", scope_val))
            .bind(("limit", limit))
            .await?
            .check().context("Search query failed")?;

        let episodes: Vec<SearchRaw> = response.take(0)?;
        let results = episodes.into_iter().map(|ep| SearchResult {
            id: ep.id.to_string(),
            title: ep.title,
            content: ep.content,
            similarity: 1.0,
            utility: 1.0,
            tier: "Standard".to_string(),
        }).collect();

        Ok(results)
    }

    async fn get_wisdom(&self, query: &str, tier: &str, limit: usize) -> Result<Vec<WisdomRule>> {
        let sql = "
            SELECT * FROM wisdom 
            WHERE tier = $tier AND (string::contains(target_pattern, $query) OR string::contains(causal_explanation, $query))
            LIMIT $limit;
        ";
        let mut response = self.db.query(sql)
            .bind(("tier", tier))
            .bind(("query", query))
            .bind(("limit", limit))
            .await?
            .check().context("Get wisdom query failed")?;

        let wisdom: Vec<WisdomRule> = response.take(0)?;
        Ok(wisdom)
    }

    async fn record_feedback(&self, id: &str, success: bool) -> Result<()> {
        let parts: Vec<&str> = id.split(':').collect();
        if parts.len() != 2 {
            anyhow::bail!("Invalid Thing ID format: {}", id);
        }
        let thing_id = surrealdb::sql::Thing {
            tb: parts[0].to_string(),
            id: surrealdb::sql::Id::from(parts[1].to_string()),
        };
        
        let fetch_sql = "SELECT VALUE utility_score FROM metrics WHERE target_id = $target_id LIMIT 1;";
        let mut response = self.db.query(fetch_sql).bind(("target_id", &thing_id)).await?.check().context("Fetch metrics query failed")?;
        let utility_opt: Option<f64> = response.take(0)?;

        let prev_utility = utility_opt.unwrap_or(1.0);
        let reinforcement = if success { 1.0 } else { 0.0 };
        
        let new_utility = (0.3 * reinforcement) + (0.7 * prev_utility);

        let update_sql = "
            UPDATE metrics 
            SET utility_score = $new_utility, access_count = access_count + 1, last_accessed = time::now()
            WHERE target_id = $target_id;
        ";
        let _ = self.db.query(update_sql)
            .bind(("new_utility", new_utility))
            .bind(("target_id", &thing_id))
            .await?
            .check().context("Update metrics query failed")?;
        
        Ok(())
    }

    async fn apply_migrations(&self) -> Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::Entity;

    #[tokio::test]
    async fn test_surreal_db_operations() {
        let backend = SurrealBackend::new_in_memory().await.unwrap();
        backend.init().await.unwrap();

        let episode = EpisodeSave {
            title: "Test caching failure".to_string(),
            content: "Observed cache mismatch in redis client.".to_string(),
            entities: vec![Entity {
                name: "RedisClient".to_string(),
                entity_type: "class".to_string(),
                summary: "redis connection pool".to_string(),
                labels: vec!["caching".to_string()],
                scope: None,
                vault_path: None,
                embedding: None,
            }],
            scope: Some("testing".to_string()),
            vault_path: None,
        };

        let ep_id = backend.save_episode(&episode).await.unwrap();
        assert!(ep_id.contains("episode"));

        let all_eps: Vec<serde_json::Value> = backend.db.select("episode").await.unwrap();
        println!("DEBUG: All episodes in DB: {:?}", all_eps);

        let search_results = backend.search("redis", Some("testing"), 2).await.unwrap();
        assert_eq!(search_results.len(), 1);
        assert!(search_results[0].content.contains("redis"));

        backend.record_feedback(&ep_id, false).await.unwrap();
    }
}
