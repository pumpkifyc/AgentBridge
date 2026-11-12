use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MessageRole {
    User,
    Assistant,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotMetadata {
    pub source_thread_id: String,
    pub title: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub cutoff_turn_id: Option<String>,
    pub project_name: Option<String>,
    pub repository_id: Option<String>,
    pub source_working_directory: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotMessage {
    pub role: MessageRole,
    pub text: String,
    pub turn_index: usize,
    pub timestamp: Option<i64>,
    pub historical_tool_summary: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotV1 {
    pub schema_version: u32,
    pub metadata: SnapshotMetadata,
    pub messages: Vec<SnapshotMessage>,
    pub warnings: Vec<String>,
}
