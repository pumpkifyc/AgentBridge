use super::super::snapshot::{
    convert::{CodexItem, ThreadData, ThreadTurn, convert_codex_items},
    schema::{MessageRole, SnapshotMessage, SnapshotMetadata, SnapshotV1},
    validate::{SnapshotError, validate_snapshot},
};

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
                    CodexItem::UserText {
                        text: "Fix login".into(),
                        timestamp: Some(110),
                    },
                    CodexItem::AssistantText {
                        text: "I will inspect the flow.".into(),
                        timestamp: Some(120),
                    },
                    CodexItem::ToolCall {
                        name: "shell".into(),
                        arguments: "rm -rf /".into(),
                        timestamp: Some(130),
                    },
                    CodexItem::ToolResult {
                        name: "shell".into(),
                        text: "found file".into(),
                        timestamp: Some(140),
                    },
                    CodexItem::Binary {
                        mime_type: "image/png".into(),
                    },
                    CodexItem::Unsupported {
                        kind: "image".into(),
                    },
                ],
            },
            ThreadTurn {
                id: "turn-2".into(),
                completed: true,
                truncated: false,
                items: vec![
                    CodexItem::UserText {
                        text: "Add a test".into(),
                        timestamp: Some(210),
                    },
                    CodexItem::AssistantText {
                        text: "Added the regression test.".into(),
                        timestamp: Some(220),
                    },
                ],
            },
            ThreadTurn {
                id: "turn-3".into(),
                completed: false,
                truncated: false,
                items: vec![CodexItem::AssistantText {
                    text: "Still working".into(),
                    timestamp: Some(310),
                }],
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
        warnings: vec!["已省略不支持的 image 内容".into()],
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
            .any(|warning| warning.contains("image"))
    );
    let serialized = serde_json::to_string(&snapshot).unwrap();
    assert!(!serialized.contains("rm -rf /"));
    assert!(serialized.contains("shell: found file"));
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
        items: vec![CodexItem::AssistantText {
            text: "Still working".into(),
            timestamp: Some(310),
        }],
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
        CodexItem::UserText {
            text:
                "key=sk-proj-12345678901234567890 token=ghp_12345678901234567890 password=hunter2"
                    .into(),
            timestamp: Some(110),
        },
        CodexItem::ToolResult {
            name: "shell".into(),
            text: "Authorization: Bearer eyJhbGciOiJIUzI1NiJ9.secret.signature".into(),
            timestamp: Some(120),
        },
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
    data.turns[0].items = vec![
        CodexItem::Unsupported {
            kind: "未知 Codex 历史条目".into(),
        },
        CodexItem::Unsupported {
            kind: "未知 Codex 历史条目".into(),
        },
        CodexItem::Unsupported {
            kind: "未知 Codex 历史条目".into(),
        },
        CodexItem::Binary {
            mime_type: "image/png".into(),
        },
        CodexItem::Binary {
            mime_type: "image/png".into(),
        },
    ];

    let snapshot = convert_codex_items(data, None).unwrap();

    assert_eq!(
        snapshot.warnings,
        vec![
            "已省略不支持的 未知 Codex 历史条目 内容（共 3 项）",
            "已省略二进制附件（image/png）（共 2 项）",
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
