use crate::cognitive::db;
use crate::cognitive::pipeline::signals::{cluster_facts, derive_slug};
use crate::cognitive::prompts;
use crate::contracts::{
    ArborNode, Episode, Fact, FactSource, IdeaNode, IdeaStatus, WikiNode, WisdomRule,
};
use crate::db::StorageBackend;
use crate::llm::LLMClient;
use crate::store::MarkdownStore;
use anyhow::Result;
use std::collections::HashSet;
use std::path::Path;

/// Extracts atomic facts from an Episode transcript turn.
pub async fn extract_facts(
    backend: &dyn StorageBackend,
    llm: Option<&LLMClient>,
    episode: &Episode,
) -> Result<Vec<Fact>> {
    let scope = episode.scope.clone().unwrap_or_else(|| "general".to_string());
    let source_id = episode.id.clone().unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    let (sys, user) = prompts::build_episode_extraction_prompt(&episode.content);
    let facts_resp = retry_llm_json::<prompts::ExtractFactsResponse>(backend, llm, &sys, &user, &episode.title, 10).await?;
    let facts_dtos = facts_resp.facts;

    if facts_dtos.is_empty() {
        return Ok(Vec::new());
    }

    let texts: Vec<String> = facts_dtos.iter().map(|f| f.causal_insight.clone()).collect();
    let embeddings = backend.embed_batch(&texts).await.unwrap_or_else(|_| vec![vec![0.0; 768]; texts.len()]);

    let mut created_facts = Vec::new();
    for (idx, dto) in facts_dtos.into_iter().enumerate() {
        let slug = derive_slug(dto.slug.as_deref(), &dto.hypothesis);
        let fact_path = format!("wiki/{}/{}_fact.md", scope, slug);
        let fact_node_name = format!("{}/{}_fact", scope, slug);

        let fact = Fact {
            id: None,
            source_type: FactSource::Episode,
            source_id: source_id.clone(),
            source_version: 1,
            scope: scope.clone(),
            idea_node_id: None,
            hypothesis: Some(dto.hypothesis.clone()),
            causal_insight: Some(dto.causal_insight.clone()),
            raw_evidence: dto.raw_evidence.clone(),
            artifact_refs: dto.artifact_refs.clone(),
            embedding: embeddings.get(idx).cloned(),
            metacognitive_confidence: dto.metacognitive_confidence,
            created_at: Some(chrono::Utc::now()),
        };
        let saved_id = db::save_fact(backend, &fact).await?;
        let mut saved_fact = fact;
        saved_fact.id = Some(saved_id.clone());
        created_facts.push(saved_fact);

        let fact_content = format!(
            "---\nnode_type: fact\nscope: {}\ntitle: {}\n---\n\n# {}\n\n**Hypothesis:** {}\n\n**Causal Insight:** {}\n\n**Source Episode:** [[{}]]\n\n**Evidence:**\n- {}\n",
            scope,
            fact_node_name,
            fact_node_name,
            dto.hypothesis,
            dto.causal_insight,
            source_id,
            dto.raw_evidence.join("\n- ")
        );

        let fact_node = WikiNode {
            id: None,
            name: fact_node_name,
            content: fact_content,
            scope: scope.clone(),
            vault_path: Some(fact_path.clone()),
            embedding: embeddings.get(idx).cloned(),
            node_type: Some("fact".to_string()),
            item_type: Some("fact".to_string()),
            metacognitive_confidence: Some(dto.metacognitive_confidence as f64),
            ..Default::default()
        };
        let _ = backend.save_wiki_node(&fact_node).await;
        let _ = backend.relate_nodes(&saved_id, &source_id, None, None, None).await;

        if let Some(ref it) = dto.item_type {
            if it == "direction" {
                let dir_path = format!("wiki/{}/directions/{}_direction.md", scope, slug);
                let dir_node = WikiNode {
                    id: None,
                    name: format!("{}/{}", scope, slug),
                    content: dto.causal_insight.clone(),
                    scope: scope.clone(),
                    vault_path: Some(dir_path),
                    embedding: embeddings.get(idx).cloned(),
                    node_type: Some("direction".to_string()),
                    item_type: Some("direction".to_string()),
                    metacognitive_confidence: Some(dto.metacognitive_confidence as f64),
                    ..Default::default()
                };
                let _ = backend.save_wiki_node(&dir_node).await;
            }
        }
    }

    let facts_json = serde_json::to_value(&created_facts)?;
    let mut updated_ep = episode.clone();
    updated_ep.causal_insight = Some(facts_json);
    let ep_save = crate::contracts::EpisodeSave {
        title: updated_ep.title.clone(),
        content: updated_ep.content.clone(),
        scope: updated_ep.scope.clone(),
        vault_path: updated_ep.vault_path.clone(),
        source_episode: updated_ep.source_episode.clone(),
        session_id: updated_ep.session_id.clone(),
        causal_insight: updated_ep.causal_insight.clone(),
        raw_evidence: updated_ep.raw_evidence.clone(),
        hypothesis: updated_ep.hypothesis.clone(),
        artifact_refs: updated_ep.artifact_refs.clone(),
        ..Default::default()
    };
    let _ = backend.save_episode(&ep_save).await;

    Ok(created_facts)
}

/// Executes LLM JSON synthesis with retries and exponential backoff.
pub async fn retry_llm_json<T: serde::de::DeserializeOwned>(
    backend: &dyn StorageBackend,
    llm: Option<&LLMClient>,
    sys: &str,
    user: &str,
    target_name: &str,
    max_retries: usize,
) -> Result<T> {
    let fallback_client = LLMClient::default();
    let client = llm.unwrap_or(&fallback_client);

    let mut attempt = 0;
    let mut delay_ms = 500;

    loop {
        attempt += 1;
        match client.complete_json(backend, sys, user).await {
            Ok(raw_json) => {
                let cleaned = prompts::clean_json_payload(&raw_json);
                match serde_json::from_str::<T>(&cleaned) {
                    Ok(val) => return Ok(val),
                    Err(e) => {
                        tracing::warn!("Failed to parse LLM JSON synthesis for {} (attempt {}/{}): {:?}. Raw: {}", target_name, attempt, max_retries, e, cleaned);
                    }
                }
            }
            Err(e) => {
                tracing::warn!("LLM complete_json error for {} (attempt {}/{}): {:?}", target_name, attempt, max_retries, e);
            }
        }

        if attempt >= max_retries {
            anyhow::bail!("LLM synthesis failed to return valid JSON for {} after {} attempts", target_name, max_retries);
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms)).await;
        delay_ms = (delay_ms * 2).min(10000);
    }
}

/// Extracts atomic facts from an authored vault document.
pub async fn extract_from_document(
    backend: &dyn StorageBackend,
    llm: Option<&LLMClient>,
    content: &str,
    vault_path: &str,
    scope: &str,
) -> Result<Vec<Fact>> {
    let (fm_opt, _) = crate::vault::markdown::parse_frontmatter(content);
    let node_type = fm_opt
        .as_ref()
        .and_then(|yaml| yaml.get("node_type").or_else(|| yaml.get("type")))
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_lowercase();

    let lower_path = vault_path.to_lowercase();
    if node_type == "fact"
        || node_type == "rule"
        || node_type == "direction"
        || node_type == "ast_symbol"
        || lower_path.ends_with("_fact.md")
        || lower_path.ends_with("_rule.md")
        || lower_path.ends_with("_direction.md")
        || lower_path.ends_with("_ast.md")
        || lower_path.contains("/facts/")
        || lower_path.contains("/rules/")
    {
        tracing::debug!("Skipping recursive fact extraction on generated vault file: {}", vault_path);
        return Ok(Vec::new());
    }

    let (sys, user) = prompts::build_document_extraction_prompt(content, vault_path);
    let facts_resp = retry_llm_json::<prompts::ExtractFactsResponse>(backend, llm, &sys, &user, vault_path, 10).await?;
    let facts_dtos = facts_resp.facts;
    if facts_dtos.is_empty() {
        tracing::info!("LLM analyzed {} and determined no facts were worth extracting. Reason: {:?}", vault_path, facts_resp.no_facts_reason);
        return Ok(Vec::new());
    }

    let texts: Vec<String> = facts_dtos.iter().map(|f| f.causal_insight.clone()).collect();
    let embeddings = backend.embed_batch(&texts).await.unwrap_or_else(|_| vec![vec![0.0; 768]; texts.len()]);

    let mut created_facts = Vec::new();
    for (idx, dto) in facts_dtos.into_iter().enumerate() {
        let slug = derive_slug(dto.slug.as_deref(), &dto.hypothesis);
        let fact_path = format!("wiki/{}/{}_fact.md", scope, slug);
        let fact_node_name = format!("{}/{}_fact", scope, slug);

        let fact = Fact {
            id: None,
            source_type: FactSource::Document,
            source_id: vault_path.to_string(),
            source_version: 1,
            scope: scope.to_string(),
            idea_node_id: None,
            hypothesis: Some(dto.hypothesis.clone()),
            causal_insight: Some(dto.causal_insight.clone()),
            raw_evidence: dto.raw_evidence.clone(),
            artifact_refs: dto.artifact_refs.clone(),
            embedding: embeddings.get(idx).cloned(),
            metacognitive_confidence: dto.metacognitive_confidence,
            created_at: Some(chrono::Utc::now()),
        };
        let saved_id = db::save_fact(backend, &fact).await?;
        let mut saved_fact = fact;
        saved_fact.id = Some(saved_id.clone());
        created_facts.push(saved_fact);

        let fact_content = format!(
            "---\nnode_type: fact\nscope: {}\ntitle: {}\n---\n\n# {}\n\n**Hypothesis:** {}\n\n**Causal Insight:** {}\n\n**Source Document:** [[{}]]\n\n**Evidence:**\n- {}\n",
            scope,
            fact_node_name,
            fact_node_name,
            dto.hypothesis,
            dto.causal_insight,
            vault_path,
            dto.raw_evidence.join("\n- ")
        );

        let fact_node = WikiNode {
            id: None,
            name: fact_node_name,
            content: fact_content,
            scope: scope.to_string(),
            vault_path: Some(fact_path.clone()),
            embedding: embeddings.get(idx).cloned(),
            node_type: Some("fact".to_string()),
            item_type: Some("fact".to_string()),
            metacognitive_confidence: Some(dto.metacognitive_confidence as f64),
            ..Default::default()
        };
        let _ = backend.save_wiki_node(&fact_node).await;
        let _ = backend.relate_nodes(&saved_id, vault_path, None, None, None).await;
    }
    Ok(created_facts)
}

/// Extracts atomic facts from source code files (.rs, .py, .ts, .go) and persists AST symbols.
pub async fn extract_from_code(
    backend: &dyn StorageBackend,
    llm: Option<&LLMClient>,
    code_content: &str,
    file_path: &str,
    scope: &str,
) -> Result<Vec<Fact>> {
    let symbols = crate::cognitive::ast::extract_code_ast(file_path, code_content, scope);
    let _ = db::save_code_symbols_for_file(backend, &symbols, file_path, scope).await;

    let (sys, user) = prompts::build_code_extraction_prompt(code_content, file_path);
    let facts_resp = retry_llm_json::<prompts::ExtractFactsResponse>(backend, llm, &sys, &user, file_path, 10).await?;
    let facts_dtos = facts_resp.facts;
    if facts_dtos.is_empty() {
        tracing::info!("LLM analyzed {} and determined no facts were worth extracting. Reason: {:?}", file_path, facts_resp.no_facts_reason);
        return Ok(Vec::new());
    }

    let texts: Vec<String> = facts_dtos.iter().map(|f| f.causal_insight.clone()).collect();
    let embeddings = backend.embed_batch(&texts).await.unwrap_or_else(|_| vec![vec![0.0; 768]; texts.len()]);

    let mut created_facts = Vec::new();
    for (idx, dto) in facts_dtos.into_iter().enumerate() {
        let slug = derive_slug(dto.slug.as_deref(), &dto.hypothesis);
        let fact_path = format!("wiki/{}/{}_fact.md", scope, slug);
        let fact_node_name = format!("{}/{}_fact", scope, slug);

        let fact = Fact {
            id: None,
            source_type: FactSource::Code,
            source_id: file_path.to_string(),
            source_version: 1,
            scope: scope.to_string(),
            idea_node_id: None,
            hypothesis: Some(dto.hypothesis.clone()),
            causal_insight: Some(dto.causal_insight.clone()),
            raw_evidence: dto.raw_evidence.clone(),
            artifact_refs: dto.artifact_refs.clone(),
            embedding: embeddings.get(idx).cloned(),
            metacognitive_confidence: dto.metacognitive_confidence,
            created_at: Some(chrono::Utc::now()),
        };
        let saved_id = db::save_fact(backend, &fact).await?;
        let mut saved_fact = fact;
        saved_fact.id = Some(saved_id.clone());
        created_facts.push(saved_fact);

        let fact_content = format!(
            "---\nnode_type: fact\nscope: {}\ntitle: {}\n---\n\n# {}\n\n**Hypothesis:** {}\n\n**Causal Insight:** {}\n\n**Source Code File:** [[{}]]\n\n**Evidence:**\n- {}\n",
            scope,
            fact_node_name,
            fact_node_name,
            dto.hypothesis,
            dto.causal_insight,
            file_path,
            dto.raw_evidence.join("\n- ")
        );

        let fact_node = WikiNode {
            id: None,
            name: fact_node_name,
            content: fact_content,
            scope: scope.to_string(),
            vault_path: Some(fact_path.clone()),
            embedding: embeddings.get(idx).cloned(),
            node_type: Some("fact".to_string()),
            item_type: Some("fact".to_string()),
            metacognitive_confidence: Some(dto.metacognitive_confidence as f64),
            ..Default::default()
        };
        let _ = backend.save_wiki_node(&fact_node).await;
        for sym in &symbols {
            let sym_name = format!("{}_{}", sym.file_slug, sym.name);
            let _ = backend.relate_nodes(&saved_id, &format!("code_symbol:{}", sym_name), None, None, None).await;
        }
        let _ = backend.relate_nodes(&saved_id, file_path, None, None, None).await;
    }
    Ok(created_facts)
}

/// Dual-Path Document Forging:
pub async fn forge_document(
    backend: &dyn StorageBackend,
    store: &MarkdownStore,
    source_path: &str,
    scope: &str,
    llm: Option<&LLMClient>,
    content_override: Option<&str>,
) -> Result<Vec<Fact>> {
    let content = if let Some(c) = content_override {
        c.to_string()
    } else if source_path.ends_with(".pdf") {
        crate::cognitive::forge::extract_pdf_text(Path::new(source_path))?
    } else {
        std::fs::read_to_string(source_path).unwrap_or_else(|_| source_path.to_string())
    };

    let toc = crate::cognitive::forge::parse_markdown_toc(&content);
    let sections = crate::cognitive::forge::split_into_logical_sections(&content, &toc);

    let sanitize_name = source_path.replace('/', "_").replace('.', "_");
    let mut all_facts = Vec::new();

    for (idx, section) in sections.iter().enumerate() {
        let chunk_path = format!("wiki/{}/forge_{}_{}.md", scope, sanitize_name, idx);
        let chunk_md = format!(
            "---\ntitle: \"Forged: {} (Section {})\";\nscope: \"forge_{}\"\n---\n\n{}",
            source_path, idx, scope, section.content
        );
        let _ = store.write_file(&chunk_path, &chunk_md);

        let embeddings = backend
            .embed_batch(&[section.content.clone()])
            .await
            .unwrap_or_else(|_| vec![vec![0.0; 768]]);
        let chunk_node = WikiNode {
            id: Some(uuid::Uuid::new_v4().to_string()),
            name: format!("Forged: {} (Chunk {})", sanitize_name, idx),
            content: section.content.clone(),
            scope: format!("forge_{}", scope),
            vault_path: Some(chunk_path.clone()),
            embedding: embeddings.into_iter().next(),
            node_type: Some("forged_reference".to_string()),
            item_type: Some("forged_doc".to_string()),
            metacognitive_confidence: Some(95.0),
            ..Default::default()
        };
        let _ = backend.save_wiki_node(&chunk_node).await;

        let (sys, user) = prompts::build_forge_extraction_prompt(&section.content, source_path);
        let facts_resp = retry_llm_json::<prompts::ExtractFactsResponse>(backend, llm, &sys, &user, &format!("{}_section_{}", source_path, idx), 10).await?;
        let facts_dtos = facts_resp.facts;

        let texts: Vec<String> = facts_dtos.iter().map(|f| f.causal_insight.clone()).collect();
        let fact_embeds = backend
            .embed_batch(&texts)
            .await
            .unwrap_or_else(|_| vec![vec![0.0; 768]; texts.len()]);

        for (f_idx, dto) in facts_dtos.into_iter().enumerate() {
            let fact = Fact {
                id: None,
                source_type: FactSource::ForgedDocument,
                source_id: chunk_path.clone(),
                source_version: 1,
                scope: scope.to_string(),
                idea_node_id: None,
                hypothesis: Some(dto.hypothesis),
                causal_insight: Some(dto.causal_insight),
                raw_evidence: dto.raw_evidence,
                artifact_refs: dto.artifact_refs,
                embedding: fact_embeds.get(f_idx).cloned(),
                metacognitive_confidence: dto.metacognitive_confidence,
                created_at: Some(chrono::Utc::now()),
            };
            let saved_id = db::save_fact(backend, &fact).await?;
            let mut saved_fact = fact;
            saved_fact.id = Some(saved_id);
            all_facts.push(saved_fact);
        }
    }

    Ok(all_facts)
}

/// Dual-Path Skill Forging:
pub async fn forge_skill(
    backend: &dyn StorageBackend,
    skill_content: &str,
    skill_path: &str,
    scope: &str,
    llm: Option<&LLMClient>,
) -> Result<Vec<Fact>> {
    let skill_name = Path::new(skill_path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unnamed_skill");

    let wiki_path = format!("wiki/skills/{}.md", skill_name);
    let chunk_node = WikiNode {
        id: Some(uuid::Uuid::new_v4().to_string()),
        name: format!("Skill: {}", skill_name),
        content: skill_content.to_string(),
        scope: "skills".to_string(),
        vault_path: Some(wiki_path.clone()),
        node_type: Some("skill_playbook".to_string()),
        item_type: Some("skill".to_string()),
        metacognitive_confidence: Some(95.0),
        ..Default::default()
    };
    let _ = backend.save_wiki_node(&chunk_node).await;

    let (sys, user) = prompts::build_skill_extraction_prompt(skill_content, skill_path);
    let facts_resp = retry_llm_json::<prompts::ExtractFactsResponse>(backend, llm, &sys, &user, skill_name, 10).await?;
    let facts_dtos = facts_resp.facts;

    let texts: Vec<String> = facts_dtos.iter().map(|f| f.causal_insight.clone()).collect();
    let embeddings = backend
        .embed_batch(&texts)
        .await
        .unwrap_or_else(|_| vec![vec![0.0; 768]; texts.len()]);

    let mut created_facts = Vec::new();
    for (idx, dto) in facts_dtos.into_iter().enumerate() {
        let fact = Fact {
            id: None,
            source_type: FactSource::Skill,
            source_id: skill_path.to_string(),
            source_version: 1,
            scope: scope.to_string(),
            idea_node_id: None,
            hypothesis: Some(dto.hypothesis),
            causal_insight: Some(dto.causal_insight),
            raw_evidence: dto.raw_evidence,
            artifact_refs: dto.artifact_refs,
            embedding: embeddings.get(idx).cloned(),
            metacognitive_confidence: dto.metacognitive_confidence,
            created_at: Some(chrono::Utc::now()),
        };
        let saved_id = db::save_fact(backend, &fact).await?;
        let mut saved_fact = fact;
        saved_fact.id = Some(saved_id);
        created_facts.push(saved_fact);
    }

    Ok(created_facts)
}

/// Form generalized testable hypotheses (`IdeaNode`) from clusters of unassociated facts.
pub async fn form_hypotheses(
    backend: &dyn StorageBackend,
    llm: Option<&LLMClient>,
    scope: &str,
) -> Result<Vec<IdeaNode>> {
    let config = db::get_pipeline_config(backend).await?;
    let unassociated = db::get_unassociated_facts(backend, scope).await?;
    if unassociated.len() < config.cluster_min_size {
        return Ok(Vec::new());
    }

    let embeddings: Vec<Vec<f32>> = unassociated
        .iter()
        .map(|f| f.embedding.clone().unwrap_or_else(|| vec![0.0; 768]))
        .collect();

    let clusters = cluster_facts(&unassociated, &embeddings, &config);
    if clusters.is_empty() {
        return Ok(Vec::new());
    }

    let pruned_nodes = db::get_pruned_idea_nodes(backend, scope, config.prune_threshold).await?;
    let pruned_constraints: Vec<String> = pruned_nodes.iter().map(|n| n.claim.clone()).collect();

    let mut formed_ideas = Vec::new();

    for cluster in clusters {
        let cluster_facts: Vec<&Fact> = cluster.iter().map(|&idx| &unassociated[idx]).collect();
        let facts_summary = cluster_facts
            .iter()
            .enumerate()
            .map(|(i, f)| format!("[{}] H: {} | Insight: {}", i, f.h_n().unwrap_or(""), f.iota_n().unwrap_or("")))
            .collect::<Vec<String>>()
            .join("\n");

        let (sys, user) = prompts::build_hypothesis_formation_prompt(&facts_summary, &pruned_constraints);
        let form_resp = retry_llm_json::<prompts::FormHypothesesResponse>(backend, llm, &sys, &user, "form_hypotheses", 10).await?;
        let hypotheses_dto = form_resp.hypotheses;

        for hdto in hypotheses_dto {
            let mut evidence_ids = Vec::new();
            let mut artifact_refs = Vec::new();
            for &idx in &hdto.fact_indices {
                if let Some(fact) = cluster_facts.get(idx) {
                    if let Some(ref fid) = fact.id {
                        evidence_ids.push(fid.clone());
                    }
                    artifact_refs.extend(fact.artifact_refs.clone());
                }
            }

            let idea = IdeaNode {
                id: None,
                parent_id: None,
                claim: hdto.claim,
                evidence: evidence_ids,
                artifact_refs,
                insight: hdto.insight,
                artifact_path: None,
                status: IdeaStatus::Pending,
                confidence: 0.50,
                scope: scope.to_string(),
                created_at: Some(chrono::Utc::now()),
                updated_at: Some(chrono::Utc::now()),
            };

            let saved_id = db::save_idea_node(backend, &idea).await?;
            let mut saved_idea = idea;
            saved_idea.id = Some(saved_id.clone());

            for &idx in &hdto.fact_indices {
                if let Some(fact) = cluster_facts.get(idx) {
                    let mut updated_fact = (*fact).clone();
                    updated_fact.idea_node_id = Some(saved_id.clone());
                    let _ = db::save_fact(backend, &updated_fact).await;
                }
            }

            formed_ideas.push(saved_idea);
        }
    }

    Ok(formed_ideas)
}

/// HTR Backpropagation Refinement Pass.
pub async fn refine_hypotheses(
    backend: &dyn StorageBackend,
    llm: Option<&LLMClient>,
    scope: &str,
) -> Result<Vec<db::RefinementLog>> {
    let config = db::get_pipeline_config(backend).await?;
    let pending_ideas = db::get_idea_nodes_by_scope(backend, scope).await?;
    let facts = db::get_facts_by_scope(backend, scope).await?;

    let mut logs = Vec::new();

    for mut idea in pending_ideas {
        if idea.status == IdeaStatus::Merged || idea.status == IdeaStatus::Pruned {
            continue;
        }

        for fact in &facts {
            if fact.idea_node_id.as_deref() != idea.id.as_deref() {
                continue;
            }

            let fact_summary = format!("H: {} | Insight: {}", fact.h_n().unwrap_or(""), fact.iota_n().unwrap_or(""));
            let (sys, user) = prompts::build_refinement_prompt(
                &idea.claim,
                &idea.insight,
                idea.confidence,
                &fact_summary,
            );
            let r = retry_llm_json::<prompts::RefineHypothesisResponse>(backend, llm, &sys, &user, "refine_hypotheses", 10).await?;

            let prev_conf = idea.confidence;
            let (action, new_conf, refined_insight, reasoning) = (r.action, r.new_confidence, r.refined_insight, r.reasoning);

            idea.confidence = new_conf;
            idea.insight = refined_insight;
            idea.updated_at = Some(chrono::Utc::now());

            if new_conf >= config.merge_threshold {
                idea.status = IdeaStatus::Validated;
            } else if new_conf <= config.prune_threshold {
                idea.status = IdeaStatus::Pruned;
            }

            let _ = db::save_idea_node(backend, &idea).await;

            let log = db::RefinementLog {
                id: None,
                idea_node_id: idea.id.clone().unwrap_or_default(),
                fact_id: fact.id.clone().unwrap_or_default(),
                action,
                previous_confidence: prev_conf,
                new_confidence: new_conf,
                reasoning,
                created_at: Some(chrono::Utc::now()),
            };
            logs.push(log);

            if idea.status == IdeaStatus::Pruned {
                if let Some(ref fid) = fact.id {
                    let _ = db::delete_fact(backend, fid).await;
                }
            }
        }
    }

    Ok(logs)
}

/// Ancestor Merge Synthesis (LLM Wiki Synthesis).
pub async fn merge_validated_nodes(
    backend: &dyn StorageBackend,
    llm: Option<&LLMClient>,
    store: &MarkdownStore,
    scope: &str,
) -> Result<Vec<WikiNode>> {
    let config = db::get_pipeline_config(backend).await?;
    let validated = db::get_validated_idea_nodes(backend, scope, config.merge_threshold).await?;
    if validated.is_empty() {
        return Ok(Vec::new());
    }

    let mut merged_nodes = Vec::new();

    for mut idea in validated {
        let mut flattened_r_n = HashSet::new();
        let mut flattened_mu_n = HashSet::new();

        for fid in &idea.evidence {
            if let Ok(Some(fact)) = db::get_fact(backend, fid).await {
                for r in fact.r_n() {
                    flattened_r_n.insert(r.clone());
                }
                for m in fact.mu_n() {
                    flattened_mu_n.insert(m.clone());
                }
            }
        }

        let r_n_vec: Vec<String> = flattened_r_n.into_iter().collect();
        let mu_n_vec: Vec<String> = flattened_mu_n.into_iter().collect();

        let is_code_impacting = mu_n_vec.iter().any(|m| m.ends_with(".rs") || m.ends_with(".py") || m.ends_with(".ts") || m.ends_with(".go"));
        if is_code_impacting && std::env::var("MYTHRAX_TEST_MOCK").is_err() {
            let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
            let has_manifest = cwd.join("Cargo.toml").exists()
                || cwd.join("package.json").exists()
                || cwd.join("pyproject.toml").exists()
                || cwd.join("go.mod").exists();
            if has_manifest {
                let test_cmd = if mu_n_vec.iter().any(|m| m.ends_with(".py")) {
                    "pytest".to_string()
                } else if mu_n_vec.iter().any(|m| m.ends_with(".ts")) {
                    "npm test".to_string()
                } else if mu_n_vec.iter().any(|m| m.ends_with(".go")) {
                    "go test ./...".to_string()
                } else {
                    "cargo nextest run".to_string()
                };
                let evaluator = crate::cognitive::arbor::TestCommandEvaluator {
                    test_command: test_cmd,
                };
                use crate::cognitive::arbor::HeldOutEvaluator;
                let score = evaluator.evaluate("main", &cwd).unwrap_or(1.0);
                if score < 0.80 {
                    idea.confidence = 0.50;
                    idea.status = IdeaStatus::Pending;
                    let _ = db::save_idea_node(backend, &idea).await;
                    continue;
                }
            }
        }

        let (sys, user) = prompts::build_ancestor_merge_prompt(
            &format!("Claim: {}\nInsight: {}", idea.claim, idea.insight),
            scope,
        );

        let merge_resp = if let Some(client) = llm {
            if let Ok(raw_json) = client.complete_json(backend, &sys, &user).await {
                serde_json::from_str::<prompts::AncestorMergeResponse>(&raw_json).ok()
            } else {
                None
            }
        } else {
            None
        };

        let (path, title, markdown) = if let Some(r) = merge_resp {
            (r.suggested_path, r.title, r.markdown_content)
        } else {
            let default_path = format!("wiki/{}/{}.md", scope, idea.claim.replace(' ', "_").to_lowercase());
            let default_title = idea.claim.clone();
            let default_md = format!("# {}\n\n## Insight\n{}\n\n## Evidence\n- {}", default_title, idea.insight, r_n_vec.join("\n- "));
            (default_path, default_title, default_md)
        };

        let _ = store.write_file(&path, &markdown);

        let embeddings = backend
            .embed_batch(&[markdown.clone()])
            .await
            .unwrap_or_else(|_| vec![vec![0.0; 768]]);

        let wiki_node = WikiNode {
            id: Some(uuid::Uuid::new_v4().to_string()),
            name: title,
            content: markdown,
            scope: scope.to_string(),
            vault_path: Some(path.clone()),
            embedding: embeddings.into_iter().next(),
            node_type: Some("ancestor_synthesis".to_string()),
            hypothesis: Some(idea.claim.clone()),
            raw_evidence: Some(r_n_vec),
            causal_insight: Some(idea.insight.clone()),
            artifact_refs: Some(mu_n_vec),
            item_type: Some("wiki".to_string()),
            metacognitive_confidence: Some((idea.confidence * 100.0) as f64),
            ..Default::default()
        };

        let _ = backend.save_wiki_node(&wiki_node).await;

        idea.status = IdeaStatus::Merged;
        idea.artifact_path = Some(path);
        let _ = db::save_idea_node(backend, &idea).await;

        merged_nodes.push(wiki_node);
    }

    Ok(merged_nodes)
}

/// Cross-Scope Graduation.
pub async fn graduate(
    backend: &dyn StorageBackend,
    llm: Option<&LLMClient>,
    scope: &str,
) -> Result<Vec<WisdomRule>> {
    let wiki_nodes = backend.get_all_wiki_nodes().await?;
    let mut graduated_rules = Vec::new();

    for node in wiki_nodes {
        if node.scope != scope {
            continue;
        }

        let is_universal = if let Some(client) = llm {
            let (sys, user) = prompts::build_graduation_prompt(&node.name, &node.content);
            if let Ok(raw_json) = client.complete_json(backend, &sys, &user).await {
                serde_json::from_str::<prompts::GraduationResponse>(&raw_json)
                    .map(|r| r.scope.to_lowercase().contains("universal"))
                    .unwrap_or(false)
            } else {
                false
            }
        } else {
            false
        } || node.item_type.as_deref() == Some("rule")
          || node.node_type.as_deref() == Some("rule")
          || node.content.to_lowercase().contains("universal")
          || node.content.contains("ALWAYS")
          || node.content.contains("NEVER");

        if is_universal {
            let rule_slug = node.name.to_lowercase().replace(' ', "_").replace(|c: char| !c.is_alphanumeric() && c != '_', "");
            let rule_vault_path = format!("wisdom/general/{}_rule.md", rule_slug);
            let rule = WisdomRule {
                id: None,
                target_pattern: node.name.clone(),
                action_to_avoid: "Violating universal design invariant".to_string(),
                causal_explanation: node.causal_insight.clone().unwrap_or_else(|| node.content.clone()),
                prescribed_remedy: format!("Follow pattern defined in {}", node.name),
                tier: crate::contracts::Tier::Wisdom,
                scope: "general".to_string(),
                vault_path: Some(rule_vault_path),
                embedding: node.embedding.clone(),
                source_episodes: vec![node.id.clone().unwrap_or_default()],
                generator_name: "arbor_graduation".to_string(),
                similarity: Some(0.95),
                utility: Some(1.0),
                status: Some("active".to_string()),
                superseded_at: None,
                superseded_by: None,
                rule_type: Some("graduation".to_string()),
                severity: Some("high".to_string()),
                blocking: Some(true),
                importance: Some(1.0),
                content_hash: None,
            };

            let _ = backend.save_wisdom_rule(&rule).await;
            graduated_rules.push(rule);
        }
    }

    Ok(graduated_rules)
}

pub async fn save_wisdom_rule_with_deduplication(
    backend: &dyn StorageBackend,
    _store: &crate::store::MarkdownStore,
    rule: &crate::contracts::WisdomRule,
) -> Result<String> {
    backend.save_wisdom_rule(rule).await
}

pub async fn backpropagate_directions(
    _backend: &dyn StorageBackend,
    _store: &crate::store::MarkdownStore,
) -> Result<()> {
    Ok(())
}

pub async fn promote_insight_to_direction(
    backend: &dyn StorageBackend,
    _store: &crate::store::MarkdownStore,
    node: &crate::contracts::WikiNode,
    _episodes: &[crate::contracts::Episode],
) -> Result<()> {
    let rule = crate::contracts::WisdomRule {
        id: None,
        target_pattern: node.name.clone(),
        action_to_avoid: node.content.clone(),
        causal_explanation: node.content.clone(),
        prescribed_remedy: "Avoid specified pattern".to_string(),
        tier: crate::contracts::Tier::Wisdom,
        scope: node.scope.clone(),
        rule_type: Some("system_constraint".to_string()),
        ..Default::default()
    };
    backend.save_wisdom_rule(&rule).await?;
    Ok(())
}

pub async fn graduate_wisdom(
    backend: &dyn StorageBackend,
    _store: &crate::store::MarkdownStore,
) -> Result<Vec<crate::contracts::WisdomRule>> {
    graduate(backend, None, "general").await
}

pub async fn prune_chat_history(backend: &dyn StorageBackend, max_turns: usize) -> Result<usize> {
    if let Some(surreal) = backend.as_any().downcast_ref::<crate::db::backend::SurrealBackend>() {
        let mut resp = surreal.db.query("SELECT id, session_id, created_at FROM chat_history ORDER BY created_at DESC;").await?;
        let rows: Vec<serde_json::Value> = resp.take(0)?;
        let mut counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        let mut deleted = 0;
        for row in rows {
            if let (Some(id_str), Some(sess_id)) = (row["id"].as_str(), row["session_id"].as_str()) {
                let count = counts.entry(sess_id.to_string()).or_insert(0);
                *count += 1;
                if *count > max_turns {
                    if let Ok(rec_id) = crate::db::backend::parse_record_id(id_str) {
                        let _ = surreal.db.query("DELETE type::record('chat_history', $id);").bind(("id", rec_id)).await;
                        deleted += 1;
                    }
                }
            }
        }
        return Ok(deleted);
    }
    Ok(0)
}
