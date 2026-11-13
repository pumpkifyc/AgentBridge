use agentbridge_lib::{
    commands::{AppState, ReceiveState, StagedSnapshot, validate_received_pairing},
    native,
    pairing::{InvitationManager, InvitationState},
    snapshot::schema::{MessageRole, SnapshotMessage, SnapshotMetadata, SnapshotV1},
    transfer::{TransferServer, protocol::certificate_for},
};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    net::{IpAddr, Ipv4Addr},
    sync::{Arc, Once, OnceLock},
    time::Duration,
};
use tauri::{
    Manager,
    ipc::{CallbackFn, InvokeBody, RuntimeAuthority},
    test::{INVOKE_KEY, mock_builder},
    webview::InvokeRequest,
};
use tauri_utils::acl::ExecutionContext;

fn test_runtime() -> &'static tokio::runtime::Runtime {
    static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    static SET_RUNTIME: Once = Once::new();
    let runtime = RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("测试运行时可创建")
    });
    SET_RUNTIME.call_once(|| tauri::async_runtime::set(runtime.handle().clone()));
    runtime
}

#[derive(Debug)]
struct ResponseReceiver(std::sync::mpsc::Receiver<Result<serde_json::Value, serde_json::Value>>);

impl ResponseReceiver {
    fn get(self) -> serde_json::Value {
        self.0
            .recv_timeout(std::time::Duration::from_secs(20))
            .expect("命令应在超时前响应")
            .expect("命令应成功")
    }
}

#[derive(Serialize, Deserialize)]
struct PairingInfo {
    endpoint: String,
    secret: String,
    fingerprint: String,
    #[serde(rename = "verificationCode")]
    verification_code: String,
    #[serde(rename = "expiresAt")]
    expires_at: i64,
}

#[test]
fn validates_the_fingerprint_derived_verification_code_before_connecting() {
    let manager = InvitationManager::new();
    let invitation = manager.create("snapshot-1");
    let valid = valid_pairing(&invitation);
    validate_received_pairing(&valid).expect("验证码应与证书指纹一致");

    let wrong_code = agentbridge_lib::transfer::protocol::PairingInfo {
        verification_code: "000000".into(),
        ..valid.clone()
    };
    assert!(
        validate_received_pairing(&wrong_code).is_err(),
        "错误验证码必须被拒绝"
    );
    assert!(
        validate_received_pairing(&agentbridge_lib::transfer::protocol::PairingInfo {
            fingerprint: "sha256:not-a-digest".into(),
            verification_code: "012345".into(),
            ..valid.clone()
        })
        .is_err()
    );
    assert!(
        validate_received_pairing(&agentbridge_lib::transfer::protocol::PairingInfo {
            endpoint: String::new(),
            ..valid
        })
        .is_err()
    );
}

fn valid_pairing(
    invitation: &agentbridge_lib::pairing::Invitation,
) -> agentbridge_lib::transfer::protocol::PairingInfo {
    agentbridge_lib::transfer::protocol::PairingInfo {
        endpoint: invitation.endpoint.clone(),
        secret: invitation.secret.clone(),
        fingerprint: invitation.fingerprint.clone(),
        verification_code: invitation.verification_code.clone(),
        expires_at: invitation.expires_at,
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProgressEvent {
    transferred_bytes: u64,
    total_bytes: u64,
}

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

fn native_bundle(snapshot: &SnapshotV1) -> Vec<u8> {
    let root = std::env::temp_dir().join(format!(
        "agentbridge-receive-native-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("系统时间有效")
            .as_nanos()
    ));
    let sessions = root.join("sessions/2026/11/13");
    fs::create_dir_all(&sessions).expect("创建测试会话目录");
    let rollout = sessions.join(format!(
        "rollout-2026-11-13T00-00-00-{}.jsonl",
        snapshot.metadata.source_thread_id
    ));
    fs::write(
        &rollout,
        format!(
            "{{\"type\":\"session_meta\",\"payload\":{{\"id\":\"{}\",\"session_id\":\"{}\"}}}}\n",
            snapshot.metadata.source_thread_id, snapshot.metadata.source_thread_id
        ),
    )
    .expect("写入测试会话");
    let bundle = native::build_bundle(&root, &rollout, snapshot).expect("构建原生会话包");
    let _ = fs::remove_dir_all(root);
    bundle
}

fn invoke(
    app: &tauri::App<tauri::test::MockRuntime>,
    cmd: &'static str,
    sequence: usize,
    body: serde_json::Value,
) -> ResponseReceiver {
    let label = format!("invoke-{}-{sequence}", std::process::id());
    let webview = tauri::WebviewWindowBuilder::new(app, label, Default::default())
        .build()
        .expect("测试窗口可创建");
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    webview.clone().on_message(
        InvokeRequest {
            cmd: cmd.into(),
            callback: CallbackFn(0),
            error: CallbackFn(1),
            url: if cfg!(windows) {
                "http://tauri.localhost".parse().expect("URL 有效")
            } else {
                "tauri://localhost".parse().expect("URL 有效")
            },
            body: InvokeBody::Json(body),
            headers: Default::default(),
            invoke_key: INVOKE_KEY.into(),
        },
        Box::new(move |_, _, response, _, _| {
            let result = match response {
                tauri::ipc::InvokeResponse::Ok(value) => {
                    Ok(value.deserialize().expect("响应应为 JSON"))
                }
                tauri::ipc::InvokeResponse::Err(error) => Err(error.0),
            };
            let _ = tx.send(result);
        }),
    );
    let _ = webview.close();
    ResponseReceiver(rx)
}

fn progress_values(app: &tauri::App<tauri::test::MockRuntime>) -> Vec<ProgressEvent> {
    let state = app.state::<ReceiveState>();
    state
        .progress
        .lock()
        .expect("进度锁可读取")
        .iter()
        .map(|event| serde_json::from_value(event.clone()).expect("进度事件可解析"))
        .collect()
}

#[test]
fn wrong_verification_code_cannot_reach_the_transfer_server() {
    let runtime = test_runtime();
    let _guard = runtime.enter();
    let manager = Arc::new(InvitationManager::new());
    let invitation =
        manager.create_with_options("snapshot-1", IpAddr::V4(Ipv4Addr::LOCALHOST), 0, 60);
    let current_snapshot = snapshot();
    let handle = runtime
        .block_on(TransferServer::serve(
            invitation.clone(),
            current_snapshot.clone(),
            native_bundle(&current_snapshot),
            manager.clone(),
        ))
        .expect("应启动传输服务器");
    let app = mock_builder()
        .manage(AppState::default())
        .manage(ReceiveState::default())
        .invoke_handler(tauri::generate_handler![
            agentbridge_lib::commands::preview_share,
        ])
        .build({
            let mut context = tauri::test::mock_context(tauri::test::noop_assets());
            let authority: &mut RuntimeAuthority = context.runtime_authority_mut();
            authority.__allow_command("preview_share".into(), ExecutionContext::Local);
            context
        })
        .expect("测试应用可创建");
    let pairing = PairingInfo {
        endpoint: handle.local_addr.to_string(),
        secret: invitation.secret.clone(),
        fingerprint: invitation.fingerprint.clone(),
        verification_code: "999999".into(),
        expires_at: invitation.expires_at,
    };
    let response = invoke(
        &app,
        "preview_share",
        0,
        serde_json::json!({
            "request": { "pairingInfo": serde_json::to_string(&pairing).unwrap() },
        }),
    )
    .0
    .recv_timeout(std::time::Duration::from_secs(5))
    .expect("命令应响应")
    .expect_err("错误验证码应被拒绝");
    assert_eq!(response["code"], "invalidPairing");
    assert_eq!(manager.status(&invitation.id), Ok(InvitationState::Waiting));
    handle.stop();
}

#[test]
fn cancelling_a_transfer_aborts_the_download_task() {
    let runtime = test_runtime();
    let _guard = runtime.enter();
    let manager = Arc::new(InvitationManager::new());
    let invitation =
        manager.create_with_options("snapshot-1", IpAddr::V4(Ipv4Addr::LOCALHOST), 0, 60);
    let current_snapshot = snapshot();
    let handle = runtime
        .block_on(TransferServer::serve(
            invitation.clone(),
            current_snapshot.clone(),
            native_bundle(&current_snapshot),
            manager.clone(),
        ))
        .expect("应启动传输服务器");
    let app = mock_builder()
        .manage(AppState::default())
        .manage(ReceiveState::default())
        .invoke_handler(tauri::generate_handler![
            agentbridge_lib::commands::preview_share,
            agentbridge_lib::commands::cancel_transfer,
        ])
        .build({
            let mut context = tauri::test::mock_context(tauri::test::noop_assets());
            let authority: &mut RuntimeAuthority = context.runtime_authority_mut();
            for command in ["preview_share", "cancel_transfer"] {
                authority.__allow_command(command.into(), ExecutionContext::Local);
            }
            context
        })
        .expect("测试应用可创建");
    let pairing = PairingInfo {
        endpoint: handle.local_addr.to_string(),
        secret: invitation.secret.clone(),
        fingerprint: invitation.fingerprint.clone(),
        verification_code: invitation.verification_code.clone(),
        expires_at: invitation.expires_at,
    };
    invoke(
        &app,
        "preview_share",
        0,
        serde_json::json!({
            "request": { "pairingInfo": serde_json::to_string(&pairing).unwrap() },
        }),
    )
    .get();
    let receive_state = app.state::<ReceiveState>();
    let client = receive_state
        .clients
        .lock()
        .expect("接收客户端锁可读取")
        .remove(&invitation.id)
        .expect("预览应保存待接收客户端")
        .client;
    let task = receive_state.runtime.spawn(async move {
        tokio::time::sleep(Duration::from_secs(5)).await;
        client.accept(None).await
    });
    let abort = task.abort_handle();
    receive_state
        .transfers
        .lock()
        .expect("传输任务锁可读取")
        .insert(invitation.id.clone(), abort);
    invoke(
        &app,
        "cancel_transfer",
        1,
        serde_json::json!({
            "shareId": invitation.id,
        }),
    )
    .get();
    let result = runtime.block_on(task);
    assert!(result.is_err() && result.expect_err("应中止").is_cancelled());
    let receive_state = app.state::<ReceiveState>();
    assert!(
        receive_state
            .transfers
            .lock()
            .expect("传输任务锁可读取")
            .is_empty()
    );
    handle.stop();
}

#[test]
fn approved_transfer_stages_a_validated_snapshot_without_creating_a_thread() {
    let runtime = test_runtime();
    let _guard = runtime.enter();
    let manager = Arc::new(InvitationManager::new());
    let invitation =
        manager.create_with_options("snapshot-1", IpAddr::V4(Ipv4Addr::LOCALHOST), 0, 60);
    let current_snapshot = snapshot();
    let handle = runtime
        .block_on(TransferServer::serve(
            invitation.clone(),
            current_snapshot.clone(),
            native_bundle(&current_snapshot),
            manager.clone(),
        ))
        .expect("应启动传输服务器");

    let app = mock_builder()
        .manage(AppState::default())
        .manage(ReceiveState::default())
        .invoke_handler(tauri::generate_handler![
            agentbridge_lib::commands::preview_share,
            agentbridge_lib::commands::accept_share,
            agentbridge_lib::commands::list_imports,
            agentbridge_lib::commands::delete_staged_snapshot,
        ])
        .build({
            let mut context = tauri::test::mock_context(tauri::test::noop_assets());
            let authority: &mut RuntimeAuthority = context.runtime_authority_mut();
            for command in [
                "preview_share",
                "accept_share",
                "list_imports",
                "delete_staged_snapshot",
            ] {
                authority.__allow_command(command.into(), ExecutionContext::Local);
            }
            context
        })
        .expect("测试应用可创建");
    let pairing = PairingInfo {
        endpoint: handle.local_addr.to_string(),
        secret: invitation.secret.clone(),
        fingerprint: invitation.fingerprint.clone(),
        verification_code: invitation.verification_code.clone(),
        expires_at: invitation.expires_at,
    };
    let preview: serde_json::Value = invoke(
        &app,
        "preview_share",
        0,
        serde_json::json!({
            "request": { "pairingInfo": serde_json::to_string(&pairing).unwrap() },
        }),
    )
    .get();
    assert_eq!(preview["metadata"]["messageCount"], 1);
    assert_eq!(
        preview["metadata"]["warnings"][0],
        "摘要预览：已省略图片附件"
    );

    let staged_value = invoke(
        &app,
        "accept_share",
        1,
        serde_json::json!({
            "shareId": preview["shareId"],
            "onEvent": "__CHANNEL__:1",
        }),
    )
    .get();
    assert!(
        !staged_value.to_string().contains(&invitation.secret),
        "暂存结果不得回传一次性凭据"
    );
    let staged: StagedSnapshot = serde_json::from_value(staged_value).expect("暂存结果应可解析");
    assert_eq!(staged.metadata.share_id, preview["shareId"]);
    let bytes = fs::read(&staged.snapshot_path).expect("暂存文件应存在");
    assert_eq!(
        native::parse_bundle(&bytes)
            .expect("暂存文件应通过原生会话包校验")
            .manifest
            .source_thread_id,
        "source-thread"
    );
    assert!(
        !staged.snapshot_path.contains("/Users/alice/project"),
        "分享方路径不能被当作本机暂存路径"
    );
    let imports = invoke(&app, "list_imports", 8, serde_json::json!({}))
        .0
        .recv_timeout(std::time::Duration::from_secs(5))
        .expect("命令应响应");
    if let Ok(value) = imports {
        assert_eq!(
            value.as_array().map(Vec::len),
            Some(0),
            "批准下载不能创建 Codex thread"
        );
    }
    let progress = progress_values(&app);
    assert!(!progress.is_empty(), "批准下载应产生真实进度事件");
    let last = progress.last().expect("至少有一个进度事件");
    assert_eq!(last.transferred_bytes, last.total_bytes);
    assert_eq!(last.total_bytes, u64::from(staged.total_bytes));
    invoke(
        &app,
        "delete_staged_snapshot",
        2,
        serde_json::json!({ "shareId": preview["shareId"] }),
    )
    .get();
    assert!(!std::path::Path::new(&staged.snapshot_path).exists());
    assert!(
        certificate_for(&invitation.fingerprint).is_none(),
        "传输完成后证书应清理"
    );
    handle.stop();
}
