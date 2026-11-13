use super::super::snapshot::{
    convert::convert_codex_items,
    schema::{MessageRole, SnapshotMessage, SnapshotMetadata, SnapshotV1},
    validate::{SnapshotError, validate_snapshot},
};
use crate::codex::models::{RecordedItem, ThreadData, ThreadTurn};
use serde_json::{Value, json};

fn recorded(kind: &str, id: &str, timestamp: Option<i64>, raw: Value) -> RecordedItem {
    RecordedItem {
        id: Some(id.into()),
        kind: kind.into(),
        timestamp,
        raw,
    }
}

fn thread() -> ThreadData {
    ThreadData {
        thread_id: "thread-source-1".into(),
        title: "Fix login flow".into(),
        created_at: 100,
        updated_at: 300,
        project_name: Some("agentbridge".into()),
        repository_id: Some("repo-1".into()),
        working_directory: Some("/Users/alice/project".into()),
        turns: vec![
            ThreadTurn {
                id: "turn-1".into(),
                completed: true,
                truncated: false,
                items: vec![
                    recorded(
                        "userMessage",
                        "user-1",
                        Some(110),
                        json!({
                            "type": "userMessage",
                            "id": "user-1",
                            "content": [{"type": "text", "text": "Fix login"}]
                        }),
                    ),
                    recorded(
                        "agentMessage",
                        "agent-1",
                        Some(120),
                        json!({
                            "type": "agentMessage",
                            "id": "agent-1",
                            "text": "I will inspect the flow."
                        }),
                    ),
                    recorded(
                        "mcpToolCall",
                        "call-1",
                        Some(130),
                        json!({
                            "type": "mcpToolCall",
                            "id": "call-1",
                            "server": "shell",
                            "tool": "exec",
                            "arguments": {"command": "rm -rf /"},
                            "result": "found file"
                        }),
                    ),
                    recorded(
                        "futureItem",
                        "future-1",
                        Some(140),
                        json!({"type": "futureItem", "id": "future-1"}),
                    ),
                    recorded(
                        "userMessage",
                        "images-1",
                        Some(150),
                        json!({
                            "type": "userMessage",
                            "id": "images-1",
                            "content": [{"type": "localImage", "path": "/tmp/image.png"}]
                        }),
                    ),
                ],
            },
            ThreadTurn {
                id: "turn-2".into(),
                completed: true,
                truncated: false,
                items: vec![
                    recorded(
                        "userMessage",
                        "user-2",
                        Some(210),
                        json!({
                            "type": "userMessage",
                            "id": "user-2",
                            "content": [{"type": "text", "text": "Add a test"}]
                        }),
                    ),
                    recorded(
                        "agentMessage",
                        "agent-2",
                        Some(220),
                        json!({
                            "type": "agentMessage",
                            "id": "agent-2",
                            "text": "Added the regression test."
                        }),
                    ),
                ],
            },
            ThreadTurn {
                id: "turn-3".into(),
                completed: false,
                truncated: false,
                items: vec![recorded(
                    "agentMessage",
                    "agent-3",
                    Some(310),
                    json!({
                        "type": "agentMessage",
                        "id": "agent-3",
                        "text": "Still working"
                    }),
                )],
            },
        ],
    }
}

fn valid_snapshot() -> SnapshotV1 {
    SnapshotV1 {
        schema_version: 1,
        metadata: SnapshotMetadata {
            source_thread_id: "thread-source-1".into(),
            title: "Fix login flow".into(),
            created_at: 100,
            updated_at: 300,
            cutoff_turn_id: Some("turn-2".into()),
            project_name: Some("agentbridge".into()),
            repository_id: Some("repo-1".into()),
            source_working_directory: Some("/Users/alice/project".into()),
        },
        messages: vec![
            SnapshotMessage {
                role: MessageRole::User,
                text: "Fix login".into(),
                turn_index: 0,
                timestamp: Some(110),
                historical_tool_summary: None,
            },
            SnapshotMessage {
                role: MessageRole::Assistant,
                text: "I will inspect the flow.".into(),
                turn_index: 0,
                timestamp: Some(120),
                historical_tool_summary: Some("shell: found file".into()),
            },
        ],
        warnings: vec!["已省略不支持的 未知 Codex 历史条目 内容".into()],
    }
}

#[test]
fn converts_completed_turns_in_order_and_keeps_source_path_as_metadata() {
    let snapshot = convert_codex_items(thread(), Some("turn-2".into())).unwrap();

    assert_eq!(snapshot.messages.len(), 4);
    assert_eq!(snapshot.messages[0].text, "Fix login");
    assert_eq!(snapshot.messages[2].text, "Add a test");
    assert_eq!(
        snapshot.metadata.source_working_directory.as_deref(),
        Some("/Users/alice/project")
    );
    assert!(
        snapshot
            .warnings
            .iter()
            .any(|warning| warning.contains("未知 Codex"))
    );
    let serialized = serde_json::to_string(&snapshot).unwrap();
    assert!(!serialized.contains("rm -rf /"));
    assert!(serialized.contains("shell/exec: found file"));
}

#[test]
fn rejects_cutoff_at_incomplete_turn() {
    let error = convert_codex_items(thread(), Some("turn-3".into())).unwrap_err();
    assert!(matches!(error, SnapshotError::InvalidCutoff));
}

#[test]
fn allows_an_earlier_completed_turn_as_the_cutoff() {
    let snapshot = convert_codex_items(thread(), Some("turn-1".into())).unwrap();

    assert_eq!(snapshot.messages.len(), 2);
    assert_eq!(snapshot.metadata.cutoff_turn_id.as_deref(), Some("turn-1"));
}

#[test]
fn an_uncut_snapshot_excludes_all_turns_when_none_are_complete() {
    let mut data = thread();
    data.turns = vec![ThreadTurn {
        id: "pending".into(),
        completed: false,
        truncated: false,
        items: vec![recorded(
            "agentMessage",
            "agent-pending",
            Some(310),
            json!({
                "type": "agentMessage",
                "id": "agent-pending",
                "text": "Still working"
            }),
        )],
    }];

    let snapshot = convert_codex_items(data, None).unwrap();
    assert!(snapshot.messages.is_empty());
    assert_eq!(snapshot.metadata.cutoff_turn_id, None);
}

#[test]
fn redacts_common_credentials_from_messages_and_tool_summaries() {
    let mut data = thread();
    data.turns.truncate(1);
    data.turns[0].items = vec![
        recorded(
            "userMessage",
            "secret-user",
            Some(110),
            json!({
                "type": "userMessage",
                "id": "secret-user",
                "content": [{
                    "type": "text",
                    "text": "key=sk-proj-12345678901234567890 token=ghp_12345678901234567890 password=hunter2"
                }]
            }),
        ),
        recorded(
            "mcpToolCall",
            "secret-tool",
            Some(120),
            json!({
                "type": "mcpToolCall",
                "id": "secret-tool",
                "server": "shell",
                "tool": "exec",
                "result": "Authorization: Bearer eyJhbGciOiJIUzI1NiJ9.secret.signature"
            }),
        ),
    ];

    let snapshot = convert_codex_items(data, None).unwrap();
    let serialized = serde_json::to_string(&snapshot).unwrap();
    for secret in ["sk-proj-123", "ghp_123", "hunter2", "eyJhbGciOi"] {
        assert!(!serialized.contains(secret), "secret leaked: {secret}");
    }
    assert!(
        snapshot
            .warnings
            .iter()
            .any(|warning| warning.contains("凭据"))
    );
}

#[test]
fn aggregates_repeated_unsupported_item_warnings() {
    let mut data = thread();
    data.turns.truncate(1);
    data.turns[0].items = (0..3)
        .map(|index| {
            recorded(
                "futureItem",
                &format!("future-{index}"),
                Some(100 + index),
                json!({"type": "futureItem", "id": format!("future-{index}")}),
            )
        })
        .chain((0..2).map(|index| {
            recorded(
                "userMessage",
                &format!("image-{index}"),
                Some(200 + index),
                json!({
                    "type": "userMessage",
                    "id": format!("image-{index}"),
                    "content": [{"type": "localImage", "path": format!("/tmp/{index}.png")}]
                }),
            )
        }))
        .collect();

    let snapshot = convert_codex_items(data, None).unwrap();

    assert_eq!(
        snapshot.warnings,
        vec![
            "预览中 未知 Codex 历史条目 已折叠（共 3 项）；原生分享包仍完整保留",
            "预览中 图片附件 已折叠（共 2 项）；原生分享包仍完整保留",
        ]
    );
}

#[test]
fn validates_and_round_trips_a_valid_snapshot() {
    let bytes = serde_json::to_vec(&valid_snapshot()).unwrap();
    let parsed = validate_snapshot(&bytes).unwrap();
    assert_eq!(parsed.schema_version, 1);
    assert_eq!(parsed.messages.len(), 2);
}

#[test]
fn rejects_unknown_version_and_malformed_json() {
    let unknown = br#"{"schemaVersion":2,"metadata":{},"messages":[],"warnings":[]}"#;
    assert!(matches!(
        validate_snapshot(unknown),
        Err(SnapshotError::UnsupportedVersion(2))
    ));
    assert!(matches!(
        validate_snapshot(b"{"),
        Err(SnapshotError::Malformed(_))
    ));
}

#[test]
fn rejects_snapshots_over_twenty_mebibytes() {
    let bytes = vec![b' '; 20 * 1024 * 1024 + 1];
    assert!(matches!(
        validate_snapshot(&bytes),
        Err(SnapshotError::TooLarge)
    ));
}

#[test]
fn rejects_too_many_messages_and_oversized_fields() {
    let mut snapshot = valid_snapshot();
    snapshot.messages = (0..10_001)
        .map(|index| SnapshotMessage {
            role: MessageRole::User,
            text: "x".into(),
            turn_index: index,
            timestamp: Some(index as i64),
            historical_tool_summary: None,
        })
        .collect();
    assert!(matches!(
        validate_snapshot(&serde_json::to_vec(&snapshot).unwrap()),
        Err(SnapshotError::TooManyMessages)
    ));

    let mut snapshot = valid_snapshot();
    snapshot.messages[0].text = "x".repeat(1024 * 1024 + 1);
    assert!(matches!(
        validate_snapshot(&serde_json::to_vec(&snapshot).unwrap()),
        Err(SnapshotError::FieldTooLarge)
    ));
}

#[test]
fn rejects_messages_out_of_order() {
    let mut snapshot = valid_snapshot();
    snapshot.messages[1].turn_index = 2;
    snapshot.messages[1].timestamp = Some(90);
    assert!(matches!(
        validate_snapshot(&serde_json::to_vec(&snapshot).unwrap()),
        Err(SnapshotError::InvalidOrder)
    ));
}
