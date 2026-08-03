use crate::db::StorageBackend;
use crate::llm::LLMClient;
use crate::store::MarkdownStore;
use anyhow::Result;
use std::sync::Arc;

/// Service layer orchestrating cognitive pipeline passes (hypotheses, refinement, graduation).
pub struct CognitiveService {
    pub backend: Arc<dyn StorageBackend>,
    pub store: Arc<MarkdownStore>,
    pub llm: Option<LLMClient>,
}

impl CognitiveService {
    /// Creates a new CognitiveService instance.
    pub fn new(
        backend: Arc<dyn StorageBackend>,
        store: Arc<MarkdownStore>,
        llm: Option<LLMClient>,
    ) -> Self {
        Self {
            backend,
            store,
            llm,
        }
    }

    /// Evaluates facts against pending hypotheses and updates confidence scores.
    pub async fn refine(&self, scope: &str) -> Result<usize> {
        let logs = crate::cognitive::pipeline::refine_hypotheses(
            &*self.backend,
            self.llm.as_ref(),
            scope,
        )
        .await?;
        Ok(logs.len())
    }

    pub async fn form_hypotheses(&self, scope: &str) -> Result<usize> {
        let ideas = crate::cognitive::pipeline::form_hypotheses(
            &*self.backend,
            self.llm.as_ref(),
            scope,
        )
        .await?;
        Ok(ideas.len())
    }

    pub async fn graduate(&self, scope: &str) -> Result<usize> {
        let rules = crate::cognitive::pipeline::graduate(
            &*self.backend,
            self.llm.as_ref(),
            scope,
        )
        .await?;
        Ok(rules.len())
    }
}
