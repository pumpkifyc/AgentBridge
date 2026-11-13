pub mod create_thread;
pub mod history;
pub mod prompt;
pub mod store;

use serde::{Deserialize, Serialize};

pub use create_thread::ImportService;
pub use prompt::build_continuation_prompt;
pub use store::{ImportStore, StoreError};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImportRecord {
    pub id: String,
    pub source_title: String,
    #[serde(default)]
    pub session_name: String,
    pub local_thread_id: String,
    pub project_path: String,
    pub imported_at: i64,
    pub snapshot_path: String,
    pub status: String,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(skip_serializing)]
    #[serde(default)]
    pub continuation_prompt: Option<String>,
    #[serde(default)]
    pub retry_count: i64,
    #[serde(default)]
    pub last_retried_at: Option<i64>,
}

#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    #[error("项目目录无效：{0}")]
    InvalidProject(String),
    #[error("会话名称无效：{0}")]
    InvalidName(String),
    #[error("续接提示超过大小限制")]
    PromptTooLarge,
    #[error("快照无效：{0}")]
    Snapshot(String),
    #[error("原生会话包无效：{0}")]
    Native(String),
    #[error("本机 Codex 导入失败：{0}")]
    Codex(String),
    #[error("本地导入记录失败：{0}")]
    Store(String),
}
