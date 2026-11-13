use super::{
    schema::{MessageRole, SnapshotMessage, SnapshotMetadata, SnapshotV1},
    validate::SnapshotError,
};
use crate::codex::models::{RecordedItem, ThreadData};
use regex::Regex;
use serde::Deserialize;
use std::sync::LazyLock;

#[derive(Debug, Clone, Deserialize, serde::Serialize, PartialEq, Eq)]
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
    let mut warnings = WarningCollector::default();
    for (turn_index, turn) in thread.turns.iter().take(included_turn_count).enumerate() {
        if turn.truncated {
            warnings.add(format!(
                "预览中第 {} 轮历史内容不完整；原生分享包保留 Codex 当前可提供的原始记录",
                turn_index + 1
            ));
        }
        for item in &turn.items {
            for converted in convert_recorded_item(item) {
                match converted {
                    CodexItem::UserText { text, timestamp } => {
                        let (text, changed) = redact(&text);
                        if changed {
                            add_redaction_warning(&mut warnings);
                        }
                        messages.push(SnapshotMessage {
                            role: MessageRole::User,
                            text,
                            turn_index,
                            timestamp,
                            historical_tool_summary: None,
                        });
                    }
                    CodexItem::AssistantText { text, timestamp } => {
                        let (text, changed) = redact(&text);
                        if changed {
                            add_redaction_warning(&mut warnings);
                        }
                        messages.push(SnapshotMessage {
                            role: MessageRole::Assistant,
                            text,
                            turn_index,
                            timestamp,
                            historical_tool_summary: None,
                        });
                    }
                    CodexItem::ToolResult {
                        name,
                        text,
                        timestamp,
                    } => {
                        let (text, changed) = redact(&text);
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
                                timestamp,
                                historical_tool_summary: Some(summary),
                            });
                        }
                    }
                    CodexItem::ToolCall { .. } => {}
                    CodexItem::Binary { mime_type } => {
                        warnings.add(format!(
                            "预览中未展开二进制附件（{mime_type}）；原生分享包仍完整保留"
                        ));
                    }
                    CodexItem::Unsupported { kind } => {
                        warnings.add(format!("预览中 {kind} 已折叠；原生分享包仍完整保留"))
                    }
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
        warnings: warnings.finish(),
    })
}

fn convert_recorded_item(item: &RecordedItem) -> Vec<CodexItem> {
    let timestamp = item.timestamp;
    match item.kind.as_str() {
        "userMessage" => {
            let mut text = Vec::new();
            let mut unsupported = Vec::new();
            for part in item
                .raw
                .get("content")
                .and_then(serde_json::Value::as_array)
                .into_iter()
                .flatten()
            {
                match part.get("type").and_then(serde_json::Value::as_str) {
                    Some("text") => {
                        text.push(
                            part.get("text")
                                .and_then(serde_json::Value::as_str)
                                .unwrap_or_default(),
                        );
                    }
                    Some("image" | "localImage") => unsupported.push("图片附件".to_owned()),
                    Some(kind) => unsupported.push(kind.to_owned()),
                    None => unsupported.push("未知输入".to_owned()),
                }
            }
            let mut converted = Vec::new();
            if !text.is_empty() {
                converted.push(CodexItem::UserText {
                    text: text.join("\n"),
                    timestamp,
                });
            }
            converted.extend(
                unsupported
                    .into_iter()
                    .map(|kind| CodexItem::Unsupported { kind }),
            );
            converted
        }
        "agentMessage" | "plan" => item
            .raw
            .get("text")
            .and_then(serde_json::Value::as_str)
            .map(|text| {
                vec![CodexItem::AssistantText {
                    text: text.to_owned(),
                    timestamp,
                }]
            })
            .unwrap_or_else(|| {
                vec![CodexItem::Unsupported {
                    kind: if item.kind == "plan" {
                        "计划"
                    } else {
                        "助手消息"
                    }
                    .into(),
                }]
            }),
        "commandExecution" => vec![
            item.raw
                .get("aggregatedOutput")
                .and_then(serde_json::Value::as_str)
                .filter(|text| !text.is_empty())
                .map_or_else(
                    || CodexItem::Unsupported {
                        kind: "命令执行（无可读输出）".into(),
                    },
                    |text| CodexItem::ToolResult {
                        name: "命令执行".into(),
                        text: text.to_owned(),
                        timestamp,
                    },
                ),
        ],
        "mcpToolCall" => {
            let server = item
                .raw
                .get("server")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("MCP");
            let tool = item
                .raw
                .get("tool")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("未知工具");
            let name = format!("{server}/{tool}");
            vec![
                CodexItem::ToolCall {
                    name: name.clone(),
                    arguments: String::new(),
                    timestamp,
                },
                item.raw
                    .get("result")
                    .filter(|value| !value.is_null())
                    .map_or_else(
                        || CodexItem::Unsupported {
                            kind: format!("{name}（无可读取结果）"),
                        },
                        |value| match value {
                            serde_json::Value::String(text) => CodexItem::ToolResult {
                                name: name.clone(),
                                text: text.clone(),
                                timestamp,
                            },
                            other => CodexItem::ToolResult {
                                name: name.clone(),
                                text: other.to_string(),
                                timestamp,
                            },
                        },
                    ),
            ]
        }
        "fileChange" => vec![CodexItem::Unsupported {
            kind: "文件变更".into(),
        }],
        _ => vec![CodexItem::Unsupported {
            kind: "未知 Codex 历史条目".into(),
        }],
    }
}

#[derive(Default)]
struct WarningCollector {
    entries: Vec<(String, usize)>,
}

impl WarningCollector {
    fn add(&mut self, warning: String) {
        if let Some((_, count)) = self
            .entries
            .iter_mut()
            .find(|(existing, _)| existing == &warning)
        {
            *count += 1;
        } else {
            self.entries.push((warning, 1));
        }
    }

    fn finish(self) -> Vec<String> {
        self.entries
            .into_iter()
            .map(|(warning, count)| {
                if count > 1 {
                    warning.split_once("；原生分享包").map_or_else(
                        || format!("{warning}（共 {count} 项）"),
                        |(preview, native)| {
                            format!("{preview}（共 {count} 项）；原生分享包{native}")
                        },
                    )
                } else {
                    warning
                }
            })
            .collect()
    }
}

fn add_redaction_warning(warnings: &mut WarningCollector) {
    warnings.add("预览中检测到疑似凭据，默认隐藏；原生分享包仍完整保留原文".to_owned());
}
