use super::{
    schema::{MessageRole, SnapshotMessage, SnapshotMetadata, SnapshotV1},
    validate::SnapshotError,
};
use regex::Regex;
use serde::Deserialize;
use std::sync::LazyLock;

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

#[derive(Debug, Clone, Deserialize)]
pub struct ThreadTurn {
    pub id: String,
    pub completed: bool,
    #[serde(default)]
    pub truncated: bool,
    pub items: Vec<CodexItem>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CodexItem {
    UserText {
        text: String,
        timestamp: Option<i64>,
    },
    AssistantText {
        text: String,
        timestamp: Option<i64>,
    },
    ToolCall {
        name: String,
        arguments: String,
        timestamp: Option<i64>,
    },
    ToolResult {
        name: String,
        text: String,
        timestamp: Option<i64>,
    },
    Binary {
        mime_type: String,
    },
    Unsupported {
        kind: String,
    },
}

static CREDENTIAL_PATTERNS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    [
        r"(?i)\b(?:sk-(?:proj|live|test)-[A-Za-z0-9_-]{8,}|gh[pousr]_[A-Za-z0-9]{8,}|github_pat_[A-Za-z0-9_]{8,})\b",
        r"(?i)\b(?:api[_-]?key|token|password|passwd|secret)\s*[:=]\s*[^\s,;]+",
        r"(?i)\bBearer\s+[A-Za-z0-9._~+/-]+=*",
    ]
    .into_iter()
    .map(|pattern| Regex::new(pattern).expect("credential regex is valid"))
    .collect()
});

fn redact(text: &str) -> (String, bool) {
    let mut redacted = text.to_owned();
    let mut changed = false;
    for pattern in CREDENTIAL_PATTERNS.iter() {
        let next = pattern
            .replace_all(&redacted, "[已隐藏的凭据]")
            .into_owned();
        changed |= next != redacted;
        redacted = next;
    }
    (redacted, changed)
}

pub fn convert_codex_items(
    thread: ThreadData,
    cutoff_turn_id: Option<String>,
) -> Result<SnapshotV1, SnapshotError> {
    let included_turn_count = if let Some(cutoff) = cutoff_turn_id.as_deref() {
        thread
            .turns
            .iter()
            .position(|turn| turn.id == cutoff && turn.completed)
            .ok_or(SnapshotError::InvalidCutoff)?
            + 1
    } else {
        thread
            .turns
            .iter()
            .rposition(|turn| turn.completed)
            .map_or(0, |index| index + 1)
    };

    let mut messages = Vec::new();
    let mut warnings = Vec::new();
    for (turn_index, turn) in thread.turns.iter().take(included_turn_count).enumerate() {
        if turn.truncated {
            warnings.push(format!("第 {} 轮历史内容已截断", turn_index + 1));
        }
        for item in &turn.items {
            match item {
                CodexItem::UserText { text, timestamp } => {
                    let (text, changed) = redact(text);
                    if changed {
                        add_redaction_warning(&mut warnings);
                    }
                    messages.push(SnapshotMessage {
                        role: MessageRole::User,
                        text,
                        turn_index,
                        timestamp: *timestamp,
                        historical_tool_summary: None,
                    });
                }
                CodexItem::AssistantText { text, timestamp } => {
                    let (text, changed) = redact(text);
                    if changed {
                        add_redaction_warning(&mut warnings);
                    }
                    messages.push(SnapshotMessage {
                        role: MessageRole::Assistant,
                        text,
                        turn_index,
                        timestamp: *timestamp,
                        historical_tool_summary: None,
                    });
                }
                CodexItem::ToolResult {
                    name,
                    text,
                    timestamp,
                } => {
                    let (text, changed) = redact(text);
                    if changed {
                        add_redaction_warning(&mut warnings);
                    }
                    let summary = format!(
                        "{}: {}",
                        name.chars().take(120).collect::<String>(),
                        text.chars().take(4000).collect::<String>()
                    );
                    if let Some(previous) =
                        messages
                            .iter_mut()
                            .rev()
                            .find(|message: &&mut SnapshotMessage| {
                                message.role == MessageRole::Assistant
                                    && message.turn_index == turn_index
                            })
                    {
                        previous.historical_tool_summary =
                            Some(match previous.historical_tool_summary.take() {
                                Some(existing) => format!("{existing}\n{summary}"),
                                None => summary,
                            });
                    } else {
                        messages.push(SnapshotMessage {
                            role: MessageRole::Assistant,
                            text: "历史工具结果".into(),
                            turn_index,
                            timestamp: *timestamp,
                            historical_tool_summary: Some(summary),
                        });
                    }
                }
                CodexItem::ToolCall { .. } => {}
                CodexItem::Binary { mime_type } => {
                    warnings.push(format!("已省略二进制附件（{mime_type}）"));
                }
                CodexItem::Unsupported { kind } => {
                    warnings.push(format!("已省略不支持的 {kind} 内容"))
                }
            }
        }
    }

    let (title, title_redacted) = redact(&thread.title);
    if title_redacted {
        add_redaction_warning(&mut warnings);
    }
    let project_name = thread.project_name.map(|name| {
        let (name, redacted) = redact(&name);
        if redacted {
            add_redaction_warning(&mut warnings);
        }
        name
    });

    Ok(SnapshotV1 {
        schema_version: 1,
        metadata: SnapshotMetadata {
            source_thread_id: thread.thread_id,
            title,
            created_at: thread.created_at,
            updated_at: thread.updated_at,
            cutoff_turn_id: cutoff_turn_id.or_else(|| {
                included_turn_count
                    .checked_sub(1)
                    .and_then(|index| thread.turns.get(index))
                    .map(|turn| turn.id.clone())
            }),
            project_name,
            repository_id: thread.repository_id,
            source_working_directory: thread.working_directory,
        },
        messages,
        warnings,
    })
}

fn add_redaction_warning(warnings: &mut Vec<String>) {
    let warning = "检测到疑似凭据，已在快照中隐藏".to_owned();
    if !warnings.contains(&warning) {
        warnings.push(warning);
    }
}
