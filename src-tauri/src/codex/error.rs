use thiserror::Error;

#[derive(Debug, Error)]
pub enum CodexError {
    #[error("找不到 Codex CLI：{0}")]
    NotInstalled(String),
    #[error("无法启动 Codex App Server：{0}")]
    Spawn(String),
    #[error("Codex App Server 已退出")]
    ProcessExited,
    #[error("Codex App Server 响应超时")]
    Timeout,
    #[error("Codex App Server 协议错误：{0}")]
    Protocol(String),
    #[error("Codex App Server 请求失败（{code}）：{message}")]
    Rpc { code: i64, message: String },
    #[error("找不到会话：{0}")]
    ThreadNotFound(String),
    #[error("创建续接会话失败：{0}")]
    TurnFailed(String),
    #[error("快照读取失败：{0}")]
    Snapshot(#[from] crate::snapshot::validate::SnapshotError),
}
