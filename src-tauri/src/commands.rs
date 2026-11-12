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
    fs,
    sync::{Arc, Mutex},
    time::Duration,
};
use tauri::{Manager, Runtime as TauriRuntime, State, WebviewWindow, ipc::Channel};
use tokio::{runtime::{Builder, Runtime}, task::AbortHandle};

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
    pub servers: Mutex<HashMap<String, crate::transfer::TransferServerHandle>>,
    snapshots: Mutex<HashMap<String, SnapshotV1>>,
}

impl AppState {
    pub fn remove_finished_servers(&self) {
        let mut servers = self.servers.lock().expect("服务器锁已中毒");
        servers.retain(|_, handle| !handle.is_stopped());
    }
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
    pub fingerprint: String,
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
        fingerprint: invitation.fingerprint,
        verification_code: invitation.verification_code,
        expires_at: invitation.expires_at,
        state: InvitationState::Waiting.display_name().into(),
    })
}

#[tauri::command]
pub fn share_status(state: State<'_, AppState>, invitation_id: String) -> Result<String, CommandError> {
    state.remove_finished_servers();
    let invitation = state
        .invitations
        .status(&invitation_id)
        .map_err(|error| CommandError::new("inactiveInvitation", error.to_string()))?;
    Ok(invitation.display_name().to_owned())
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
    pub pairing_info: String,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StagedSnapshot {
    pub share_id: String,
    pub endpoint: String,
    pub fingerprint: String,
    pub metadata: crate::transfer::ShareMetadata,
    pub snapshot_path: String,
    pub total_bytes: u32,
    pub chunk_count: u32,
}

pub struct ReceiveState {
    pub clients: Mutex<HashMap<String, crate::transfer::client::PendingTransfer>>,
    pub transfers: Mutex<HashMap<String, AbortHandle>>,
    pub staged: Mutex<HashMap<String, StagedSnapshot>>,
    pub progress: Mutex<Vec<serde_json::Value>>,
    pub imports: Mutex<Vec<ImportRecord>>,
    pub runtime: Runtime,
}

impl Default for ReceiveState {
    fn default() -> Self {
        Self {
            clients: Mutex::new(HashMap::new()),
            transfers: Mutex::new(HashMap::new()),
            staged: Mutex::new(HashMap::new()),
            progress: Mutex::new(Vec::new()),
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
    let pairing: crate::transfer::protocol::PairingInfo =
        serde_json::from_str(&request.pairing_info)
            .map_err(|error| CommandError::new("invalidPairing", format!("配对信息无效：{error}")))?;
    validate_received_pairing(&pairing)
        .map_err(|error| CommandError::new("invalidPairing", error))?;
    let endpoint = pairing.endpoint.clone();
    let secret = pairing.secret.clone();
    let fingerprint = pairing.fingerprint.clone();
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
        .insert(
            share_id.clone(),
            crate::transfer::client::PendingTransfer {
                client,
                pairing,
                proposal: proposal.clone(),
            },
        );
    Ok(ReceivedPreview {
        share_id,
        metadata: proposal.metadata,
        total_bytes: proposal.total_bytes,
        chunk_count: proposal.chunk_count,
    })
}

#[tauri::command]
pub async fn decline_share(state: State<'_, ReceiveState>, share_id: String) -> Result<(), CommandError> {
    let pending = state
        .clients
        .lock()
        .expect("接收客户端锁已中毒")
        .remove(&share_id)
        .ok_or_else(|| CommandError::new("notFound", "找不到待处理的分享"))?;
    state
        .runtime
        .spawn(async move { pending.client.decline().await })
        .await
        .map_err(|error| CommandError::new("transferFailed", error.to_string()))?
        .map_err(|error| CommandError::new("transferFailed", error.to_string()))
}

#[tauri::command]
pub async fn accept_share(
    window: WebviewWindow<impl TauriRuntime>,
    state: State<'_, ReceiveState>,
    share_id: String,
    on_event: Channel<crate::transfer::client::TransferProgress>,
) -> Result<StagedSnapshot, CommandError> {
    let pending = state
        .clients
        .lock()
        .expect("接收客户端锁已中毒")
        .remove(&share_id)
        .ok_or_else(|| CommandError::new("notFound", "找不到待处理的分享"))?;
    let pairing = pending.pairing.clone();
    let proposal = pending.proposal.clone();
    let progress: crate::transfer::client::ProgressCallback = Arc::new(move |event| {
        let _ = on_event.send(event);
    });
    let task = state
        .runtime
        .spawn(async move { pending.client.accept(Some(progress)).await });
    state
        .transfers
        .lock()
        .expect("传输任务锁已中毒")
        .insert(share_id.clone(), task.abort_handle());
    let result = task.await;
    let removed = state
        .transfers
        .lock()
        .expect("传输任务锁已中毒")
        .remove(&share_id);
    if removed.is_none() {
        return Err(CommandError::new("cancelled", "下载已取消"));
    }
    let (snapshot, progress_events) = result
        .map_err(|error| CommandError::new("transferFailed", error.to_string()))?
        .map_err(|error| CommandError::new("transferFailed", error.to_string()))?;
    let bytes = serde_json::to_vec(&snapshot)
        .map_err(|error| CommandError::new("invalidSnapshot", error.to_string()))?;
    validate_snapshot(&bytes)
        .map_err(|error| CommandError::new("invalidSnapshot", error.to_string()))?;
    let path = staging_path(&window, &share_id)?;
    fs::write(&path, &bytes)
        .map_err(|error| CommandError::new("stagingFailed", format!("暂存快照失败：{error}")))?;
    let staged = StagedSnapshot {
        share_id: share_id.clone(),
        endpoint: pairing.endpoint,
        fingerprint: pairing.fingerprint,
        metadata: proposal.metadata,
        snapshot_path: path.to_string_lossy().into_owned(),
        total_bytes: proposal.total_bytes,
        chunk_count: proposal.chunk_count,
    };
    state
        .staged
        .lock()
        .expect("暂存锁已中毒")
        .insert(share_id.clone(), staged.clone());
    state
        .progress
        .lock()
        .expect("进度锁已中毒")
        .extend(progress_events);
    Ok(staged)
}

pub fn validate_received_pairing(
    pairing: &crate::transfer::protocol::PairingInfo,
) -> Result<(), String> {
    const FINGERPRINT_PREFIX: &str = "sha256:";
    if pairing.endpoint.trim().is_empty()
        || pairing.secret.trim().is_empty()
        || pairing.fingerprint.len() != FINGERPRINT_PREFIX.len() + 64
        || !pairing.fingerprint.starts_with(FINGERPRINT_PREFIX)
        || !pairing.fingerprint[FINGERPRINT_PREFIX.len()..]
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
        || pairing.verification_code != crate::pairing::invitation::short_code(&pairing.fingerprint)
    {
        return Err("配对信息无效，请核对验证码和证书指纹，并从分享方重新复制完整 JSON。".into());
    }
    Ok(())
}

fn staging_path(
    window: &WebviewWindow<impl TauriRuntime>,
    share_id: &str,
) -> Result<std::path::PathBuf, CommandError> {
    let digest = hex::encode(sha2::Sha256::digest(share_id.as_bytes()));
    let directory = window
        .path()
        .app_data_dir()
        .map_err(|error| CommandError::new("stagingFailed", format!("无法定位应用数据目录：{error}")))?
        .join("staged-snapshots");
    fs::create_dir_all(&directory)
        .map_err(|error| CommandError::new("stagingFailed", format!("创建暂存目录失败：{error}")))?;
    Ok(directory.join(format!("{digest}.json")))
}

#[tauri::command]
pub fn cancel_transfer(state: State<'_, ReceiveState>, share_id: String) -> Result<(), CommandError> {
    let task = state
        .transfers
        .lock()
        .expect("传输任务锁已中毒")
        .remove(&share_id);
    if let Some(task) = task {
        task.abort();
        return Ok(());
    }
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
pub struct RetryPreviewRequest {
    pub pairing_info: String,
    pub share_id: String,
}

#[tauri::command]
pub async fn retry_preview(
    state: State<'_, ReceiveState>,
    request: RetryPreviewRequest,
) -> Result<ReceivedPreview, CommandError> {
    state
        .clients
        .lock()
        .expect("接收客户端锁已中毒")
        .remove(&request.share_id);
    preview_share(
        state,
        PreviewShareRequest {
            pairing_info: request.pairing_info,
        },
    )
    .await
}

#[tauri::command]
pub fn delete_staged_snapshot(
    state: State<'_, ReceiveState>,
    share_id: String,
) -> Result<(), CommandError> {
    let staged = state
        .staged
        .lock()
        .expect("暂存锁已中毒")
        .remove(&share_id)
        .ok_or_else(|| CommandError::new("notFound", "找不到本机暂存快照"))?;
    fs::remove_file(&staged.snapshot_path)
        .map_err(|error| CommandError::new("deleteFailed", format!("删除暂存快照失败：{error}")))?;
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

