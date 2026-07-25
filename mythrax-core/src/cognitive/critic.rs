use crate::cognitive::arbor::ArborLlmClient;
use crate::db::StorageBackend;
use anyhow::{Context, Result};

pub struct ArborCritic;

impl Default for ArborCritic {
    fn default() -> Self {
        Self::new()
    }
}

impl ArborCritic {
    pub fn new() -> Self {
        Self
    }

    pub async fn evaluate<L: ArborLlmClient>(
        &self,
        db: &dyn StorageBackend,
        llm_client: &L,
        run_logs: &str,
    ) -> Result<CriticOutput> {
        let prompt = format!(
            "Analyze the following execution/test run logs and evaluate the output.\n\
             Return a JSON object containing the fields 'success' (boolean indicating if the tests passed), \
             'score' (float rating the performance improvement or correctness, e.g. from 0.0 to 100.0), \
             and 'insight' (string summarizing the lessons learned or failure reason).\n\n\
             Logs:\n{}\n\n\
             JSON Response:",
            run_logs
        );

        let response_str = llm_client.evaluate_run(db, &prompt).await?;
        let output: CriticOutput = serde_json::from_str(&response_str).context(format!(
            "Failed to parse CriticOutput from LLM response: {}",
            response_str
        ))?;

        Ok(output)
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CriticOutput {
    pub success: bool,
    pub score: f32,
    pub insight: String,
}
