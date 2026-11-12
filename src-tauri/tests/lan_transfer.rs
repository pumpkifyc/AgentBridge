use agentbridge_lib::{
    pairing::{InvitationManager, InvitationState, invitation::short_code},
    snapshot::schema::{MessageRole, SnapshotMessage, SnapshotMetadata, SnapshotV1},
    transfer::{
        TransferClient, TransferClientError, TransferServer,
        integrity::{self, IntegrityError},
        protocol::{MAX_FAILED_AUTHENTICATIONS, ShareMetadata},
    },
};
use std::{
    net::{IpAddr, Ipv4Addr},
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::time::sleep;

fn snapshot() -> SnapshotV1 {
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
        messages: vec![
            SnapshotMessage {
                role: MessageRole::User,
                text: "修复登录".into(),
                turn_index: 0,
                timestamp: Some(110),
                historical_tool_summary: None,
            },
            SnapshotMessage {
                role: MessageRole::Assistant,
                text: "我会检查登录流程。".into(),
                turn_index: 0,
                timestamp: Some(120),
                historical_tool_summary: None,
            },
        ],
        warnings: vec!["已省略图片附件".into()],
    }
}

async fn start_server(
    manager: &Arc<InvitationManager>,
    ttl_seconds: u64,
) -> (
    agentbridge_lib::pairing::Invitation,
    agentbridge_lib::transfer::TransferServerHandle,
) {
    let invitation = manager.create_with_options(
        "snapshot-1",
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        0,
        ttl_seconds,
    );
    let handle = TransferServer::serve(invitation.clone(), snapshot(), manager.clone())
        .await
        .expect("应启动传输服务器");
    let mut updated = invitation.clone();
    updated.endpoint = handle.local_addr.to_string();
    updated.port = handle.local_addr.port();
    (updated, handle)
}

fn status(manager: &InvitationManager, invitation: &agentbridge_lib::pairing::Invitation) -> InvitationState {
    // get() returns the state stored when last updated; active invitations are accessible.
    manager
        .get(&invitation.id)
        .expect("邀请应存在")
        .state
}

#[tokio::test]
async fn invitation_generates_unique_256_bit_secrets_and_certificate_pin() {
    let manager = InvitationManager::new();
    let first = manager.create("snapshot-1");
    let second = manager.create("snapshot-1");
    assert_ne!(first.id, second.id);
    assert_ne!(first.secret, second.secret);
    assert_eq!(
        base64::Engine::decode(
            &base64::engine::general_purpose::URL_SAFE_NO_PAD,
            &first.secret,
        )
        .expect("凭据应为 URL 安全的 Base64")
        .len(),
        32
    );
    assert!(first.fingerprint.starts_with("sha256:"));
    assert_eq!(first.fingerprint.len(), 7 + 64);
    assert_eq!(first.verification_code.len(), 6);
    assert_eq!(first.verification_code, short_code(&first.fingerprint));
    assert_eq!(first.state, InvitationState::Waiting);
    assert_eq!(first.state.display_name(), "等待配对");
    assert!(first.expires_at > 0);
}

#[tokio::test]
async fn preview_returns_only_metadata_and_accept_transfers_validated_snapshot() {
    let manager = Arc::new(InvitationManager::new());
    let (invitation, handle) = start_server(&manager, 60).await;
    let (client, proposal) = TransferClient::preview(
        &invitation.endpoint,
        &invitation.secret,
        &invitation.fingerprint,
    )
    .await
    .expect("应完成 TLS 指纹验证和配对");

    assert_eq!(proposal.metadata.share_id, invitation.id);
    assert_eq!(proposal.metadata.source_title, "修复登录流程");
    assert_eq!(proposal.metadata.message_count, 2);
    assert_eq!(proposal.metadata.warnings, vec!["已省略图片附件"]);
    assert_eq!(
        proposal.metadata.source_working_directory.as_deref(),
        Some("/Users/alice/project")
    );
    assert_eq!(proposal.total_bytes as usize, proposal.total_bytes as usize);
    assert!(proposal.chunk_count > 0);
    assert_eq!(status(&manager, &invitation), InvitationState::Previewed);

    let received = client.accept().await.expect("应校验摘要并接受快照");
    assert_eq!(received, snapshot());
    assert_eq!(client_invitation_to_string(&proposal.metadata), invitation.id);
    sleep(Duration::from_millis(30)).await;
    assert!(matches!(
        manager.get(&invitation.id),
        Err(agentbridge_lib::pairing::InvitationError::Inactive)
    ));
    handle.stop();
}

fn client_invitation_to_string(metadata: &ShareMetadata) -> String {
    metadata.share_id.clone()
}

#[tokio::test]
async fn wrong_certificate_fingerprint_is_rejected_before_authentication() {
    let manager = Arc::new(InvitationManager::new());
    let (invitation, handle) = start_server(&manager, 60).await;
    let other = manager.create("snapshot-2");
    let error = TransferClient::preview(
        &invitation.endpoint,
        &invitation.secret,
        &other.fingerprint,
    )
    .await
    .expect_err("指纹不匹配时必须拒绝 TLS 连接");
    assert!(matches!(error, TransferClientError::Connection(_) | TransferClientError::Tls(_)));
    assert_eq!(status(&manager, &invitation), InvitationState::Waiting);
    handle.stop();
}

#[tokio::test]
async fn wrong_credentials_are_rejected_and_rate_limited() {
    let manager = Arc::new(InvitationManager::new());
    let (invitation, handle) = start_server(&manager, 60).await;
    for _ in 0..MAX_FAILED_AUTHENTICATIONS {
        let error = TransferClient::preview(
            &invitation.endpoint,
            "wrong-credential",
            &invitation.fingerprint,
        )
        .await
        .expect_err("错误凭据必须被拒绝");
        assert!(matches!(error, TransferClientError::Rejected(_, _)));
    }
    let error = TransferClient::preview(
        &invitation.endpoint,
        "wrong-credential",
        &invitation.fingerprint,
    )
    .await
    .expect_err("多次失败后必须限速");
    assert!(matches!(
        error,
        TransferClientError::Rejected(
            agentbridge_lib::transfer::protocol::ErrorCode::RateLimited,
            _
        )
    ));
    assert_eq!(status(&manager, &invitation), InvitationState::Waiting);
    handle.stop();
}

#[tokio::test]
async fn receiver_can_decline_before_any_content_is_transferred() {
    let manager = Arc::new(InvitationManager::new());
    let (invitation, handle) = start_server(&manager, 60).await;
    let (client, _) = TransferClient::preview(
        &invitation.endpoint,
        &invitation.secret,
        &invitation.fingerprint,
    )
    .await
    .expect("应显示预览");
    client.decline().await.expect("应明确拒绝分享");
    sleep(Duration::from_millis(30)).await;
    assert!(matches!(
        manager.get(&invitation.id),
        Err(agentbridge_lib::pairing::InvitationError::Inactive)
    ));
    handle.stop();
}

#[tokio::test]
async fn expired_and_revoked_invitations_cannot_be_used() {
    let manager = Arc::new(InvitationManager::new());
    let (expired, expired_handle) = start_server(&manager, 0).await;
    sleep(Duration::from_millis(20)).await;
    assert!(matches!(
        manager.get(&expired.id),
        Err(agentbridge_lib::pairing::InvitationError::Expired)
    ));
    let error = TransferClient::preview(
        &expired.endpoint,
        &expired.secret,
        &expired.fingerprint,
    )
    .await
    .expect_err("过期邀请不能被使用");
    assert!(matches!(error, TransferClientError::Rejected(_, _)));
    expired_handle.stop();

    let (invitation, handle) = start_server(&manager, 60).await;
    manager.revoke(&invitation.id).expect("应撤销邀请");
    let error = TransferClient::preview(
        &invitation.endpoint,
        &invitation.secret,
        &invitation.fingerprint,
    )
    .await
    .expect_err("撤销的邀请不能被使用");
    assert!(matches!(error, TransferClientError::Rejected(_, _)));
    handle.stop();
}

#[tokio::test]
async fn one_time_credential_cannot_be_replayed() {
    let manager = Arc::new(InvitationManager::new());
    let (invitation, handle) = start_server(&manager, 60).await;
    let (client, _) = TransferClient::preview(
        &invitation.endpoint,
        &invitation.secret,
        &invitation.fingerprint,
    )
    .await
    .expect("首次配对成功");
    client.accept().await.expect("首次传输成功");
    sleep(Duration::from_millis(30)).await;
    let error = TransferClient::preview(
        &invitation.endpoint,
        &invitation.secret,
        &invitation.fingerprint,
    )
    .await
    .expect_err("相同凭据不能重放");
    assert!(matches!(error, TransferClientError::Rejected(_, _)));
    handle.stop();
}

#[tokio::test]
async fn corrupted_digest_is_detected_and_transfer_is_not_accepted() {
    let bytes = b"snapshot-bytes";
    assert_eq!(integrity::verify(bytes, &integrity::digest(bytes)), Ok(()));
    assert_eq!(
        integrity::verify(bytes, "0000"),
        Err(IntegrityError::DigestMismatch)
    );
    let encoded = integrity::encode_chunk(bytes);
    assert_eq!(integrity::decode_chunk(&encoded).expect("可解码"), bytes);
}

#[tokio::test]
async fn cancelled_or_stopped_server_is_unavailable() {
    let manager = Arc::new(InvitationManager::new());
    let (invitation, handle) = start_server(&manager, 60).await;
    manager.cancel(&invitation.id).expect("应取消邀请");
    let error = TransferClient::preview(
        &invitation.endpoint,
        &invitation.secret,
        &invitation.fingerprint,
    )
    .await
    .expect_err("取消的邀请不能被使用");
    assert!(matches!(error, TransferClientError::Rejected(_, _)));
    handle.stop();
    assert!(handle.is_stopped());
}

#[tokio::test]
async fn interrupted_preview_can_retry_the_same_still_valid_invitation() {
    let manager = Arc::new(InvitationManager::new());
    let (invitation, handle) = start_server(&manager, 60).await;
    let (client, _) = TransferClient::preview(
        &invitation.endpoint,
        &invitation.secret,
        &invitation.fingerprint,
    )
    .await
    .expect("应完成预览");
    drop(client);
    sleep(Duration::from_millis(30)).await;
    assert_ne!(
        manager.status(&invitation.id),
        Ok(InvitationState::Accepted)
    );

    let (retry, _) = TransferClient::preview(
        &invitation.endpoint,
        &invitation.secret,
        &invitation.fingerprint,
    )
    .await
    .expect("预览连接中断后应允许同一邀请重试");
    let received = retry.accept().await.expect("重试应完成校验后的快照传输");
    assert_eq!(received, snapshot());
    sleep(Duration::from_millis(30)).await;
    assert_eq!(
        manager.status(&invitation.id),
        Ok(InvitationState::Accepted)
    );
    assert!(matches!(
        manager.get(&invitation.id),
        Err(agentbridge_lib::pairing::InvitationError::Inactive)
    ));
    handle.stop();
}

#[tokio::test]
async fn a_successful_transfer_consumes_the_invitation_but_a_failed_one_does_not() {
    let manager = Arc::new(InvitationManager::new());
    let (invitation, handle) = start_server(&manager, 60).await;
    handle.stop();
    let error = TransferClient::preview(
        &invitation.endpoint,
        &invitation.secret,
        &invitation.fingerprint,
    )
    .await
    .expect_err("已停止的服务器无法连接");
    assert!(matches!(error, TransferClientError::Connection(_)));
    assert_eq!(
        manager.status(&invitation.id),
        Ok(InvitationState::Waiting)
    );

    let (second, second_handle) = start_server(&manager, 60).await;
    let (client, _) = TransferClient::preview(
        &second.endpoint,
        &second.secret,
        &second.fingerprint,
    )
    .await
    .expect("应完成预览");
    client.accept().await.expect("应完成传输");
    sleep(Duration::from_millis(30)).await;
    let replay = TransferClient::preview(
        &second.endpoint,
        &second.secret,
        &second.fingerprint,
    )
    .await
    .expect_err("成功传输后的凭据不能重放");
    assert!(matches!(replay, TransferClientError::Rejected(_, _)));
    assert_eq!(
        manager.status(&second.id),
        Ok(InvitationState::Accepted)
    );
    second_handle.stop();
}

#[tokio::test]
async fn server_binds_only_to_the_selected_interface() {
    let manager = Arc::new(InvitationManager::new());
    let (invitation, handle) = start_server(&manager, 60).await;
    assert_eq!(handle.local_addr.ip(), IpAddr::V4(Ipv4Addr::LOCALHOST));
    assert_eq!(invitation.bind_ip, "127.0.0.1");
    assert_eq!(handle.local_addr.port(), invitation.endpoint.rsplit_once(':').unwrap().1.parse::<u16>().unwrap());
    handle.stop();
}

#[test]
fn metadata_fields_are_limited_to_share_description() {
    let metadata = ShareMetadata::from_snapshot("share-1", &snapshot());
    let serialized = serde_json::to_string(&metadata).expect("元数据可序列化");
    assert!(serialized.contains("修复登录流程"));
    assert!(!serialized.contains("我会检查登录流程"));
    assert!(!serialized.contains("我会检查登录流程"));
    assert!(SystemTime::now().duration_since(UNIX_EPOCH).is_ok());
}
