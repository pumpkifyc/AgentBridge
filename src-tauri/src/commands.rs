use crate::{
    codex::{client::CodexClient, error::CodexError, models::Page},
    pairing::{InvitationManager, InvitationState},
    snapshot::{schema::SnapshotV1, validate::validate_snapshot},
    transfer::TransferServer,
};
use serde::{Deserialize, Serialize};
use sha2::Digest;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};
use tauri::{State, ipc::Channel};
use tokio::runtime::{Builder, Runtime};

const MIN_COMPATIBLE_CLI: (u32, u32, u32) = (0, 142, 2);
const IMPORT_TURN_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexStatus {
    pub state: String,
    pub detail: String,
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandError {
    pub message: String,
    pub code: String,
}

impl CommandError {
    fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}

impl From<CodexError> for CommandError {
    fn from(error: CodexError) -> Self {
        let code = match &error {
            CodexError::NotInstalled(_) => "notInstalled",
            CodexError::Timeout => "timeout",
            CodexError::Rpc { code: -32601, .. } => "unsupported",
            CodexError::Rpc { .. } => "rpcError",
            CodexError::Snapshot(_) => "invalidSnapshot",
            CodexError::ThreadNotFound(_) => "threadNotFound",
            CodexError::TurnFailed(_) => "turnFailed",
            _ => "codexUnavailable",
        };
        Self::new(code, error.to_string())
    }
}

pub struct AppState {
    pub invitations: Arc<InvitationManager>,
    servers: Mutex<HashMap<String, crate::transfer::TransferServerHandle>>,
    snapshots: Mutex<HashMap<String, SnapshotV1>>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            invitations: Arc::new(InvitationManager::new()),
            servers: Mutex::new(HashMap::new()),
            snapshots: Mutex::new(HashMap::new()),
        }
    }
}

fn compatible(version: &str) -> bool {
    let parts: Vec<u32> = version
        .split_whitespace()
        .find(|word| word.starts_with(|c: char| c.is_ascii_digit()))
        .unwrap_or("0.0.0")
        .split('.')
        .map(|part| part.parse().unwrap_or(0))
        .collect();
    let version = (
        parts.first().copied().unwrap_or(0),
        parts.get(1).copied().unwrap_or(0),
        parts.get(2).copied().unwrap_or(0),
    );
    version >= MIN_COMPATIBLE_CLI
}

fn codex_version() -> Result<String, CommandError> {
    let output = std::process::Command::new("codex")
        .arg("--version")
        .output()
        .map_err(|error| CommandError::new("notInstalled", format!("找不到 Codex CLI：{error}")))?;
    if !output.status.success() {
        return Err(CommandError::new(
            "codexUnavailable",
            "Codex CLI 返回了非零退出状态",
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

#[tauri::command]
pub fn codex_status() -> Result<CodexStatus, CommandError> {
    let version = codex_version()?;
    if !compatible(&version) {
        return Ok(CodexStatus {
            state: "unsupported".into(),
            detail: format!("Codex CLI {version} 版本过低，请升级到与桌面版匹配的版本。"),
            version: Some(version),
        });
    }
    let mut client = CodexClient::connect().map_err(CommandError::from)?;
    let page = client.list_threads(None).map_err(CommandError::from)?;
    if page.data.is_empty() {
        return Ok(CodexStatus {
            state: "unsupported".into(),
            detail: "本机 Codex 没有通过公开接口返回会话。请确认桌面版与 CLI 版本一致，且会话均为受支持的历史格式。".into(),
            version: Some(version),
        });
    }
    Ok(CodexStatus {
        state: "available".into(),
        detail: "本机 Codex 可用".into(),
        version: Some(version),
    })
}

#[tauri::command]
pub fn list_threads(cursor: Option<String>) -> Result<Page<crate::codex::models::ThreadSummary>, CommandError> {
    let mut client = CodexClient::connect().map_err(CommandError::from)?;
    client.list_threads(cursor).map_err(CommandError::from)
}

#[tauri::command]
pub fn list_turns(thread_id: String) -> Result<Vec<crate::codex::models::TurnSummary>, CommandError> {
    let mut client = CodexClient::connect().map_err(CommandError::from)?;
    client.list_turns(&thread_id).map_err(CommandError::from)
}

#[tauri::command]
pub fn preview_snapshot(
    state: State<'_, AppState>,
    thread_id: String,
    cutoff_turn_id: Option<String>,
) -> Result<SnapshotV1, CommandError> {
    let mut client = CodexClient::connect().map_err(CommandError::from)?;
    let snapshot = client
        .read_snapshot(&thread_id, cutoff_turn_id.as_deref())
        .map_err(CommandError::from)?;
    let bytes = serde_json::to_vec(&snapshot)
        .map_err(|error| CommandError::new("invalidSnapshot", error.to_string()))?;
    let validated = validate_snapshot(&bytes)
        .map_err(|error| CommandError::new("invalidSnapshot", error.to_string()))?;
    state
        .snapshots
        .lock()
        .expect("快照锁已中毒")
        .insert(thread_id, validated.clone());
    Ok(validated)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShareSession {
    pub id: String,
    pub endpoint: String,
    pub secret: String,
    pub verification_code: String,
    pub expires_at: i64,
    pub state: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartShareRequest {
    pub thread_id: String,
    pub bind_ip: String,
}

#[tauri::command]
pub async fn start_share(
    state: State<'_, AppState>,
    request: StartShareRequest,
) -> Result<ShareSession, CommandError> {
    let snapshot = state
        .snapshots
        .lock()
        .expect("快照锁已中毒")
        .get(&request.thread_id)
        .cloned()
        .ok_or_else(|| CommandError::new("invalidSnapshot", "请先预览要分享的会话"))?;
    let ip = request
        .bind_ip
        .parse()
        .map_err(|error| CommandError::new("invalidEndpoint", format!("局域网地址无效：{error}")))?;
    let invitation = state.invitations.create_with_options(
        request.thread_id.clone(),
        ip,
        0,
        crate::pairing::DEFAULT_TTL_SECONDS,
    );
    let handle = TransferServer::serve(
        invitation.clone(),
        snapshot,
        Arc::clone(&state.invitations),
    )
    .await
    .map_err(|error| CommandError::new("serverFailed", error.to_string()))?;
    let endpoint = handle.local_addr.to_string();
    state
        .servers
        .lock()
        .expect("服务器锁已中毒")
        .insert(invitation.id.clone(), handle);
    Ok(ShareSession {
        id: invitation.id,
        endpoint,
        secret: invitation.secret,
        verification_code: invitation.verification_code,
        expires_at: invitation.expires_at,
        state: InvitationState::Waiting.display_name().into(),
    })
}

#[tauri::command]
pub fn share_status(state: State<'_, AppState>, invitation_id: String) -> Result<String, CommandError> {
    let invitation = state
        .invitations
        .get(&invitation_id)
        .map_err(|error| CommandError::new("inactiveInvitation", error.to_string()))?;
    Ok(invitation.state.display_name().to_owned())
}

#[tauri::command]
pub fn revoke_share(state: State<'_, AppState>, invitation_id: String) -> Result<(), CommandError> {
    state
        .invitations
        .revoke(&invitation_id)
        .map_err(|error| CommandError::new("inactiveInvitation", error.to_string()))?;
    if let Some(handle) = state
        .servers
        .lock()
        .expect("服务器锁已中毒")
        .remove(&invitation_id)
    {
        handle.stop();
    }
    Ok(())
}

#[tauri::command]
pub fn cancel_share(state: State<'_, AppState>, invitation_id: String) -> Result<(), CommandError> {
    state
        .invitations
        .cancel(&invitation_id)
        .map_err(|error| CommandError::new("inactiveInvitation", error.to_string()))?;
    if let Some(handle) = state
        .servers
        .lock()
        .expect("服务器锁已中毒")
        .remove(&invitation_id)
    {
        handle.stop();
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewShareRequest {
    pub endpoint: String,
    pub secret: String,
    pub fingerprint: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReceivedPreview {
    pub share_id: String,
    pub metadata: crate::transfer::ShareMetadata,
    pub total_bytes: u32,
    pub chunk_count: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportRecord {
    pub id: String,
    pub source_title: String,
    pub local_thread_id: String,
    pub project_path: String,
    pub imported_at: i64,
    pub snapshot_path: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotPreviewText {
    pub messages: Vec<String>,
    pub continuation_prompt: String,
}

pub struct ReceiveState {
    pub clients: Mutex<HashMap<String, crate::transfer::TransferClient>>,
    pub imports: Mutex<Vec<ImportRecord>>,
    pub runtime: Runtime,
}

impl Default for ReceiveState {
    fn default() -> Self {
        Self {
            clients: Mutex::new(HashMap::new()),
            imports: Mutex::new(Vec::new()),
            runtime: Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
                .expect("创建接收运行时失败"),
        }
    }
}

#[tauri::command]
pub async fn preview_share(
    state: State<'_, ReceiveState>,
    request: PreviewShareRequest,
) -> Result<ReceivedPreview, CommandError> {
    let endpoint = request.endpoint.clone();
    let secret = request.secret.clone();
    let fingerprint = request.fingerprint.clone();
    let (client, proposal) = state
        .runtime
        .spawn(async move {
            crate::transfer::TransferClient::preview(&endpoint, &secret, &fingerprint).await
        })
        .await
        .map_err(|error| CommandError::new("transferFailed", error.to_string()))?
        .map_err(|error| CommandError::new("transferFailed", error.to_string()))?;
    let share_id = proposal.metadata.share_id.clone();
    state
        .clients
        .lock()
        .expect("接收客户端锁已中毒")
        .insert(share_id.clone(), client);
    Ok(ReceivedPreview {
        share_id,
        metadata: proposal.metadata,
        total_bytes: proposal.total_bytes,
        chunk_count: proposal.chunk_count,
    })
}

#[tauri::command]
pub async fn decline_share(state: State<'_, ReceiveState>, share_id: String) -> Result<(), CommandError> {
    let client = state
        .clients
        .lock()
        .expect("接收客户端锁已中毒")
        .remove(&share_id)
        .ok_or_else(|| CommandError::new("notFound", "找不到待处理的分享"))?;
    state
        .runtime
        .spawn(async move { client.decline().await })
        .await
        .map_err(|error| CommandError::new("transferFailed", error.to_string()))?
        .map_err(|error| CommandError::new("transferFailed", error.to_string()))
}

#[tauri::command]
pub async fn accept_share(
    state: State<'_, ReceiveState>,
    share_id: String,
    on_event: Channel<crate::transfer::protocol::ServerMessage>,
) -> Result<SnapshotV1, CommandError> {
    let client = state
        .clients
        .lock()
        .expect("接收客户端锁已中毒")
        .remove(&share_id)
        .ok_or_else(|| CommandError::new("notFound", "找不到待处理的分享"))?;
    let _ = on_event.send(crate::transfer::protocol::ServerMessage::Declined);
    let snapshot = state
        .runtime
        .spawn(async move { client.accept().await })
        .await
        .map_err(|error| CommandError::new("transferFailed", error.to_string()))?
        .map_err(|error| CommandError::new("transferFailed", error.to_string()))?;
    Ok(snapshot)
}

#[tauri::command]
pub fn cancel_transfer(state: State<'_, ReceiveState>, share_id: String) -> Result<(), CommandError> {
    state
        .clients
        .lock()
        .expect("接收客户端锁已中毒")
        .remove(&share_id)
        .ok_or_else(|| CommandError::new("notFound", "找不到进行中的传输"))?;
    Ok(())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportRequest {
    pub share_id: String,
    pub project_path: String,
    pub snapshot: SnapshotV1,
}

#[tauri::command]
pub fn import_snapshot(
    state: State<'_, ReceiveState>,
    request: ImportRequest,
) -> Result<ImportRecord, CommandError> {
    let bytes = serde_json::to_vec(&request.snapshot)
        .map_err(|error| CommandError::new("invalidSnapshot", error.to_string()))?;
    let snapshot = validate_snapshot(&bytes)
        .map_err(|error| CommandError::new("invalidSnapshot", error.to_string()))?;
    let project = std::path::Path::new(&request.project_path);
    if !project.is_dir() {
        return Err(CommandError::new(
            "invalidProject",
            "请选择接收方电脑上存在的项目目录",
        ));
    }
    let prompt = crate::imports::build_continuation_prompt(&snapshot);
    let mut client = CodexClient::connect().map_err(CommandError::from)?;
    let local_thread_id = client
        .start_thread(&request.project_path)
        .map_err(CommandError::from)?;
    client
        .start_turn(&local_thread_id, &request.project_path, &prompt)
        .map_err(CommandError::from)?;
    client
        .wait_for_turn_completion(IMPORT_TURN_TIMEOUT)
        .map_err(CommandError::from)?;
    let record = ImportRecord {
        id: format!("import-{}", hex::encode(sha2::Sha256::digest(local_thread_id.as_bytes()))),
        source_title: snapshot.metadata.title,
        local_thread_id,
        project_path: request.project_path,
        imported_at: crate::pairing::invitation::unix_seconds(),
        snapshot_path: format!("{}", std::path::Path::new(&request.share_id).display()),
        status: "已完成".into(),
    };
    state
        .imports
        .lock()
        .expect("导入记录锁已中毒")
        .push(record.clone());
    Ok(record)
}

#[tauri::command]
pub fn list_imports(state: State<'_, ReceiveState>) -> Result<Vec<ImportRecord>, CommandError> {
    Ok(state
        .imports
        .lock()
        .expect("导入记录锁已中毒")
        .clone())
}

#[tauri::command]
pub fn delete_import(state: State<'_, ReceiveState>, import_id: String) -> Result<(), CommandError> {
    let mut imports = state.imports.lock().expect("导入记录锁已中毒");
    let before = imports.len();
    imports.retain(|record| record.id != import_id);
    if imports.len() == before {
        return Err(CommandError::new("notFound", "找不到导入记录"));
    }
    Ok(())
}

