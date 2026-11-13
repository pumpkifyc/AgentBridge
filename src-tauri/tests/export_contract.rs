use agentbridge_lib::{
    codex::models::Page,
    commands::{PreviewSnapshotRequest, ShareSession, StartShareRequest},
    pairing::{InvitationManager, InvitationState},
    snapshot::{
        schema::{MessageRole, SnapshotMessage, SnapshotMetadata, SnapshotV1},
        validate::validate_snapshot,
    },
};
use serde_json::{Value, json};

fn test_snapshot() -> SnapshotV1 {
    SnapshotV1 {
        schema_version: 1,
        metadata: SnapshotMetadata {
            source_thread_id: "source-thread".into(),
            title: "修复登录流程".into(),
            created_at: 100,
            updated_at: 300,
            cutoff_turn_id: Some("turn-2".into()),
            project_name: Some("agentbridge".into()),
            repository_id: Some("repo-1".into()),
            source_working_directory: Some("/Users/alice/project".into()),
        },
        messages: vec![SnapshotMessage {
            role: MessageRole::User,
            text: "修复登录".into(),
            turn_index: 0,
            timestamp: Some(110),
            historical_tool_summary: None,
        }],
        warnings: vec!["已省略图片附件".into()],
    }
}

#[test]
fn snapshot_wire_format_uses_the_camel_case_fields_expected_by_the_ui() {
    let bytes = serde_json::to_vec(&test_snapshot()).expect("快照可序列化");
    let wire: Value = serde_json::from_slice(&bytes).expect("快照 JSON 可解析");
    assert_eq!(wire["schemaVersion"], 1);
    assert_eq!(wire["metadata"]["sourceThreadId"], "source-thread");
    assert_eq!(wire["metadata"]["createdAt"], 100);
    assert_eq!(wire["metadata"]["updatedAt"], 300);
    assert_eq!(wire["metadata"]["cutoffTurnId"], "turn-2");
    assert_eq!(wire["metadata"]["projectName"], "agentbridge");
    assert_eq!(wire["metadata"]["repositoryId"], "repo-1");
    assert_eq!(
        wire["metadata"]["sourceWorkingDirectory"],
        "/Users/alice/project"
    );
    assert_eq!(wire["messages"][0]["role"], "user");
    assert_eq!(wire["messages"][0]["turnIndex"], 0);
    assert_eq!(wire["messages"][0]["timestamp"], 110);
    assert_eq!(wire["messages"][0]["historicalToolSummary"], Value::Null);
    for legacy in [
        "schema_version",
        "source_thread_id",
        "created_at",
        "updated_at",
        "cutoff_turn_id",
        "project_name",
        "repository_id",
        "source_working_directory",
        "turn_index",
        "historical_tool_summary",
    ] {
        assert!(
            !wire.to_string().contains(legacy),
            "序列化结果仍包含 {legacy}"
        );
    }
    assert_eq!(
        validate_snapshot(&bytes).expect("应接受当前 wire 格式"),
        test_snapshot()
    );
}

#[test]
fn validator_rejects_the_old_underscore_snapshot_version_field() {
    let legacy = json!({
        "schema_version": 1,
        "metadata": {},
        "messages": [],
        "warnings": []
    });
    assert!(validate_snapshot(&serde_json::to_vec(&legacy).unwrap()).is_err());
}

#[test]
fn pagination_wire_format_uses_next_cursor() {
    let page = Page::<u32> {
        data: vec![1],
        next_cursor: Some("page-2".into()),
    };
    let wire = serde_json::to_value(&page).expect("分页结果可序列化");
    assert_eq!(wire["data"], json!([1]));
    assert_eq!(wire["nextCursor"], "page-2");
    assert!(wire.get("next_cursor").is_none());
}

#[test]
fn share_status_reader_reports_terminal_states_without_an_error() {
    let manager = InvitationManager::new();
    let invitation = manager.create("snapshot-1");
    assert_eq!(manager.status(&invitation.id), Ok(InvitationState::Waiting));
    for state in [
        InvitationState::Paired,
        InvitationState::Previewed,
        InvitationState::Transferring,
        InvitationState::Accepted,
        InvitationState::Declined,
        InvitationState::Revoked,
        InvitationState::Cancelled,
        InvitationState::Expired,
        InvitationState::Failed,
    ] {
        manager.set_state(&invitation.id, state);
        assert_eq!(manager.status(&invitation.id), Ok(state));
    }
    assert!(
        manager.get(&invitation.id).is_ok(),
        "失败状态在有效期内仍允许明确重试"
    );
    manager.set_state(&invitation.id, InvitationState::Accepted);
    assert!(
        manager.get(&invitation.id).is_err(),
        "传输完成仍必须拒绝重放"
    );
}

#[test]
fn completed_transfer_is_distinct_from_a_local_import() {
    assert_eq!(InvitationState::Accepted.display_name(), "传输完成");
    assert_eq!(InvitationState::Imported.display_name(), "已导入");
}

#[test]
fn start_share_uses_a_nested_camel_case_request_and_returns_the_tls_fingerprint() {
    let request: StartShareRequest = serde_json::from_value(json!({
        "threadId": "thread-1",
        "previewId": "preview-1",
        "bindIp": "192.168.1.20"
    }))
    .expect("前端 start_share 请求应可解析");
    assert_eq!(request.thread_id, "thread-1");
    assert_eq!(request.preview_id, "preview-1");
    assert_eq!(request.bind_ip, "192.168.1.20");

    let response = ShareSession {
        id: "invitation-1".into(),
        endpoint: "192.168.1.20:49152".into(),
        secret: "secret-value".into(),
        fingerprint: format!("sha256:{}", "a".repeat(64)),
        verification_code: "012345".into(),
        expires_at: 1_800_000_300,
        state: InvitationState::Waiting.display_name().into(),
    };
    let wire = serde_json::to_value(response).expect("邀请响应可序列化");
    assert_eq!(wire["fingerprint"], format!("sha256:{}", "a".repeat(64)));
    assert_eq!(wire["verificationCode"], "012345");
    assert_eq!(wire["expiresAt"], 1_800_000_300);
    assert_eq!(wire["state"], "等待配对");
}

#[test]
fn preview_snapshot_uses_a_nested_camel_case_request_with_a_preview_token() {
    let request: PreviewSnapshotRequest = serde_json::from_value(json!({
        "threadId": "thread-1",
        "cutoffTurnId": "turn-2",
        "previewId": "preview-1"
    }))
    .expect("前端 preview_snapshot 请求应可解析");

    assert_eq!(request.thread_id, "thread-1");
    assert_eq!(request.cutoff_turn_id.as_deref(), Some("turn-2"));
    assert_eq!(request.preview_id, "preview-1");
}
