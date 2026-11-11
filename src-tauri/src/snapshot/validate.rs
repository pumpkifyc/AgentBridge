use super::schema::SnapshotV1;
use thiserror::Error;

pub const MAX_SNAPSHOT_BYTES: usize = 20 * 1024 * 1024;
pub const MAX_MESSAGES: usize = 10_000;
pub const MAX_FIELD_BYTES: usize = 1024 * 1024;

#[derive(Debug, Error)]
pub enum SnapshotError {
    #[error("快照超过 20 MiB 限制")]
    TooLarge,
    #[error("快照格式错误：{0}")]
    Malformed(String),
    #[error("不支持的快照版本：{0}")]
    UnsupportedVersion(u64),
    #[error("快照消息数量超过限制")]
    TooManyMessages,
    #[error("快照文本字段超过 1 MiB 限制")]
    FieldTooLarge,
    #[error("快照消息顺序无效")]
    InvalidOrder,
    #[error("指定的截止轮次不存在或尚未完成")]
    InvalidCutoff,
}

pub fn validate_snapshot(bytes: &[u8]) -> Result<SnapshotV1, SnapshotError> {
    if bytes.len() > MAX_SNAPSHOT_BYTES {
        return Err(SnapshotError::TooLarge);
    }

    let value: serde_json::Value = serde_json::from_slice(bytes)
        .map_err(|error| SnapshotError::Malformed(error.to_string()))?;
    let version = value
        .get("schema_version")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| SnapshotError::Malformed("缺少 schema_version".into()))?;
    if version != 1 {
        return Err(SnapshotError::UnsupportedVersion(version));
    }

    let snapshot: SnapshotV1 = serde_json::from_value(value)
        .map_err(|error| SnapshotError::Malformed(error.to_string()))?;
    if snapshot.messages.len() > MAX_MESSAGES {
        return Err(SnapshotError::TooManyMessages);
    }

    let fields = [
        snapshot.metadata.source_thread_id.as_str(),
        snapshot.metadata.title.as_str(),
        snapshot
            .metadata
            .cutoff_turn_id
            .as_deref()
            .unwrap_or_default(),
        snapshot
            .metadata
            .project_name
            .as_deref()
            .unwrap_or_default(),
        snapshot
            .metadata
            .repository_id
            .as_deref()
            .unwrap_or_default(),
        snapshot
            .metadata
            .source_working_directory
            .as_deref()
            .unwrap_or_default(),
    ];
    if fields.iter().any(|field| field.len() > MAX_FIELD_BYTES)
        || snapshot.messages.iter().any(|message| {
            message.text.len() > MAX_FIELD_BYTES
                || message
                    .historical_tool_summary
                    .as_ref()
                    .is_some_and(|field| field.len() > MAX_FIELD_BYTES)
        })
        || snapshot
            .warnings
            .iter()
            .any(|warning| warning.len() > MAX_FIELD_BYTES)
    {
        return Err(SnapshotError::FieldTooLarge);
    }

    let mut last_turn = None;
    let mut last_timestamp = None;
    for message in &snapshot.messages {
        if last_turn.is_some_and(|turn| message.turn_index < turn)
            || message
                .timestamp
                .is_some_and(|time| last_timestamp.is_some_and(|last| time < last))
        {
            return Err(SnapshotError::InvalidOrder);
        }
        last_turn = Some(message.turn_index);
        if let Some(timestamp) = message.timestamp {
            last_timestamp = Some(timestamp);
        }
    }

    Ok(snapshot)
}
