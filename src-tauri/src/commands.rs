use crate::{
    codex::{
        client::CodexClient,
        error::CodexError,
        models::{Page, RecordedThread, ThreadSummary},
    },
    db::ImportDatabase,
    imports::{ImportError, ImportRecord, ImportService},
    native::{self, NativeBundleError},
    pairing::{InvitationManager, InvitationState},
    preview::{convert::convert_preview_timeline, schema::PreviewTimelineV1},
    snapshot::{
        convert::convert_codex_items, schema::SnapshotV1, validate::validate_local_snapshot,
    },
    transfer::TransferServer,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    fs,
    net::{IpAddr, Ipv4Addr},
    path::{Component, Path, PathBuf},
    sync::{Arc, Mutex},
};
use tauri::{Manager, Runtime as TauriRuntime, State, WebviewWindow, ipc::Channel};
use tauri_plugin_dialog::DialogExt;
use tokio::{
    runtime::{Builder, Runtime},
    task::AbortHandle,
};

const MIN_COMPATIBLE_CLI: (u32, u32, u32) = (0, 167, 2);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalAddress {
    pub ip: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexStatus {
    pub state: String,
    pub detail: String,
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CodexProject {
    pub path: String,
    pub name: String,
    pub thread_count: usize,
    pub updated_at: i64,
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

impl From<NativeBundleError> for CommandError {
    fn from(error: NativeBundleError) -> Self {
        Self::new("invalidNativeBundle", error.to_string())
    }
}

pub struct AppState {
    pub invitations: Arc<InvitationManager>,
    pub servers: Mutex<HashMap<String, crate::transfer::TransferServerHandle>>,
    pub codex: Arc<Mutex<Option<CodexClient>>>,
    previews: PreviewRegistry,
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
            codex: Arc::new(Mutex::new(None)),
            previews: PreviewRegistry::default(),
        }
    }
}

#[derive(Debug, Clone)]
struct PreparedPreview {
    thread_id: String,
    snapshot: SnapshotV1,
    timeline: PreviewTimelineV1,
    native_bundle: Vec<u8>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GeneratedPreview {
    pub snapshot: SnapshotV1,
    pub timeline: PreviewTimelineV1,
}

#[derive(Default)]
struct PreviewRegistry {
    state: Mutex<PreviewRegistryState>,
}

#[derive(Default)]
struct PreviewRegistryState {
    entries: HashMap<String, PreparedPreview>,
    cancelled: HashSet<String>,
}

impl PreviewRegistry {
    fn insert(
        &self,
        request: &PreviewSnapshotRequest,
        snapshot: SnapshotV1,
        timeline: PreviewTimelineV1,
        native_bundle: Vec<u8>,
    ) -> Result<(), CommandError> {
        request.validate()?;
        let mut state = self.state.lock().expect("预览锁已中毒");
        if state.cancelled.remove(&request.preview_id) {
            return Err(CommandError::new("cancelled", "预览已取消，忽略迟到结果"));
        }
        if state.entries.contains_key(&request.preview_id) {
            return Err(CommandError::new(
                "invalidPreview",
                "预览标识重复，请重新生成分享预览",
            ));
        }
        state.entries.insert(
            request.preview_id.clone(),
            PreparedPreview {
                thread_id: request.thread_id.clone(),
                snapshot,
                timeline,
                native_bundle,
            },
        );
        Ok(())
    }

    fn snapshot_for_share(
        &self,
        thread_id: &str,
        preview_id: &str,
    ) -> Result<(SnapshotV1, Vec<u8>), CommandError> {
        validate_preview_id(preview_id)?;
        let preview = self
            .state
            .lock()
            .expect("预览锁已中毒")
            .entries
            .get(preview_id)
            .cloned()
            .ok_or_else(|| CommandError::new("invalidPreview", "分享预览已失效，请重新生成"))?;
        if preview.thread_id != thread_id {
            return Err(CommandError::new(
                "invalidPreview",
                "分享预览与会话不匹配，请重新生成",
            ));
        }
        Ok((preview.snapshot, preview.native_bundle))
    }

    fn display_for_preview(&self, preview_id: &str) -> Result<GeneratedPreview, CommandError> {
        validate_preview_id(preview_id)?;
        let preview = self
            .state
            .lock()
            .expect("预览锁已中毒")
            .entries
            .get(preview_id)
            .cloned()
            .ok_or_else(|| CommandError::new("invalidPreview", "分享预览已失效，请重新生成"))?;
        Ok(GeneratedPreview {
            snapshot: preview.snapshot,
            timeline: preview.timeline,
        })
    }

    fn cancel(&self, preview_id: &str) {
        if !preview_id.is_empty() {
            let mut state = self.state.lock().expect("预览锁已中毒");
            if state.entries.remove(preview_id).is_none() {
                if state.cancelled.len() >= MAX_CANCELLED_PREVIEW_IDS
                    && let Some(stale) = state.cancelled.iter().next().cloned()
                {
                    state.cancelled.remove(&stale);
                }
                state.cancelled.insert(preview_id.to_owned());
            }
        }
    }
}

const MAX_PREVIEW_ID_LENGTH: usize = 128;
const MAX_CANCELLED_PREVIEW_IDS: usize = 1024;

fn validate_preview_id(preview_id: &str) -> Result<(), CommandError> {
    if preview_id.trim().is_empty() || preview_id.len() > MAX_PREVIEW_ID_LENGTH {
        return Err(CommandError::new(
            "invalidPreview",
            "预览标识无效，请重新生成分享预览",
        ));
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewSnapshotRequest {
    pub thread_id: String,
    pub cutoff_turn_id: Option<String>,
    pub preview_id: String,
}

impl PreviewSnapshotRequest {
    fn validate(&self) -> Result<(), CommandError> {
        if self.thread_id.trim().is_empty() {
            return Err(CommandError::new("threadNotFound", "请选择要分享的会话"));
        }
        if self
            .cutoff_turn_id
            .as_deref()
            .is_some_and(|turn_id| turn_id.trim().is_empty())
        {
            return Err(CommandError::new(
                "invalidSnapshot",
                "截止轮次无效，请重新选择",
            ));
        }
        validate_preview_id(&self.preview_id)
    }
}

#[cfg(test)]
mod preview_registry_tests {
    use super::*;
    use crate::codex::models::{RecordedItem, RecordedThread, ThreadData, ThreadTurn};
    use crate::preview::schema::{PreviewNoticeScope, PreviewTimelineV1, PreviewTurn};
    use crate::snapshot::schema::SnapshotMetadata;
    use serde_json::json;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn request(thread_id: &str, preview_id: &str) -> PreviewSnapshotRequest {
        PreviewSnapshotRequest {
            thread_id: thread_id.into(),
            cutoff_turn_id: None,
            preview_id: preview_id.into(),
        }
    }

    fn snapshot(thread_id: &str) -> SnapshotV1 {
        SnapshotV1 {
            schema_version: 1,
            metadata: SnapshotMetadata {
                source_thread_id: thread_id.into(),
                title: "测试会话".into(),
                created_at: 1,
                updated_at: 2,
                cutoff_turn_id: None,
                project_name: None,
                repository_id: None,
                source_working_directory: None,
            },
            messages: Vec::new(),
            warnings: Vec::new(),
        }
    }

    fn timeline(thread_id: &str) -> PreviewTimelineV1 {
        PreviewTimelineV1 {
            schema_version: 1,
            source_thread_id: thread_id.into(),
            cutoff_turn_id: None,
            turns: vec![PreviewTurn {
                id: "turn-1".into(),
                index: 0,
                status: "completed".into(),
                started_at: Some(1),
                completed_at: Some(2),
                items: Vec::new(),
            }],
            notices: vec![crate::preview::schema::PreviewNotice {
                code: "unknown_items".into(),
                count: 1,
                message: "只影响预览，原生分享包仍完整保留".into(),
                scope: PreviewNoticeScope::DisplayOnly,
            }],
        }
    }

    fn recorded_thread(thread_id: &str, rollout_path: &std::path::Path) -> RecordedThread {
        RecordedThread {
            data: ThreadData {
                thread_id: thread_id.into(),
                title: "原生包失败测试".into(),
                created_at: 1,
                updated_at: 2,
                project_name: None,
                repository_id: None,
                working_directory: Some("/tmp/project".into()),
                turns: vec![ThreadTurn {
                    id: "turn-1".into(),
                    completed: true,
                    status: Some("completed".into()),
                    error: None,
                    started_at: Some(1),
                    completed_at: Some(2),
                    truncated: false,
                    items: vec![RecordedItem {
                        id: Some("agent-1".into()),
                        kind: "agentMessage".into(),
                        timestamp: Some(1),
                        raw: json!({
                            "type": "agentMessage",
                            "id": "agent-1",
                            "text": "测试"
                        }),
                    }],
                }],
            },
            rollout_path: Some(rollout_path.to_string_lossy().into_owned()),
        }
    }

    #[test]
    fn registry_keeps_timeline_for_local_display_but_share_returns_only_snapshot_and_bundle() {
        let registry = PreviewRegistry::default();
        let preview_request = request("thread-1", "preview-1");
        registry
            .insert(
                &preview_request,
                snapshot("thread-1"),
                timeline("thread-1"),
                vec![1, 2],
            )
            .expect("预览应保存成功");

        assert_eq!(
            registry
                .display_for_preview("preview-1")
                .expect("本机预览应可读取")
                .timeline
                .source_thread_id,
            "thread-1"
        );
        assert_eq!(
            registry
                .snapshot_for_share("thread-1", "preview-1")
                .expect("分享数据应可读取")
                .1,
            vec![1, 2]
        );
    }

    #[test]
    fn failed_native_bundle_does_not_leave_shareable_preview() {
        let registry = PreviewRegistry::default();
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let codex_home = std::env::temp_dir().join(format!(
            "agentbridge-preview-{}-{nanos}",
            std::process::id()
        ));
        std::fs::create_dir_all(&codex_home).unwrap();
        let missing_rollout = codex_home.join("sessions/missing.jsonl");
        let recorded = recorded_thread("thread-1", &missing_rollout);

        let error = prepare_preview_data("thread-1", None, recorded, &codex_home)
            .expect_err("不存在的 rollout 必须阻止生成分享");
        assert_eq!(error.code, "invalidNativeBundle");
        let share_error = registry
            .snapshot_for_share("thread-1", "preview-failed")
            .expect_err("失败预览不能进入分享注册表");
        assert_eq!(share_error.code, "invalidPreview");

        std::fs::remove_dir_all(codex_home).unwrap();
    }

    #[test]
    fn preview_token_is_bound_to_its_source_thread() {
        let registry = PreviewRegistry::default();
        let preview_request = request("thread-1", "preview-1");
        registry
            .insert(
                &preview_request,
                snapshot("thread-1"),
                timeline("thread-1"),
                vec![1, 2, 3],
            )
            .expect("预览应保存成功");

        let error = registry
            .snapshot_for_share("thread-2", "preview-1")
            .expect_err("预览令牌不能跨会话复用");
        assert_eq!(error.code, "invalidPreview");
        assert_eq!(
            registry
                .snapshot_for_share("thread-1", "preview-1")
                .expect("原会话应可继续使用预览")
                .0
                .metadata
                .source_thread_id,
            "thread-1"
        );
    }

    #[test]
    fn cancelling_a_preview_prevents_later_sharing() {
        let registry = PreviewRegistry::default();
        registry
            .insert(
                &request("thread-1", "preview-1"),
                snapshot("thread-1"),
                timeline("thread-1"),
                vec![1, 2, 3],
            )
            .expect("预览应保存成功");

        registry.cancel("preview-1");

        let error = registry
            .snapshot_for_share("thread-1", "preview-1")
            .expect_err("取消后的预览令牌必须失效");
        assert_eq!(error.code, "invalidPreview");
    }

    #[test]
    fn cancellation_before_generation_finishes_rejects_the_late_preview() {
        let registry = PreviewRegistry::default();
        let preview_request = request("thread-1", "preview-late");
        registry.cancel("preview-late");

        let error = registry
            .insert(
                &preview_request,
                snapshot("thread-1"),
                timeline("thread-1"),
                vec![1, 2, 3],
            )
            .expect_err("取消后迟到的预览不能重新插入");

        assert_eq!(error.code, "cancelled");
        assert!(registry.display_for_preview("preview-late").is_err());
    }

    #[test]
    fn duplicate_preview_token_is_rejected_without_overwriting_the_original() {
        let registry = PreviewRegistry::default();
        registry
            .insert(
                &request("thread-1", "preview-1"),
                snapshot("thread-1"),
                timeline("thread-1"),
                vec![1, 2, 3],
            )
            .expect("首个预览应保存成功");

        let error = registry
            .insert(
                &request("thread-2", "preview-1"),
                snapshot("thread-2"),
                timeline("thread-2"),
                vec![4, 5, 6],
            )
            .expect_err("重复预览标识必须被拒绝");
        assert_eq!(error.code, "invalidPreview");
        assert_eq!(
            registry
                .snapshot_for_share("thread-1", "preview-1")
                .expect("原预览不能被覆盖")
                .0
                .metadata
                .source_thread_id,
            "thread-1"
        );
    }

    #[test]
    fn rejects_the_old_cli_that_lacks_current_thread_items_api() {
        assert!(!compatible("codex-cli 0.142.2"));
        assert!(compatible("codex-cli 0.167.2"));
    }
}

fn with_codex<T>(
    codex: &Arc<Mutex<Option<CodexClient>>>,
    action: impl FnOnce(&mut CodexClient) -> Result<T, CommandError>,
) -> Result<T, CommandError> {
    let mut guard = codex
        .lock()
        .map_err(|_| CommandError::new("codexUnavailable", "Codex 连接状态不可用"))?;
    if guard.is_none() {
        *guard = Some(CodexClient::connect().map_err(CommandError::from)?);
    }
    let client = guard.as_mut().expect("Codex 连接已初始化");
    let result = action(client);
    if result.is_err() {
        *guard = None;
    }
    result
}

async fn run_codex_command<T: Send + 'static>(
    state: &AppState,
    action: impl FnOnce(&mut CodexClient) -> Result<T, CommandError> + Send + 'static,
) -> Result<T, CommandError> {
    let codex = Arc::clone(&state.codex);
    tokio::task::spawn_blocking(move || with_codex(&codex, action))
        .await
        .map_err(|error| CommandError::new("codexUnavailable", error.to_string()))?
}

fn local_ipv4_addresses() -> Vec<LocalAddress> {
    let mut addresses: Vec<LocalAddress> = if_addrs::get_if_addrs()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|interface| match interface.ip() {
            IpAddr::V4(ip) if !ip.is_loopback() && !ip.is_link_local() => Some(LocalAddress {
                ip: ip.to_string(),
                name: interface.name,
            }),
            _ => None,
        })
        .collect();
    addresses = sort_local_addresses(addresses);
    addresses.dedup_by(|left, right| left.ip == right.ip);
    addresses
}

fn sort_local_addresses(mut addresses: Vec<LocalAddress>) -> Vec<LocalAddress> {
    addresses.sort_by(|left, right| {
        local_address_priority(left)
            .cmp(&local_address_priority(right))
            .then(left.ip.cmp(&right.ip))
            .then(left.name.cmp(&right.name))
    });
    addresses
}

fn local_address_priority(address: &LocalAddress) -> u8 {
    let name = address.name.to_ascii_lowercase();
    if [
        "vethernet",
        "virtual",
        "vmware",
        "virtualbox",
        "vbox",
        "hyper-v",
        "docker",
        "wsl",
        "tailscale",
        "zerotier",
        "npcap",
        "tap-",
        "tun",
        "utun",
    ]
    .iter()
    .any(|marker| name.contains(marker))
    {
        2
    } else if ["wi-fi", "wifi", "wlan", "ethernet", "eth", "en"]
        .iter()
        .any(|marker| name.contains(marker))
    {
        0
    } else {
        1
    }
}

#[tauri::command]
pub fn list_local_addresses() -> Vec<LocalAddress> {
    let mut addresses = local_ipv4_addresses();
    if addresses.is_empty() {
        addresses.push(LocalAddress {
            ip: Ipv4Addr::LOCALHOST.to_string(),
            name: "localhost".into(),
        });
    }
    addresses
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
    let executable =
        crate::codex::executable::resolve_codex_executable().map_err(CommandError::from)?;
    let mut command = std::process::Command::new(executable);
    crate::codex::executable::configure_codex_command(&mut command);
    let output = command
        .arg("--version")
        .output()
        .map_err(|error| CommandError::new("notInstalled", format!("找不到 Codex CLI：{error}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        return Err(CommandError::new(
            "codexUnavailable",
            codex_version_failure_message(output.status.code(), &stdout, &stderr),
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn codex_version_failure_message(exit_code: Option<i32>, stdout: &str, stderr: &str) -> String {
    let detail = if !stderr.trim().is_empty() {
        stderr.trim()
    } else if !stdout.trim().is_empty() {
        stdout.trim()
    } else {
        "未提供错误信息"
    };
    let exit_code = exit_code.map_or_else(|| "未知".into(), |code| code.to_string());
    format!("Codex CLI 返回了非零退出状态（退出码 {exit_code}）：{detail}")
}

#[tauri::command]
pub async fn codex_status() -> Result<CodexStatus, CommandError> {
    let version = tokio::task::spawn_blocking(codex_version)
        .await
        .map_err(|error| CommandError::new("codexUnavailable", error.to_string()))??;
    if !compatible(&version) {
        return Ok(CodexStatus {
            state: "unsupported".into(),
            detail: format!("Codex CLI {version} 版本过低，请升级到与桌面版匹配的版本。"),
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
pub async fn list_threads(
    state: State<'_, AppState>,
    cursor: Option<String>,
) -> Result<Page<crate::codex::models::ThreadSummary>, CommandError> {
    run_codex_command(&state, move |client| {
        client.list_threads(cursor).map_err(CommandError::from)
    })
    .await
}

#[tauri::command]
pub async fn list_codex_projects(
    state: State<'_, AppState>,
) -> Result<Vec<CodexProject>, CommandError> {
    let threads = run_codex_command(&state, move |client| {
        let mut threads = Vec::new();
        let mut cursor = None;
        for _ in 0..200 {
            let page = client.list_threads(cursor).map_err(CommandError::from)?;
            threads.extend(page.data);
            cursor = page.next_cursor;
            if cursor.is_none() {
                return Ok(threads);
            }
        }
        Err(CommandError::new("codexUnavailable", "Codex 项目分页过多"))
    })
    .await?;
    Ok(summarize_codex_projects(threads))
}

fn summarize_codex_projects(threads: Vec<ThreadSummary>) -> Vec<CodexProject> {
    let mut projects = HashMap::<String, CodexProject>::new();
    let mut seen_threads = HashSet::new();
    for thread in threads {
        if !seen_threads.insert(thread.id.clone()) {
            continue;
        }
        let path = PathBuf::from(thread.cwd.trim());
        if !path.is_dir() {
            continue;
        }
        let path = path.canonicalize().unwrap_or(path);
        let path_string = path.to_string_lossy().into_owned();
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|name| !name.is_empty())
            .unwrap_or(&path_string)
            .to_owned();
        let project = projects.entry(path_string.clone()).or_insert(CodexProject {
            path: path_string,
            name,
            thread_count: 0,
            updated_at: thread.updated_at,
        });
        project.thread_count += 1;
        project.updated_at = project.updated_at.max(thread.updated_at);
    }
    let mut projects = projects.into_values().collect::<Vec<_>>();
    projects.sort_by(|left, right| {
        right
            .updated_at
            .cmp(&left.updated_at)
            .then_with(|| left.name.cmp(&right.name))
    });
    projects
}

#[tauri::command]
pub async fn list_turns(
    state: State<'_, AppState>,
    thread_id: String,
) -> Result<Vec<crate::codex::models::TurnSummary>, CommandError> {
    run_codex_command(&state, move |client| {
        client.list_turns(&thread_id).map_err(CommandError::from)
    })
    .await
}

fn prepare_preview_data(
    thread_id: &str,
    cutoff_turn_id: Option<&str>,
    recorded: RecordedThread,
    codex_home: &std::path::Path,
) -> Result<(SnapshotV1, PreviewTimelineV1, Vec<u8>), CommandError> {
    if recorded.data.thread_id != thread_id {
        return Err(CommandError::new(
            "invalidSnapshot",
            "读取到的会话与分享请求不匹配，请重新生成预览",
        ));
    }
    let snapshot = convert_codex_items(recorded.data.clone(), cutoff_turn_id.map(str::to_owned))
        .map_err(|error| CommandError::new("invalidSnapshot", error.to_string()))?;
    let timeline = convert_preview_timeline(&recorded.data, cutoff_turn_id)
        .map_err(|error| CommandError::new("invalidSnapshot", error.to_string()))?;
    if snapshot.metadata.cutoff_turn_id != timeline.cutoff_turn_id {
        return Err(CommandError::new(
            "invalidSnapshot",
            "预览截止轮次不一致，请重新生成",
        ));
    }
    validate_local_snapshot(&snapshot)
        .map_err(|error| CommandError::new("invalidSnapshot", error.to_string()))?;
    let rollout_path = recorded.rollout_path.ok_or_else(|| {
        CommandError::new(
            "invalidNativeBundle",
            "Codex 未返回会话 rollout 文件路径，当前版本可能不支持原生分享",
        )
    })?;
    let native_bundle =
        native::build_bundle(codex_home, std::path::Path::new(&rollout_path), &snapshot)?;
    Ok((snapshot, timeline, native_bundle))
}

#[tauri::command]
pub async fn generate_preview(
    state: State<'_, AppState>,
    request: PreviewSnapshotRequest,
) -> Result<GeneratedPreview, CommandError> {
    request.validate()?;
    let read_thread_id = request.thread_id.clone();
    let recorded = run_codex_command(&state, move |client| {
        client
            .read_thread_data(&read_thread_id)
            .map_err(CommandError::from)
    })
    .await?;
    let codex_home = native::codex_home()?;
    let thread_id = request.thread_id.clone();
    let cutoff = request.cutoff_turn_id.clone();
    let (snapshot, timeline, native_bundle) = tokio::task::spawn_blocking(move || {
        prepare_preview_data(&thread_id, cutoff.as_deref(), recorded, &codex_home)
    })
    .await
    .map_err(|error| CommandError::new("invalidNativeBundle", error.to_string()))??;
    state
        .previews
        .insert(&request, snapshot, timeline, native_bundle)?;
    state.previews.display_for_preview(&request.preview_id)
}

#[tauri::command]
pub fn cancel_preview(state: State<'_, AppState>, preview_id: String) -> Result<(), CommandError> {
    state.previews.cancel(&preview_id);
    Ok(())
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
    pub preview_id: String,
    pub bind_ip: String,
}

#[tauri::command]
pub async fn start_share(
    state: State<'_, AppState>,
    request: StartShareRequest,
) -> Result<ShareSession, CommandError> {
    if request.thread_id.trim().is_empty() {
        return Err(CommandError::new("threadNotFound", "请选择要分享的会话"));
    }
    let (snapshot, native_bundle) = state
        .previews
        .snapshot_for_share(&request.thread_id, &request.preview_id)?;
    let ip = request.bind_ip.parse().map_err(|error| {
        CommandError::new("invalidEndpoint", format!("局域网地址无效：{error}"))
    })?;
    let invitation = state.invitations.create_with_options(
        request.thread_id.clone(),
        ip,
        0,
        crate::pairing::DEFAULT_TTL_SECONDS,
    );
    let handle = TransferServer::serve(
        invitation.clone(),
        snapshot,
        native_bundle,
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
pub fn share_status(
    state: State<'_, AppState>,
    invitation_id: String,
) -> Result<String, CommandError> {
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
    pub import_service: Arc<ImportService>,
    pub runtime: Runtime,
}

impl Default for ReceiveState {
    fn default() -> Self {
        Self {
            clients: Mutex::new(HashMap::new()),
            transfers: Mutex::new(HashMap::new()),
            staged: Mutex::new(HashMap::new()),
            progress: Mutex::new(Vec::new()),
            import_service: Arc::new(ImportService::default()),
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
        serde_json::from_str(&request.pairing_info).map_err(|error| {
            CommandError::new("invalidPairing", format!("配对信息无效：{error}"))
        })?;
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
    state.clients.lock().expect("接收客户端锁已中毒").insert(
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
pub async fn decline_share(
    state: State<'_, ReceiveState>,
    share_id: String,
) -> Result<(), CommandError> {
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
    let (bundle_bytes, progress_events) = result
        .map_err(|error| CommandError::new("transferFailed", error.to_string()))?
        .map_err(|error| CommandError::new("transferFailed", error.to_string()))?;
    native::parse_bundle(&bundle_bytes)
        .map_err(|error| CommandError::new("invalidNativeBundle", error.to_string()))?;
    let path = staging_path(&window, &share_id)?;
    fs::write(&path, &bundle_bytes).map_err(|error| {
        CommandError::new("stagingFailed", format!("暂存原生会话包失败：{error}"))
    })?;
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
    let digest = staged_snapshot_digest(share_id);
    let directory = window
        .path()
        .app_data_dir()
        .map_err(|error| {
            CommandError::new("stagingFailed", format!("无法定位应用数据目录：{error}"))
        })?
        .join("staged-snapshots");
    fs::create_dir_all(&directory).map_err(|error| {
        CommandError::new("stagingFailed", format!("创建暂存目录失败：{error}"))
    })?;
    Ok(directory.join(format!("{digest}.agbridge")))
}

#[tauri::command]
pub fn cancel_transfer(
    state: State<'_, ReceiveState>,
    share_id: String,
) -> Result<(), CommandError> {
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
    #[serde(default)]
    pub session_name: Option<String>,
    #[serde(default)]
    pub conflict_policy: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImportConflictReport {
    pub files: Vec<String>,
}

#[tauri::command]
pub fn import_snapshot(
    window: WebviewWindow<impl TauriRuntime>,
    state: State<'_, ReceiveState>,
    request: ImportRequest,
) -> Result<ImportRecord, CommandError> {
    let (app_data_dir, snapshot_path, bytes) =
        read_staged_bundle(&window, &state, &request.share_id)?;
    let project = std::path::Path::new(&request.project_path);
    let database_path = app_data_dir.join("imports.db");
    let database = ImportDatabase::open(database_path)
        .map_err(|error| CommandError::new("databaseFailed", error.to_string()))?;
    let conflict_policy = parse_import_conflict_policy(request.conflict_policy.as_deref())?;
    state
        .import_service
        .create_local_thread_with_options(
            &bytes,
            project,
            &snapshot_path,
            &database,
            request.session_name.as_deref(),
            conflict_policy,
        )
        .map_err(map_import_error)
}

#[tauri::command]
pub fn inspect_import_conflicts(
    window: WebviewWindow<impl TauriRuntime>,
    state: State<'_, ReceiveState>,
    share_id: String,
) -> Result<ImportConflictReport, CommandError> {
    let (_, _, bytes) = read_staged_bundle(&window, &state, &share_id)?;
    let bundle = native::parse_bundle(&bytes)?;
    let codex_home = native::codex_home()?;
    let files = native::inspect_bundle_conflicts(&codex_home, &bundle)?;
    Ok(ImportConflictReport { files })
}

fn read_staged_bundle<R: TauriRuntime>(
    window: &WebviewWindow<R>,
    state: &ReceiveState,
    share_id: &str,
) -> Result<(PathBuf, PathBuf, Vec<u8>), CommandError> {
    let staged = state
        .staged
        .lock()
        .expect("暂存锁已中毒")
        .get(share_id)
        .cloned()
        .ok_or_else(|| CommandError::new("notFound", "找不到本机暂存快照，请重新接收分享"))?;
    let snapshot_path = PathBuf::from(&staged.snapshot_path);
    let bytes = fs::read(&snapshot_path).map_err(|error| {
        CommandError::new(
            "invalidNativeBundle",
            format!("读取暂存原生会话包失败：{error}"),
        )
    })?;
    native::parse_bundle(&bytes)?;
    let app_data_dir = window.path().app_data_dir().map_err(|error| {
        CommandError::new("databaseFailed", format!("无法定位应用数据目录：{error}"))
    })?;
    validate_staged_snapshot_path(&app_data_dir, &snapshot_path, share_id)?;
    Ok((app_data_dir, snapshot_path, bytes))
}

fn parse_import_conflict_policy(
    value: Option<&str>,
) -> Result<native::InstallConflictPolicy, CommandError> {
    match value.unwrap_or("preserveLocal") {
        "preserveLocal" => Ok(native::InstallConflictPolicy::PreserveLocal),
        "replaceLocal" => Ok(native::InstallConflictPolicy::ReplaceLocal),
        _ => Err(CommandError::new(
            "invalidConflictPolicy",
            "会话冲突处理方式无效，请重新选择",
        )),
    }
}

#[tauri::command]
pub fn list_imports(
    window: WebviewWindow<impl TauriRuntime>,
) -> Result<Vec<ImportRecord>, CommandError> {
    let database_path = window
        .path()
        .app_data_dir()
        .map_err(|error| CommandError::new("databaseFailed", error.to_string()))?
        .join("imports.db");
    ImportDatabase::open(database_path)
        .and_then(|database| database.list())
        .map_err(|error| CommandError::new("databaseFailed", error.to_string()))
}

#[tauri::command]
pub fn retry_import(
    window: WebviewWindow<impl TauriRuntime>,
    state: State<'_, ReceiveState>,
    import_id: String,
) -> Result<ImportRecord, CommandError> {
    let database = open_database(&window)?;
    let app_data_dir = window
        .path()
        .app_data_dir()
        .map_err(|error| CommandError::new("databaseFailed", error.to_string()))?;
    retry_import_record(
        &state.import_service,
        &database,
        &app_data_dir,
        &import_id,
        crate::pairing::invitation::unix_seconds(),
    )
}

pub fn retry_import_record(
    service: &ImportService,
    database: &ImportDatabase,
    app_data_dir: &std::path::Path,
    import_id: &str,
    retried_at: i64,
) -> Result<ImportRecord, CommandError> {
    let previous = database
        .get(import_id)
        .map_err(|error| CommandError::new("databaseFailed", error.to_string()))?
        .ok_or_else(|| CommandError::new("notFound", "找不到导入记录"))?;
    if previous.status == "completed" {
        return Err(CommandError::new(
            "notRetryable",
            "该导入已完成，不能重复创建 Codex 会话",
        ));
    }
    let snapshot_path = std::path::PathBuf::from(&previous.snapshot_path);
    validate_staged_snapshot_path(app_data_dir, &snapshot_path, &previous.id)?;
    let bytes = fs::read(&snapshot_path).map_err(|error| {
        CommandError::new(
            "invalidNativeBundle",
            format!("读取暂存原生会话包失败：{error}"),
        )
    })?;
    native::parse_bundle(&bytes)?;
    database
        .mark_retried(import_id, retried_at)
        .map_err(|error| CommandError::new("databaseFailed", error.to_string()))?;
    let session_name =
        (!previous.session_name.trim().is_empty()).then_some(previous.session_name.as_str());
    let mut record = service
        .create_local_thread_with_options(
            &bytes,
            std::path::Path::new(&previous.project_path),
            &snapshot_path,
            database,
            session_name,
            native::InstallConflictPolicy::PreserveLocal,
        )
        .map_err(map_import_error)?;
    record.id = import_id.to_owned();
    record.retry_count = previous.retry_count + 1;
    record.last_retried_at = Some(retried_at);
    database
        .upsert(&record)
        .map_err(|error| CommandError::new("databaseFailed", error.to_string()))?;
    Ok(record)
}

pub fn delete_import_files<F: FnOnce(&str)>(
    database: &ImportDatabase,
    app_data_dir: &std::path::Path,
    import_id: &str,
    remove_staged: F,
) -> Result<(), CommandError> {
    let record = database
        .get(import_id)
        .map_err(|error| CommandError::new("databaseFailed", error.to_string()))?;
    database
        .delete(import_id)
        .map_err(|error| CommandError::new("databaseFailed", error.to_string()))?;
    let Some(record) = record else {
        return Ok(());
    };
    if record.snapshot_path.is_empty() {
        return Ok(());
    }
    remove_staged(&record.snapshot_path);
    let staged_root = app_data_dir.join("staged-snapshots");
    let snapshot = std::path::Path::new(&record.snapshot_path);
    let canonical_staged_root = staged_root.canonicalize().ok();
    let canonical_snapshot = snapshot.canonicalize().ok();
    let inside_staged_root = match (canonical_staged_root, canonical_snapshot) {
        (Some(root), Some(path)) => path.starts_with(&root) && path != root,
        _ => false,
    };
    if inside_staged_root {
        let _ = fs::remove_file(snapshot);
    }
    Ok(())
}

fn validate_staged_snapshot_path(
    app_data_dir: &std::path::Path,
    snapshot_path: &std::path::Path,
    share_id: &str,
) -> Result<(), CommandError> {
    let staged_root = app_data_dir.join("staged-snapshots");
    let filename = snapshot_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    let canonical_root = staged_root.canonicalize().ok();
    let canonical_snapshot = snapshot_path.canonicalize().ok();
    let expected_name = format!("{}.agbridge", staged_snapshot_digest(share_id));
    if filename.is_empty()
        || filename != expected_name
        || !matches!((canonical_root, canonical_snapshot), (Some(root), Some(path)) if path.starts_with(&root) && path != root)
    {
        return Err(CommandError::new(
            "invalidSnapshot",
            "暂存快照路径无效，请重新接收分享",
        ));
    }
    Ok(())
}

pub fn validate_staged_snapshot_path_for_test(
    app_data_dir: &std::path::Path,
    snapshot_path: &std::path::Path,
) -> Result<(), CommandError> {
    validate_staged_snapshot_path(app_data_dir, snapshot_path, "share-1")
}

pub fn staged_snapshot_digest(share_id: &str) -> String {
    hex::encode(Sha256::digest(share_id.as_bytes()))
}

#[tauri::command]
pub fn open_import(
    window: WebviewWindow<impl TauriRuntime>,
    state: State<'_, ReceiveState>,
    import_id: String,
) -> Result<(), CommandError> {
    let database = open_database(&window)?;
    let record = database
        .get(&import_id)
        .map_err(|error| CommandError::new("databaseFailed", error.to_string()))?
        .ok_or_else(|| CommandError::new("notFound", "找不到导入记录"))?;
    if record.status != "completed" || record.local_thread_id.is_empty() {
        return Err(CommandError::new(
            "notCompleted",
            "该导入尚未创建本机会话，请重试导入",
        ));
    }
    if !state.import_service.thread_exists(&record.local_thread_id) {
        return Err(CommandError::new(
            "threadMissing",
            "本机 Codex 会话尚未出现在公开会话列表中，请重启 Codex 后重试打开",
        ));
    }
    let opener = crate::imports::history::CodexOpener::default();
    opener
        .open_thread(&record.local_thread_id)
        .map_err(|error| CommandError::new("openUnsupported", error.to_string()))
}

#[tauri::command]
pub fn copy_continuation_prompt(
    window: WebviewWindow<impl TauriRuntime>,
    import_id: String,
) -> Result<String, CommandError> {
    let database = open_database(&window)?;
    database
        .get(&import_id)
        .map_err(|error| CommandError::new("databaseFailed", error.to_string()))?
        .and_then(|record| record.continuation_prompt)
        .ok_or_else(|| CommandError::new("notFound", "找不到保留的续接提示"))
}

#[tauri::command]
pub fn delete_import(
    window: WebviewWindow<impl TauriRuntime>,
    state: State<'_, ReceiveState>,
    import_id: String,
) -> Result<(), CommandError> {
    let app_data_dir = window
        .path()
        .app_data_dir()
        .map_err(|error| CommandError::new("databaseFailed", error.to_string()))?;
    let database = ImportDatabase::open(app_data_dir.join("imports.db"))
        .map_err(|error| CommandError::new("databaseFailed", error.to_string()))?;
    delete_import_files(&database, &app_data_dir, &import_id, |snapshot_path| {
        state
            .staged
            .lock()
            .expect("暂存锁已中毒")
            .retain(|_, staged| staged.snapshot_path != snapshot_path);
    })
}

fn open_database(
    window: &WebviewWindow<impl TauriRuntime>,
) -> Result<ImportDatabase, CommandError> {
    let database_path = window
        .path()
        .app_data_dir()
        .map_err(|error| CommandError::new("databaseFailed", error.to_string()))?
        .join("imports.db");
    ImportDatabase::open(database_path)
        .map_err(|error| CommandError::new("databaseFailed", error.to_string()))
}

#[tauri::command]
pub async fn select_project_directory(
    window: WebviewWindow<impl TauriRuntime>,
) -> Result<Option<String>, CommandError> {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    window
        .dialog()
        .file()
        .set_title("选择接收方本机项目目录")
        .pick_folder(move |selected| {
            let _ = sender.send(
                selected
                    .and_then(|path| path.into_path().ok())
                    .map(|path| path.to_string_lossy().into_owned()),
            );
        });
    receiver
        .await
        .map_err(|_| CommandError::new("dialogFailed", "目录选择器未返回结果"))
}

#[tauri::command]
pub async fn select_project_parent(
    window: WebviewWindow<impl TauriRuntime>,
) -> Result<Option<String>, CommandError> {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    window
        .dialog()
        .file()
        .set_title("选择新项目的父目录")
        .pick_folder(move |selected| {
            let _ = sender.send(
                selected
                    .and_then(|path| path.into_path().ok())
                    .map(|path| path.to_string_lossy().into_owned()),
            );
        });
    receiver
        .await
        .map_err(|_| CommandError::new("dialogFailed", "目录选择器未返回结果"))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateProjectRequest {
    pub parent_path: String,
    pub project_name: String,
}

#[tauri::command]
pub fn create_project_directory(request: CreateProjectRequest) -> Result<String, CommandError> {
    create_project_directory_at(
        Path::new(request.parent_path.trim()),
        request.project_name.trim(),
    )
    .map(|path| path.to_string_lossy().into_owned())
}

fn create_project_directory_at(parent: &Path, project_name: &str) -> Result<PathBuf, CommandError> {
    if !parent.is_dir() {
        return Err(CommandError::new(
            "invalidProject",
            "请选择一个存在的父目录",
        ));
    }
    if project_name.is_empty() {
        return Err(CommandError::new("invalidProject", "项目名称不能为空"));
    }
    if project_name.chars().count() > 80 {
        return Err(CommandError::new(
            "invalidProject",
            "项目名称最多允许 80 个字符",
        ));
    }
    if project_name.chars().any(char::is_control) {
        return Err(CommandError::new(
            "invalidProject",
            "项目名称不能包含控制字符",
        ));
    }
    let mut components = Path::new(project_name).components();
    let valid_component = matches!(
        components.next(),
        Some(Component::Normal(value)) if value == std::ffi::OsStr::new(project_name)
    );
    if !valid_component || components.next().is_some() {
        return Err(CommandError::new(
            "invalidProject",
            "项目名称不能包含路径分隔符",
        ));
    }
    let target = parent.join(project_name);
    if target.exists() {
        return Err(CommandError::new(
            "projectExists",
            "同名项目目录已存在，请从 Codex 已有项目中直接选择",
        ));
    }
    fs::create_dir(&target).map_err(|error| {
        CommandError::new("invalidProject", format!("无法创建项目目录：{error}"))
    })?;
    Ok(target)
}

fn map_import_error(error: ImportError) -> CommandError {
    let code = match &error {
        ImportError::InvalidProject(_) => "invalidProject",
        ImportError::InvalidName(_) => "invalidName",
        ImportError::PromptTooLarge | ImportError::Snapshot(_) => "invalidSnapshot",
        ImportError::Native(_) => "invalidNativeBundle",
        ImportError::Codex(_) => "codexUnavailable",
        ImportError::Store(_) => "databaseFailed",
    };
    CommandError::new(code, error.to_string())
}

#[cfg(test)]
mod local_address_tests {
    use super::*;
    use crate::codex::models::ThreadSummary;
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    fn unique_temp_dir(label: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("系统时间有效")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "agentbridge-command-{label}-{}-{nanos}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("创建测试目录");
        path
    }

    #[test]
    fn physical_lan_interfaces_are_preferred_over_virtual_adapters() {
        let addresses = sort_local_addresses(vec![
            LocalAddress {
                ip: "172.21.192.1".into(),
                name: "vEthernet (WSL)".into(),
            },
            LocalAddress {
                ip: "192.168.1.20".into(),
                name: "Wi-Fi".into(),
            },
        ]);

        assert_eq!(addresses[0].name, "Wi-Fi");
        assert_eq!(addresses[1].name, "vEthernet (WSL)");
    }

    #[test]
    fn codex_threads_are_grouped_into_existing_local_projects() {
        let root = unique_temp_dir("projects");
        let alpha = root.join("alpha");
        let beta = root.join("beta");
        fs::create_dir_all(&alpha).expect("创建 alpha");
        fs::create_dir_all(&beta).expect("创建 beta");
        let threads = vec![
            ThreadSummary {
                id: "thread-1".into(),
                title: Some("alpha one".into()),
                preview: String::new(),
                cwd: alpha.to_string_lossy().into_owned(),
                created_at: 1,
                updated_at: 10,
                repository_id: None,
            },
            ThreadSummary {
                id: "thread-2".into(),
                title: Some("alpha two".into()),
                preview: String::new(),
                cwd: alpha.to_string_lossy().into_owned(),
                created_at: 2,
                updated_at: 30,
                repository_id: None,
            },
            ThreadSummary {
                id: "thread-3".into(),
                title: Some("beta".into()),
                preview: String::new(),
                cwd: beta.to_string_lossy().into_owned(),
                created_at: 3,
                updated_at: 20,
                repository_id: None,
            },
        ];

        let projects = summarize_codex_projects(threads);

        assert_eq!(projects.len(), 2);
        assert_eq!(projects[0].name, "alpha");
        assert_eq!(projects[0].thread_count, 2);
        assert_eq!(projects[0].updated_at, 30);
        assert_eq!(
            projects[0].path,
            alpha
                .canonicalize()
                .expect("规范化 alpha")
                .to_string_lossy()
        );
        assert_eq!(projects[1].name, "beta");
        fs::remove_dir_all(root).expect("清理测试目录");
    }

    #[test]
    fn creates_a_new_project_directory_under_an_existing_parent() {
        let root = unique_temp_dir("new-project");

        let created = create_project_directory_at(&root, "共享项目").expect("应创建新项目目录");

        assert_eq!(created, root.join("共享项目"));
        assert!(created.is_dir());
        assert!(
            create_project_directory_at(&root, "共享项目")
                .expect_err("同名目录不能静默覆盖")
                .message
                .contains("已存在")
        );
        fs::remove_dir_all(root).expect("清理测试目录");
    }

    #[test]
    fn import_conflict_policy_defaults_to_preserving_local_files() {
        assert_eq!(
            parse_import_conflict_policy(None).expect("默认策略"),
            native::InstallConflictPolicy::PreserveLocal
        );
        assert_eq!(
            parse_import_conflict_policy(Some("replaceLocal")).expect("覆盖策略"),
            native::InstallConflictPolicy::ReplaceLocal
        );
        assert_eq!(
            parse_import_conflict_policy(Some("unexpected"))
                .expect_err("未知策略必须拒绝")
                .code,
            "invalidConflictPolicy"
        );
    }
}

#[cfg(test)]
mod codex_version_tests {
    use super::*;

    #[test]
    fn reports_exit_code_and_stderr_when_codex_fails() {
        let message = codex_version_failure_message(Some(127), "", "env: node: No such file");

        assert!(message.contains("退出码 127"));
        assert!(message.contains("env: node: No such file"));
    }
}
