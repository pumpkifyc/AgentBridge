use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PreviewItemKind {
    UserMessage,
    AssistantMessage,
    Plan,
    Reasoning,
    Command,
    FileChange,
    ToolCall,
    WebSearch,
    Image,
    Unknown,
    Error,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PreviewBlockKind {
    Text,
    Code,
    Diff,
    Json,
    Image,
    Error,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PreviewNoticeScope {
    DisplayOnly,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PreviewBlock {
    pub kind: PreviewBlockKind,
    pub label: String,
    pub content: String,
    pub sensitive: bool,
    pub default_expanded: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PreviewItem {
    pub id: String,
    pub kind: PreviewItemKind,
    pub title: String,
    pub status: Option<String>,
    pub timestamp: Option<i64>,
    pub sensitive: bool,
    pub blocks: Vec<PreviewBlock>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PreviewTurn {
    pub id: String,
    pub index: usize,
    pub status: String,
    pub started_at: Option<i64>,
    pub completed_at: Option<i64>,
    pub items: Vec<PreviewItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PreviewNotice {
    pub code: String,
    pub count: usize,
    pub message: String,
    pub scope: PreviewNoticeScope,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PreviewTimelineV1 {
    pub schema_version: u32,
    pub source_thread_id: String,
    pub cutoff_turn_id: Option<String>,
    pub turns: Vec<PreviewTurn>,
    pub notices: Vec<PreviewNotice>,
}
