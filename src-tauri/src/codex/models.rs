use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ThreadSummary {
    pub id: String,
    pub title: Option<String>,
    pub preview: String,
    pub cwd: String,
    pub created_at: i64,
    pub updated_at: i64,
    #[serde(default)]
    pub repository_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Page<T> {
    pub data: Vec<T>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TurnSummary {
    pub id: String,
    pub completed: bool,
    pub started_at: Option<i64>,
    pub completed_at: Option<i64>,
    #[serde(default)]
    pub items: Vec<crate::snapshot::convert::CodexItem>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct RecordedItem {
    pub id: Option<String>,
    pub kind: String,
    pub timestamp: Option<i64>,
    pub raw: Value,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ThreadTurn {
    pub id: String,
    pub completed: bool,
    #[serde(default)]
    pub truncated: bool,
    pub items: Vec<RecordedItem>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ThreadData {
    pub thread_id: String,
    pub title: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub project_name: Option<String>,
    pub repository_id: Option<String>,
    pub working_directory: Option<String>,
    pub turns: Vec<ThreadTurn>,
}

#[derive(Debug, Clone)]
pub struct RecordedThread {
    pub data: ThreadData,
    pub rollout_path: Option<String>,
}
