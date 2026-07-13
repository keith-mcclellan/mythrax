use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use anyhow::Result;
use crate::db::backend::{SurrealBackend, format_record_id};
use surrealdb_types::SurrealValue;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum CognitiveTaskType {
    Synthesis,
    Compaction,
    Extraction,
    Refinement,
    Custom(String),
}

impl std::fmt::Display for CognitiveTaskType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Synthesis => write!(f, "Synthesis"),
            Self::Compaction => write!(f, "Compaction"),
            Self::Extraction => write!(f, "Extraction"),
            Self::Refinement => write!(f, "Refinement"),
            Self::Custom(s) => write!(f, "{}", s),
        }
    }
}

impl std::str::FromStr for CognitiveTaskType {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "Synthesis" => Ok(Self::Synthesis),
            "Compaction" => Ok(Self::Compaction),
            "Extraction" => Ok(Self::Extraction),
            "Refinement" => Ok(Self::Refinement),
            other => Ok(Self::Custom(other.to_string())),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ExpectedFormat {
    Json(Option<String>),
    Any,
}

impl std::fmt::Display for ExpectedFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Json(Some(schema)) => write!(f, "Json({})", schema),
            Self::Json(None) => write!(f, "Json"),
            Self::Any => write!(f, "Any"),
        }
    }
}

impl std::str::FromStr for ExpectedFormat {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.starts_with("Json(") && s.ends_with(')') {
            let schema = &s[5..s.len() - 1];
            Ok(Self::Json(Some(schema.to_string())))
        } else if s == "Json" {
            Ok(Self::Json(None))
        } else if s == "Any" {
            Ok(Self::Any)
        } else {
            Ok(Self::Any)
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum Priority {
    Deferred,
    Normal,
    Immediate,
}

impl std::fmt::Display for Priority {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Immediate => write!(f, "Immediate"),
            Self::Normal => write!(f, "Normal"),
            Self::Deferred => write!(f, "Deferred"),
        }
    }
}

impl std::str::FromStr for Priority {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "Immediate" => Ok(Self::Immediate),
            "Normal" => Ok(Self::Normal),
            "Deferred" => Ok(Self::Deferred),
            other => anyhow::bail!("Invalid priority: {}", other),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum TaskStatus {
    Pending,
    Injected,
    Completed,
    Failed,
    Expired,
}

impl std::fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pending => write!(f, "Pending"),
            Self::Injected => write!(f, "Injected"),
            Self::Completed => write!(f, "Completed"),
            Self::Failed => write!(f, "Failed"),
            Self::Expired => write!(f, "Expired"),
        }
    }
}

impl std::str::FromStr for TaskStatus {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "Pending" => Ok(Self::Pending),
            "Injected" => Ok(Self::Injected),
            "Completed" => Ok(Self::Completed),
            "Failed" => Ok(Self::Failed),
            "Expired" => Ok(Self::Expired),
            other => anyhow::bail!("Invalid status: {}", other),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CognitiveTask {
    pub id: String, // String record ID, e.g. "cognitive_task:uuid"
    pub task_type: String,
    pub prompt: String,
    pub system_instruction: String,
    pub expected_format: String,
    pub priority: String,
    pub created_at: DateTime<Utc>,
    pub status: String,
    pub result: Option<String>,
    pub ttl_minutes: i64,
    pub injected_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue)]
pub struct CognitiveTaskRaw {
    pub id: surrealdb::types::RecordId,
    pub task_type: String,
    pub prompt: String,
    pub system_instruction: String,
    pub expected_format: String,
    pub priority: String,
    pub created_at: DateTime<Utc>,
    pub status: String,
    pub result: Option<String>,
    pub ttl_minutes: i64,
    pub injected_at: Option<DateTime<Utc>>,
}

impl From<CognitiveTaskRaw> for CognitiveTask {
    fn from(raw: CognitiveTaskRaw) -> Self {
        CognitiveTask {
            id: format_record_id(&raw.id),
            task_type: raw.task_type,
            prompt: raw.prompt,
            system_instruction: raw.system_instruction,
            expected_format: raw.expected_format,
            priority: raw.priority,
            created_at: raw.created_at,
            status: raw.status,
            result: raw.result,
            ttl_minutes: raw.ttl_minutes,
            injected_at: raw.injected_at,
        }
    }
}

impl SurrealBackend {
    pub async fn create_cognitive_task(&self, task: &CognitiveTask) -> Result<String> {
        let query_str = "
            CREATE type::record('cognitive_task', $id_val) CONTENT {
                task_type: $task_type,
                prompt: $prompt,
                system_instruction: $system_instruction,
                expected_format: $expected_format,
                priority: $priority,
                created_at: $created_at,
                status: $status,
                result: $result,
                ttl_minutes: $ttl_minutes,
                injected_at: $injected_at
            };
        ";
        let id_val = if task.id.contains(':') {
            task.id.splitn(2, ':').collect::<Vec<&str>>()[1].to_string()
        } else {
            task.id.clone()
        };
        let mut response = self.db.query(query_str)
            .bind(("id_val", id_val.as_str()))
            .bind(("task_type", task.task_type.as_str()))
            .bind(("prompt", task.prompt.as_str()))
            .bind(("system_instruction", task.system_instruction.as_str()))
            .bind(("expected_format", task.expected_format.as_str()))
            .bind(("priority", task.priority.as_str()))
            .bind(("created_at", task.created_at))
            .bind(("status", task.status.as_str()))
            .bind(("result", task.result.as_deref()))
            .bind(("ttl_minutes", task.ttl_minutes))
            .bind(("injected_at", task.injected_at))
            .await?;
        
        let created: Option<CognitiveTaskRaw> = response.take(0)?;
        if let Some(c) = created {
            Ok(format_record_id(&c.id))
        } else {
            anyhow::bail!("Failed to create cognitive task")
        }
    }

    pub async fn get_cognitive_task(&self, id: &str) -> Result<Option<CognitiveTask>> {
        let rec_id = if id.contains(':') {
            id.to_string()
        } else {
            format!("cognitive_task:{}", id)
        };
        let query_str = "SELECT * FROM type::record('cognitive_task', $id_val) LIMIT 1;";
        let id_val = rec_id.splitn(2, ':').collect::<Vec<&str>>()[1].to_string();
        let mut response = self.db.query(query_str)
            .bind(("id_val", id_val.as_str()))
            .await?;
        let task_raw: Option<CognitiveTaskRaw> = response.take(0)?;
        Ok(task_raw.map(CognitiveTask::from))
    }

    pub async fn update_cognitive_task_status(&self, id: &str, status: TaskStatus, result: Option<String>) -> Result<()> {
        let rec_id = if id.contains(':') {
            id.to_string()
        } else {
            format!("cognitive_task:{}", id)
        };
        let id_val = rec_id.splitn(2, ':').collect::<Vec<&str>>()[1].to_string();
        
        let query_str = if status == TaskStatus::Injected {
            "UPDATE type::record('cognitive_task', $id_val) SET status = $status, injected_at = time::now();"
        } else {
            "UPDATE type::record('cognitive_task', $id_val) SET status = $status, result = $result;"
        };

        self.db.query(query_str)
            .bind(("id_val", id_val.as_str()))
            .bind(("status", status.to_string()))
            .bind(("result", result.as_deref()))
            .await?
            .check()?;
        Ok(())
    }

    pub async fn get_pending_cognitive_tasks(&self) -> Result<Vec<CognitiveTask>> {
        let query_str = "SELECT * FROM cognitive_task WHERE status = 'Pending' ORDER BY created_at ASC;";
        let mut response = self.db.query(query_str).await?;
        let tasks: Vec<CognitiveTaskRaw> = response.take(0)?;
        Ok(tasks.into_iter().map(CognitiveTask::from).collect())
    }

    pub async fn get_injected_tasks_older_than_ttl(&self) -> Result<Vec<CognitiveTask>> {
        let query_str = "SELECT * FROM cognitive_task WHERE status = 'Injected';";
        let mut response = self.db.query(query_str).await?;
        let tasks: Vec<CognitiveTaskRaw> = response.take(0)?;
        
        let now = Utc::now();
        let expired_tasks = tasks.into_iter()
            .map(CognitiveTask::from)
            .filter(|t| {
                if let Some(injected) = t.injected_at {
                    injected + chrono::Duration::minutes(t.ttl_minutes) < now
                } else {
                    false
                }
            })
            .collect();
        
        Ok(expired_tasks)
    }

    pub async fn save_pipeline_state(&self, callback_id: &str, state_json: &str) -> Result<()> {
        let query_str = "
            UPSERT type::record('pipeline_state', $callback_id) CONTENT {
                state_json: $state_json,
                created_at: time::now()
            };
        ";
        self.db.query(query_str)
            .bind(("callback_id", callback_id))
            .bind(("state_json", state_json))
            .await?
            .check()?;
        Ok(())
    }

    pub async fn get_pipeline_state(&self, callback_id: &str) -> Result<Option<String>> {
        let query_str = "SELECT VALUE state_json FROM type::record('pipeline_state', $callback_id) LIMIT 1;";
        let mut response = self.db.query(query_str)
            .bind(("callback_id", callback_id))
            .await?;
        let state_opt: Option<String> = response.take(0)?;
        Ok(state_opt)
    }

    pub async fn delete_pipeline_state(&self, callback_id: &str) -> Result<()> {
        let query_str = "DELETE type::record('pipeline_state', $callback_id);";
        self.db.query(query_str)
            .bind(("callback_id", callback_id))
            .await?
            .check()?;
        Ok(())
    }
}
