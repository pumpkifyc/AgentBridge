use serde_json::Value;

use super::{
    schema::{
        PreviewBlock, PreviewBlockKind, PreviewItem, PreviewItemKind, PreviewNotice,
        PreviewNoticeScope, PreviewTimelineV1, PreviewTurn,
    },
    sensitive::mark_block_sensitive,
};
use crate::{
    codex::models::{RecordedItem, ThreadData},
    snapshot::validate::SnapshotError,
};

pub fn convert_preview_timeline(
    thread: &ThreadData,
    cutoff_turn_id: Option<&str>,
) -> Result<PreviewTimelineV1, SnapshotError> {
    let included_turn_count = if let Some(cutoff) = cutoff_turn_id {
        thread
            .turns
            .iter()
            .position(|turn| turn.id == cutoff && turn.completed)
            .ok_or(SnapshotError::InvalidCutoff)?
            + 1
    } else {
        thread.turns.len()
    };

    let mut unknown_items = 0;
    let mut file_changes = 0;
    let mut command_outputs = 0;
    let mut images = 0;
    let mut tool_outputs = 0;
    let mut sensitive_blocks = 0;
    let turns = thread
        .turns
        .iter()
        .take(included_turn_count)
        .enumerate()
        .map(|(turn_index, turn)| {
            let mut items = turn
                .items
                .iter()
                .enumerate()
                .map(|(item_index, item)| {
                    let mut preview =
                        normalize_item(item, turn_index, item_index).unwrap_or_else(|| {
                            unknown_items += 1;
                            unknown_item(item, item_index)
                        });
                    for block in &mut preview.blocks {
                        mark_block_sensitive(block);
                        sensitive_blocks += usize::from(block.sensitive);
                        file_changes += usize::from(block.kind == PreviewBlockKind::Diff);
                        command_outputs += usize::from(
                            preview.kind == PreviewItemKind::Command
                                && block.label == "输出"
                                && !block.sensitive,
                        );
                        images += usize::from(block.kind == PreviewBlockKind::Image);
                        tool_outputs += usize::from(
                            preview.kind == PreviewItemKind::ToolCall
                                && matches!(block.label.as_str(), "工具结果" | "工具错误"),
                        );
                    }
                    preview.sensitive = preview.blocks.iter().any(|block| block.sensitive);
                    preview
                })
                .collect::<Vec<_>>();
            if let Some(error) = turn.error.as_ref().filter(|error| !error.is_null()) {
                items.push(PreviewItem {
                    id: format!("{}-turn-error", turn.id),
                    kind: PreviewItemKind::Error,
                    title: "轮次错误".into(),
                    status: turn.status.clone(),
                    timestamp: turn.completed_at.or(turn.started_at),
                    sensitive: false,
                    blocks: vec![block(
                        PreviewBlockKind::Error,
                        "错误详情",
                        value_text(error),
                        false,
                    )],
                });
            }
            PreviewTurn {
                id: turn.id.clone(),
                index: turn_index,
                status: turn
                    .status
                    .clone()
                    .filter(|status| !status.is_empty())
                    .unwrap_or_else(|| {
                        if turn.completed {
                            "completed".into()
                        } else {
                            "incomplete".into()
                        }
                    }),
                started_at: turn
                    .started_at
                    .or_else(|| turn.items.iter().filter_map(|item| item.timestamp).min()),
                completed_at: turn
                    .completed_at
                    .or_else(|| turn.items.iter().filter_map(|item| item.timestamp).max()),
                items,
            }
        })
        .collect();

    let mut notices = Vec::new();
    add_notice(
        &mut notices,
        "sensitive_blocks",
        sensitive_blocks,
        "预览检测到疑似敏感块，默认隐藏；点击“显示原文”可核对，原生分享包仍完整保留原文。",
    );
    add_notice(
        &mut notices,
        "unknown_items",
        unknown_items,
        "预览中有未知 Codex 条目，默认折叠原始 JSON；原生分享包仍完整保留。",
    );
    add_notice(
        &mut notices,
        "file_changes",
        file_changes,
        "预览默认折叠文件 diff；导入后仍可查看完整原生记录。",
    );
    add_notice(
        &mut notices,
        "command_outputs",
        command_outputs,
        "预览默认折叠命令输出；历史命令不会重新执行，原生分享包仍完整保留。",
    );
    add_notice(
        &mut notices,
        "images",
        images,
        "预览只显示图片路径或引用；原生分享包仍完整保留图片记录。",
    );
    add_notice(
        &mut notices,
        "tool_outputs",
        tool_outputs,
        "预览默认折叠工具参数和结果；历史工具调用不会重新执行。",
    );

    Ok(PreviewTimelineV1 {
        schema_version: 1,
        source_thread_id: thread.thread_id.clone(),
        cutoff_turn_id: cutoff_turn_id.map(str::to_owned),
        turns,
        notices,
    })
}

fn normalize_item(
    item: &RecordedItem,
    turn_index: usize,
    item_index: usize,
) -> Option<PreviewItem> {
    let id = item
        .id
        .clone()
        .unwrap_or_else(|| format!("turn-{turn_index}-item-{item_index}-{}", item.kind));
    let status = string_field(&item.raw, "status");
    let base = || PreviewItem {
        id: id.clone(),
        kind: PreviewItemKind::Unknown,
        title: format!("Codex 条目：{}", item.kind),
        status: status.clone(),
        timestamp: item.timestamp,
        sensitive: false,
        blocks: Vec::new(),
    };

    let mut preview = match item.kind.as_str() {
        "userMessage" => {
            let parts = item.raw.get("content")?.as_array()?;
            let mut blocks = Vec::new();
            for part in parts {
                let part_type = part.get("type")?.as_str()?;
                match part_type {
                    "text" => blocks.push(block(
                        PreviewBlockKind::Text,
                        "消息",
                        part.get("text")?.as_str()?.to_owned(),
                        true,
                    )),
                    "image" | "localImage" => blocks.push(block(
                        PreviewBlockKind::Image,
                        "图片附件",
                        image_reference(part),
                        true,
                    )),
                    _ => blocks.push(block(
                        PreviewBlockKind::Json,
                        format!("输入：{part_type}"),
                        pretty_json(part),
                        false,
                    )),
                }
            }
            PreviewItem {
                kind: PreviewItemKind::UserMessage,
                title: "用户消息".into(),
                blocks,
                ..base()
            }
        }
        "agentMessage" => PreviewItem {
            kind: PreviewItemKind::AssistantMessage,
            title: "助手回复".into(),
            blocks: vec![block(
                PreviewBlockKind::Text,
                "回复",
                string_field(&item.raw, "text")?,
                true,
            )],
            ..base()
        },
        "plan" => PreviewItem {
            kind: PreviewItemKind::Plan,
            title: "计划".into(),
            blocks: vec![block(
                PreviewBlockKind::Text,
                "计划内容",
                string_field(&item.raw, "text")?,
                false,
            )],
            ..base()
        },
        "reasoning" => {
            let mut blocks = Vec::new();
            push_string_array(
                &mut blocks,
                item.raw.get("summary"),
                "思考摘要",
                PreviewBlockKind::Text,
            );
            push_string_array(
                &mut blocks,
                item.raw.get("content"),
                "思考原文",
                PreviewBlockKind::Json,
            );
            if blocks.is_empty() {
                return None;
            }
            PreviewItem {
                kind: PreviewItemKind::Reasoning,
                title: "思考".into(),
                blocks,
                ..base()
            }
        }
        "commandExecution" => {
            let command = string_field(&item.raw, "command")?;
            let status = status.as_deref().unwrap_or("unknown");
            let duration = item
                .raw
                .get("durationMs")
                .and_then(Value::as_i64)
                .map(|millis| format!(" · {millis} ms"))
                .unwrap_or_default();
            let exit_code = item
                .raw
                .get("exitCode")
                .and_then(Value::as_i64)
                .map(|code| format!(" · exit {code}"))
                .unwrap_or_default();
            let output = item
                .raw
                .get("aggregatedOutput")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
                .unwrap_or("无可读输出")
                .to_owned();
            PreviewItem {
                kind: PreviewItemKind::Command,
                title: format!("命令执行 · {status}{exit_code}{duration}"),
                status: Some(status.into()),
                blocks: vec![
                    block(PreviewBlockKind::Code, "命令", command, true),
                    block(PreviewBlockKind::Code, "输出", output, false),
                ],
                ..base()
            }
        }
        "fileChange" => {
            let changes = item.raw.get("changes")?.as_array()?;
            let status = status.as_deref().unwrap_or("unknown");
            let mut paths = Vec::new();
            let mut blocks = Vec::new();
            for change in changes {
                let path = string_field(change, "path")?;
                let diff = string_field(change, "diff")?;
                paths.push(path.clone());
                blocks.push(block(PreviewBlockKind::Diff, path, diff, false));
            }
            if blocks.is_empty() {
                return None;
            }
            PreviewItem {
                kind: PreviewItemKind::FileChange,
                title: format!("文件修改 · {} 个文件 · {status}", blocks.len()),
                blocks,
                ..base()
            }
        }
        "mcpToolCall" => {
            let server = string_field(&item.raw, "server")?;
            let tool = string_field(&item.raw, "tool")?;
            let mut blocks = Vec::new();
            if let Some(arguments) = item.raw.get("arguments")
                && !arguments.is_null()
            {
                blocks.push(block(
                    PreviewBlockKind::Json,
                    "工具参数",
                    pretty_json(arguments),
                    false,
                ));
            }
            if let Some(result) = item.raw.get("result")
                && !result.is_null()
            {
                blocks.push(block(
                    PreviewBlockKind::Json,
                    "工具结果",
                    value_text(result),
                    false,
                ));
            }
            if let Some(error) = item.raw.get("error")
                && !error.is_null()
            {
                blocks.push(block(
                    PreviewBlockKind::Error,
                    "工具错误",
                    value_text(error),
                    false,
                ));
            }
            PreviewItem {
                kind: PreviewItemKind::ToolCall,
                title: format!("{server}/{tool}"),
                blocks,
                ..base()
            }
        }
        "dynamicToolCall" => {
            let tool = string_field(&item.raw, "tool")?;
            let namespace =
                string_field(&item.raw, "namespace").unwrap_or_else(|| "dynamic".into());
            let mut blocks = vec![block(
                PreviewBlockKind::Json,
                "工具参数",
                value_text(item.raw.get("arguments").unwrap_or(&Value::Null)),
                false,
            )];
            if let Some(content) = item.raw.get("contentItems")
                && !content.is_null()
            {
                blocks.push(block(
                    PreviewBlockKind::Json,
                    "工具结果",
                    pretty_json(content),
                    false,
                ));
            }
            PreviewItem {
                kind: PreviewItemKind::ToolCall,
                title: format!("{namespace}/{tool}"),
                blocks,
                ..base()
            }
        }
        "functionCallOutput" => {
            let name = string_field(&item.raw, "name")?;
            PreviewItem {
                kind: PreviewItemKind::ToolCall,
                title: name,
                blocks: vec![block(
                    PreviewBlockKind::Json,
                    "工具结果",
                    value_text(item.raw.get("output").unwrap_or(&Value::Null)),
                    false,
                )],
                ..base()
            }
        }
        "webSearch" => {
            let query = string_field(&item.raw, "query")?;
            let mut blocks = vec![block(PreviewBlockKind::Text, "搜索词", query, true)];
            if let Some(action) = item.raw.get("action")
                && !action.is_null()
            {
                blocks.push(block(
                    PreviewBlockKind::Json,
                    "搜索操作",
                    pretty_json(action),
                    false,
                ));
            }
            if let Some(results) = item.raw.get("results")
                && !results.is_null()
            {
                blocks.push(block(
                    PreviewBlockKind::Json,
                    "搜索结果",
                    pretty_json(results),
                    false,
                ));
            }
            PreviewItem {
                kind: PreviewItemKind::WebSearch,
                title: "Web 搜索".into(),
                blocks,
                ..base()
            }
        }
        "imageView" => PreviewItem {
            kind: PreviewItemKind::Image,
            title: "图片".into(),
            blocks: vec![block(
                PreviewBlockKind::Image,
                "图片路径",
                string_field(&item.raw, "path")?,
                true,
            )],
            ..base()
        },
        "imageGeneration" => PreviewItem {
            kind: PreviewItemKind::Image,
            title: "生成的图片".into(),
            blocks: generated_image_blocks(&item.raw),
            ..base()
        },
        "error" => PreviewItem {
            kind: PreviewItemKind::Error,
            title: "错误".into(),
            blocks: vec![block(
                PreviewBlockKind::Error,
                "错误详情",
                value_text(&item.raw),
                false,
            )],
            ..base()
        },
        _ => return None,
    };

    if preview.blocks.is_empty() {
        return None;
    }
    preview.sensitive = preview.blocks.iter().any(|block| block.sensitive);
    Some(preview)
}

fn unknown_item(item: &RecordedItem, item_index: usize) -> PreviewItem {
    PreviewItem {
        id: item
            .id
            .clone()
            .unwrap_or_else(|| format!("unknown-{item_index}-{}", item.kind)),
        kind: PreviewItemKind::Unknown,
        title: format!("Codex 条目：{}", item.kind),
        status: string_field(&item.raw, "status"),
        timestamp: item.timestamp,
        sensitive: false,
        blocks: vec![block(
            PreviewBlockKind::Json,
            "原始 JSON",
            pretty_json(&item.raw),
            false,
        )],
    }
}

fn block(
    kind: PreviewBlockKind,
    label: impl Into<String>,
    content: String,
    expanded: bool,
) -> PreviewBlock {
    PreviewBlock {
        kind,
        label: label.into(),
        content,
        sensitive: false,
        default_expanded: expanded,
    }
}

fn push_string_array(
    blocks: &mut Vec<PreviewBlock>,
    value: Option<&Value>,
    label: &str,
    kind: PreviewBlockKind,
) {
    if let Some(values) = value.and_then(Value::as_array) {
        for value in values
            .iter()
            .filter_map(Value::as_str)
            .filter(|value| !value.is_empty())
        {
            blocks.push(block(kind, label, value.to_owned(), false));
        }
    }
}

fn string_field(value: &Value, field: &str) -> Option<String> {
    value.get(field).and_then(Value::as_str).map(str::to_owned)
}

fn value_text(value: &Value) -> String {
    value
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| pretty_json(value))
}

fn pretty_json(value: &Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
}

fn image_reference(value: &Value) -> String {
    ["path", "url", "fileId"]
        .into_iter()
        .find_map(|field| value.get(field).and_then(Value::as_str))
        .filter(|reference| !reference.trim_start().starts_with("data:"))
        .map(str::to_owned)
        .unwrap_or_else(|| "预览不展开内联图片数据；打开原生会话可查看。".into())
}

fn generated_image_blocks(value: &Value) -> Vec<PreviewBlock> {
    let mut blocks = Vec::new();
    if let Some(reference) = ["path", "url", "fileId"]
        .into_iter()
        .find_map(|field| value.get(field).and_then(Value::as_str))
        .filter(|reference| !reference.trim_start().starts_with("data:"))
    {
        blocks.push(block(
            PreviewBlockKind::Image,
            "图片引用",
            reference.to_owned(),
            true,
        ));
    } else {
        blocks.push(block(
            PreviewBlockKind::Text,
            "图片内容",
            "预览不展开内联图片数据；打开原生会话可查看。".into(),
            false,
        ));
    }
    if let Some(inputs) = value.get("inputImages").and_then(Value::as_array) {
        let references = inputs
            .iter()
            .map(image_reference)
            .collect::<Vec<_>>()
            .join("\n");
        if !references.is_empty() {
            blocks.push(block(PreviewBlockKind::Text, "输入图片", references, false));
        }
    }
    if let Some(error) = value.get("error").filter(|error| !error.is_null()) {
        blocks.push(block(
            PreviewBlockKind::Error,
            "失败原因",
            value_text(error),
            false,
        ));
    }
    blocks
}

fn add_notice(notices: &mut Vec<PreviewNotice>, code: &str, count: usize, message: &str) {
    if count > 0 {
        notices.push(PreviewNotice {
            code: code.into(),
            count,
            message: message.into(),
            scope: PreviewNoticeScope::DisplayOnly,
        });
    }
}
