use super::schema::{
    PreviewBlock, PreviewBlockKind, PreviewItem, PreviewItemKind, PreviewNotice,
    PreviewNoticeScope, PreviewTimelineV1, PreviewTurn,
};
use crate::codex::models::{RecordedItem, ThreadData, ThreadTurn};
use serde_json::{Value, json};

fn sample_timeline() -> PreviewTimelineV1 {
    PreviewTimelineV1 {
        schema_version: 1,
        source_thread_id: "thread-1".into(),
        cutoff_turn_id: Some("turn-1".into()),
        turns: vec![PreviewTurn {
            id: "turn-1".into(),
            index: 0,
            status: "completed".into(),
            started_at: Some(100),
            completed_at: Some(200),
            items: vec![PreviewItem {
                id: "user-1".into(),
                kind: PreviewItemKind::UserMessage,
                title: "用户".into(),
                status: Some("completed".into()),
                timestamp: Some(100),
                sensitive: false,
                blocks: vec![PreviewBlock {
                    kind: PreviewBlockKind::Text,
                    label: "消息".into(),
                    content: "修复登录流程".into(),
                    sensitive: false,
                    default_expanded: true,
                }],
            }],
        }],
        notices: vec![PreviewNotice {
            code: "unknown_items".into(),
            count: 0,
            message: "未知条目只影响预览，原生分享包仍完整保留。".into(),
            scope: PreviewNoticeScope::DisplayOnly,
        }],
    }
}

#[test]
fn serializes_preview_timeline_v1_with_ordered_turns_and_display_only_notice() {
    let value = serde_json::to_value(sample_timeline()).expect("预览时间线应可序列化");

    assert_eq!(value["schemaVersion"], 1);
    assert_eq!(value["turns"][0]["items"][0]["kind"], "user_message");
    assert_eq!(value["turns"][0]["items"][0]["blocks"][0]["kind"], "text");
    assert_eq!(value["notices"][0]["scope"], "display_only");
}

fn recorded(kind: &str, id: &str, timestamp: i64, raw: Value) -> RecordedItem {
    RecordedItem {
        id: Some(id.into()),
        kind: kind.into(),
        timestamp: Some(timestamp),
        raw,
    }
}

fn full_thread() -> ThreadData {
    ThreadData {
        thread_id: "thread-preview".into(),
        title: "完整预览".into(),
        created_at: 100,
        updated_at: 500,
        project_name: Some("agentbridge".into()),
        repository_id: None,
        working_directory: Some("/tmp/project".into()),
        turns: vec![
            ThreadTurn {
                id: "turn-1".into(),
                completed: true,
                truncated: false,
                items: vec![
                    recorded(
                        "userMessage",
                        "user-1",
                        110,
                        json!({
                            "type": "userMessage",
                            "id": "user-1",
                            "content": [{"type": "text", "text": "请检查完整预览"}]
                        }),
                    ),
                    recorded(
                        "reasoning",
                        "reasoning-1",
                        120,
                        json!({
                            "type": "reasoning",
                            "id": "reasoning-1",
                            "summary": ["检查历史结构"],
                            "content": []
                        }),
                    ),
                    recorded(
                        "commandExecution",
                        "command-1",
                        130,
                        json!({
                            "type": "commandExecution",
                            "id": "command-1",
                            "command": "npm test",
                            "cwd": "/tmp/project",
                            "status": "completed",
                            "aggregatedOutput": "全部通过",
                            "exitCode": 0,
                            "durationMs": 1200
                        }),
                    ),
                    recorded(
                        "fileChange",
                        "file-1",
                        140,
                        json!({
                            "type": "fileChange",
                            "id": "file-1",
                            "status": "completed",
                            "changes": [{
                                "path": "src/App.tsx",
                                "kind": "update",
                                "diff": "diff --git a/src/App.tsx b/src/App.tsx\n+preview"
                            }]
                        }),
                    ),
                    recorded(
                        "mcpToolCall",
                        "tool-1",
                        150,
                        json!({
                            "type": "mcpToolCall",
                            "id": "tool-1",
                            "server": "files",
                            "tool": "read",
                            "status": "completed",
                            "arguments": {"path": "README.md"},
                            "result": {"content": [{"type": "text", "text": "已读取"}]},
                            "error": null
                        }),
                    ),
                    recorded(
                        "webSearch",
                        "search-1",
                        160,
                        json!({
                            "type": "webSearch",
                            "id": "search-1",
                            "query": "Codex App Server",
                            "action": null,
                            "results": [{"title": "文档"}]
                        }),
                    ),
                    recorded(
                        "imageView",
                        "image-1",
                        170,
                        json!({
                            "type": "imageView",
                            "id": "image-1",
                            "path": "/tmp/project/preview.png"
                        }),
                    ),
                    recorded(
                        "futureItem",
                        "future-1",
                        180,
                        json!({
                            "type": "futureItem",
                            "id": "future-1",
                            "payloadValue": 42
                        }),
                    ),
                ],
            },
            ThreadTurn {
                id: "turn-2".into(),
                completed: true,
                truncated: false,
                items: vec![recorded(
                    "agentMessage",
                    "agent-2",
                    220,
                    json!({
                        "type": "agentMessage",
                        "id": "agent-2",
                        "text": "完整预览已完成"
                    }),
                )],
            },
            ThreadTurn {
                id: "turn-3".into(),
                completed: false,
                truncated: false,
                items: vec![recorded(
                    "agentMessage",
                    "agent-3",
                    320,
                    json!({
                        "type": "agentMessage",
                        "id": "agent-3",
                        "text": "仍在处理"
                    }),
                )],
            },
        ],
    }
}

#[test]
fn normalizes_every_item_kind_and_keeps_original_order() {
    let timeline =
        super::convert::convert_preview_timeline(&full_thread(), Some("turn-2")).unwrap();

    assert_eq!(timeline.turns.len(), 2);
    assert_eq!(
        timeline.turns[0]
            .items
            .iter()
            .map(|item| item.kind)
            .collect::<Vec<_>>(),
        [
            PreviewItemKind::UserMessage,
            PreviewItemKind::Reasoning,
            PreviewItemKind::Command,
            PreviewItemKind::FileChange,
            PreviewItemKind::ToolCall,
            PreviewItemKind::WebSearch,
            PreviewItemKind::Image,
            PreviewItemKind::Unknown,
        ]
    );
    assert!(timeline.turns[0].items[2].blocks[0].default_expanded);
    assert!(!timeline.turns[0].items[2].blocks[1].default_expanded);
}

#[test]
fn unknown_item_falls_back_to_raw_json() {
    let timeline = super::convert::convert_preview_timeline(&full_thread(), Some("turn-1"))
        .expect("未知条目不应让预览失败");
    let unknown = timeline.turns[0].items.last().expect("应保留未知条目");

    assert_eq!(unknown.kind, PreviewItemKind::Unknown);
    assert_eq!(unknown.blocks[0].kind, PreviewBlockKind::Json);
    assert!(unknown.blocks[0].content.contains("\"payloadValue\": 42"));
    assert!(!unknown.blocks[0].default_expanded);
}

#[test]
fn command_without_output_keeps_command_status_and_duration() {
    let mut thread = full_thread();
    thread.turns.truncate(1);
    thread.turns[0].items = vec![recorded(
        "commandExecution",
        "command-empty",
        130,
        json!({
            "type": "commandExecution",
            "id": "command-empty",
            "command": "false",
            "cwd": "/tmp/project",
            "status": "failed",
            "aggregatedOutput": null,
            "exitCode": 1,
            "durationMs": 37
        }),
    )];

    let timeline = super::convert::convert_preview_timeline(&thread, None).unwrap();
    let command = &timeline.turns[0].items[0];

    assert!(command.title.contains("failed"));
    assert!(command.title.contains("37 ms"));
    assert_eq!(command.blocks[0].content, "false");
    assert_eq!(command.blocks[1].content, "无可读输出");
    assert!(!command.blocks[1].default_expanded);
}

#[test]
fn sensitive_block_is_marked_without_removing_original_text() {
    let mut thread = full_thread();
    thread.turns.truncate(1);
    thread.turns[0].items = vec![recorded(
        "userMessage",
        "secret-user",
        110,
        json!({
            "type": "userMessage",
            "id": "secret-user",
            "content": [{
                "type": "text",
                "text": "token=sk-test-example-12345678"
            }]
        }),
    )];

    let timeline = super::convert::convert_preview_timeline(&thread, None).unwrap();
    let block = &timeline.turns[0].items[0].blocks[0];

    assert!(timeline.turns[0].items[0].sensitive);
    assert!(block.sensitive);
    assert!(!block.default_expanded);
    assert_eq!(block.content, "token=sk-test-example-12345678");
    assert_eq!(timeline.notices[0].code, "sensitive_blocks");
    assert_eq!(timeline.notices[0].scope, PreviewNoticeScope::DisplayOnly);
}
