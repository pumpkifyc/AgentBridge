use super::{
    error::CodexError,
    executable::{configure_codex_command, resolve_codex_executable},
    models::{Page, ThreadSummary, TurnSummary},
    protocol::{RpcRequest, RpcResponse},
};
use crate::snapshot::{
    convert::{CodexItem, ThreadData, ThreadTurn, convert_codex_items},
    schema::SnapshotV1,
    validate::SnapshotError,
};
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, BufWriter, Write},
    process::{Child, ChildStdin, Command, Stdio},
    sync::{Arc, Mutex, mpsc},
    thread,
    time::{Duration, Instant},
};

const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(20);
const MAX_NOTIFICATIONS_PER_REQUEST: usize = 10_000;
const TURN_PAGE_SIZE: u32 = 50;
const ITEM_PAGE_SIZE: u32 = 50;
const MAX_TURN_PAGES: usize = 200;
const MAX_ITEM_PAGES: usize = 200;

pub struct CodexClient {
    child: Child,
    writer: Arc<Mutex<BufWriter<ChildStdin>>>,
    responses: mpsc::Receiver<Result<RpcResponse, String>>,
    notifications: mpsc::Receiver<Value>,
    next_id: u64,
    timeout: Duration,
    diagnostics: Arc<Mutex<Vec<u8>>>,
    cancelled: bool,
}

impl CodexClient {
    pub fn connect() -> Result<Self, CodexError> {
        Self::connect_with_executable(resolve_codex_executable()?)
    }

    pub fn connect_with_executable(
        executable: impl Into<std::ffi::OsString>,
    ) -> Result<Self, CodexError> {
        Self::connect_with_timeout(executable, DEFAULT_REQUEST_TIMEOUT)
    }

    pub fn connect_with_timeout(
        executable: impl Into<std::ffi::OsString>,
        timeout: Duration,
    ) -> Result<Self, CodexError> {
        let mut command = Command::new(executable.into());
        configure_codex_command(&mut command);
        let mut child = command
            .arg("app-server")
            .arg("--stdio")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| {
                if error.kind() == std::io::ErrorKind::NotFound {
                    CodexError::NotInstalled(error.to_string())
                } else {
                    CodexError::Spawn(error.to_string())
                }
            })?;

        let stdin = child.stdin.take().ok_or(CodexError::ProcessExited)?;
        let stdout = child.stdout.take().ok_or(CodexError::ProcessExited)?;
        let stderr = child.stderr.take().ok_or(CodexError::ProcessExited)?;
        let diagnostics = Arc::new(Mutex::new(Vec::new()));
        let diagnostic_buffer = Arc::clone(&diagnostics);
        thread::spawn(move || {
            let mut reader = BufReader::new(stderr);
            let mut buffer = [0u8; 4096];
            let mut total = 0usize;
            loop {
                match std::io::Read::read(&mut reader, &mut buffer) {
                    Ok(0) | Err(_) => break,
                    Ok(count) => {
                        if total < 64 * 1024 {
                            let keep = count.min(64 * 1024 - total);
                            if let Ok(mut saved) = diagnostic_buffer.lock() {
                                saved.extend_from_slice(&buffer[..keep]);
                            }
                        }
                        total = total.saturating_add(count);
                    }
                }
            }
        });

        let (sender, responses) = mpsc::channel();
        let (notification_sender, notifications) = mpsc::channel();
        thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                let line = match line {
                    Ok(line) => line,
                    Err(error) => {
                        let _ = sender.send(Err(error.to_string()));
                        break;
                    }
                };
                let parsed = match serde_json::from_str::<Value>(&line) {
                    Ok(value) => value,
                    Err(error) => {
                        let _ = sender.send(Err(error.to_string()));
                        continue;
                    }
                };
                if parsed.get("id").is_some() {
                    let response = serde_json::from_value::<RpcResponse>(parsed)
                        .map_err(|error| error.to_string());
                    if sender.send(response).is_err() {
                        break;
                    }
                } else if notification_sender.send(parsed).is_err() {
                    break;
                }
            }
        });

        let mut client = Self {
            child,
            writer: Arc::new(Mutex::new(BufWriter::new(stdin))),
            responses,
            notifications,
            next_id: 1,
            timeout,
            diagnostics,
            cancelled: false,
        };
        client.request(
            "initialize",
            json!({
                "clientInfo": {
                    "name": "agentbridge",
                    "title": "AgentBridge",
                    "version": env!("CARGO_PKG_VERSION")
                },
                "capabilities": {"experimentalApi": true}
            }),
        )?;
        client.notify("initialized", json!({}))?;
        Ok(client)
    }

    fn notify(&self, method: &str, params: Value) -> Result<(), CodexError> {
        self.write_line(&json!({"method": method, "params": params}))
    }

    fn write_line(&self, value: &Value) -> Result<(), CodexError> {
        let mut writer = self
            .writer
            .lock()
            .map_err(|_| CodexError::Protocol("写入锁已中毒".into()))?;
        serde_json::to_writer(&mut *writer, value)
            .map_err(|error| CodexError::Protocol(error.to_string()))?;
        writer
            .write_all(b"\n")
            .and_then(|_| writer.flush())
            .map_err(|error| CodexError::Spawn(error.to_string()))
    }

    fn request(&mut self, method: &str, params: Value) -> Result<Value, CodexError> {
        if self.cancelled {
            return Err(CodexError::ProcessExited);
        }
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        let request = serde_json::to_value(RpcRequest { id, method, params })
            .map_err(|error| CodexError::Protocol(error.to_string()))?;
        self.write_line(&request)?;

        for _ in 0..MAX_NOTIFICATIONS_PER_REQUEST {
            let response = self
                .responses
                .recv_timeout(self.timeout)
                .map_err(|error| match error {
                    mpsc::RecvTimeoutError::Timeout => CodexError::Timeout,
                    mpsc::RecvTimeoutError::Disconnected => CodexError::ProcessExited,
                })?
                .map_err(CodexError::Protocol)?;
            if response.id != Some(id) {
                continue;
            }
            if let Some(error) = response.error {
                return Err(CodexError::Rpc {
                    code: error.code,
                    message: error.message,
                });
            }
            return response
                .result
                .ok_or_else(|| CodexError::Protocol("响应缺少 result".into()));
        }
        Err(CodexError::Timeout)
    }

    pub fn diagnostics(&self) -> Vec<u8> {
        self.diagnostics
            .lock()
            .map(|diagnostics| diagnostics.clone())
            .unwrap_or_default()
    }

    pub fn cancel(&mut self) {
        self.cancelled = true;
        terminate_process_tree(&mut self.child);
    }

    pub fn drain_notifications(&self) -> usize {
        let mut count = 0;
        while self.notifications.try_recv().is_ok() {
            count += 1;
        }
        count
    }

    pub fn list_threads(
        &mut self,
        cursor: Option<String>,
    ) -> Result<Page<ThreadSummary>, CodexError> {
        let response = self.request(
            "thread/list",
            json!({
                "cursor": cursor,
                "limit": 100,
                "sortDirection": "desc",
                "sortKey": "updated_at",
                "archived": false,
                "sourceKinds": ["cli", "vscode", "exec", "appServer", "unknown"]
            }),
        )?;
        let page: ThreadListResponse = serde_json::from_value(response)
            .map_err(|error| CodexError::Protocol(error.to_string()))?;
        Ok(Page {
            data: page.data.into_iter().map(ThreadSummary::from).collect(),
            next_cursor: page.next_cursor,
        })
    }

    pub fn list_turns(&mut self, thread_id: &str) -> Result<Vec<TurnSummary>, CodexError> {
        let mut all_turns = Vec::new();
        let mut cursor: Option<String> = None;
        for _ in 0..MAX_TURN_PAGES {
            let response = self.request(
                "thread/turns/list",
                json!({
                    "threadId": thread_id,
                    "cursor": cursor,
                    "limit": TURN_PAGE_SIZE,
                    "sortDirection": "asc",
                    "itemsView": "notLoaded"
                }),
            )?;
            let page: TurnsPage = serde_json::from_value(response)
                .map_err(|error| CodexError::Protocol(error.to_string()))?;
            all_turns.extend(page.data.into_iter().map(TurnSummary::from));
            cursor = page.next_cursor;
            if cursor.is_none() {
                return Ok(all_turns);
            }
        }
        Err(CodexError::Protocol("会话轮次分页过多".into()))
    }

    pub fn read_snapshot(
        &mut self,
        thread_id: &str,
        cutoff_turn_id: Option<&str>,
    ) -> Result<SnapshotV1, CodexError> {
        let thread_response = self.request(
            "thread/read",
            json!({"threadId": thread_id, "includeTurns": false}),
        )?;
        let thread: ThreadReadResponse = serde_json::from_value(thread_response)
            .map_err(|error| CodexError::Protocol(error.to_string()))?;
        let thread = thread
            .thread
            .ok_or_else(|| CodexError::ThreadNotFound(thread_id.into()))?;

        let mut turns = self.list_turns(thread_id)?;
        if let Some(cutoff) = cutoff_turn_id
            && !turns.iter().any(|turn| turn.id == cutoff && turn.completed)
        {
            return Err(CodexError::Snapshot(SnapshotError::InvalidCutoff));
        }
        let mut items_by_turn: std::collections::HashMap<String, Vec<CodexItem>> =
            std::collections::HashMap::new();
        let mut item_cursor: Option<String> = None;
        for page_number in 0..MAX_ITEM_PAGES {
            let response = self.request(
                "thread/items/list",
                json!({
                    "threadId": thread_id,
                    "cursor": item_cursor,
                    "limit": ITEM_PAGE_SIZE,
                    "sortDirection": "asc"
                }),
            )?;
            let page: ThreadItemsPage = serde_json::from_value(response)
                .map_err(|error| CodexError::Protocol(error.to_string()))?;
            for entry in page.data {
                let timestamp = entry
                    .started_at_ms
                    .map(|millis| millis / 1_000)
                    .or_else(|| entry.completed_at_ms.map(|millis| millis / 1_000))
                    .or(entry.started_at)
                    .or(entry.completed_at);
                items_by_turn
                    .entry(entry.turn_id)
                    .or_default()
                    .extend(convert_item(entry.item, timestamp));
            }
            item_cursor = page.next_cursor;
            if item_cursor.is_none() {
                break;
            }
            if page_number + 1 == MAX_ITEM_PAGES {
                return Err(CodexError::Protocol("会话条目分页过多".into()));
            }
        }
        for turn in &mut turns {
            turn.items = items_by_turn.remove(&turn.id).unwrap_or_default();
        }

        let turns = turns
            .into_iter()
            .map(|turn| ThreadTurn {
                id: turn.id,
                completed: turn.completed,
                truncated: false,
                items: turn.items,
            })
            .collect();
        Ok(convert_codex_items(
            ThreadData {
                thread_id: thread.id,
                title: thread.name.unwrap_or_else(|| thread.preview.clone()),
                created_at: thread.created_at,
                updated_at: thread.updated_at,
                project_name: None,
                repository_id: thread.git_info.and_then(|info| info.repository_id),
                working_directory: Some(thread.cwd),
                turns,
            },
            cutoff_turn_id.map(str::to_owned),
        )?)
    }

    pub fn start_thread(&mut self, cwd: &str) -> Result<String, CodexError> {
        let response = self.request("thread/start", json!({"cwd": cwd}))?;
        let result: ThreadIdentityResponse = serde_json::from_value(response)
            .map_err(|error| CodexError::Protocol(error.to_string()))?;
        Ok(result.thread.id)
    }

    pub fn find_thread(&mut self, thread_id: &str) -> Result<Option<ThreadSummary>, CodexError> {
        let mut cursor = None;
        for _ in 0..MAX_TURN_PAGES {
            let page = self.list_threads(cursor)?;
            if let Some(thread) = page.data.into_iter().find(|thread| thread.id == thread_id) {
                return Ok(Some(thread));
            }
            cursor = page.next_cursor;
            if cursor.is_none() {
                break;
            }
        }
        Ok(None)
    }

    pub fn start_turn(&mut self, thread_id: &str, cwd: &str, text: &str) -> Result<(), CodexError> {
        let response = self.request(
            "turn/start",
            json!({
                "threadId": thread_id,
                "cwd": cwd,
                "input": [{"type": "text", "text": text}]
            }),
        )?;
        let _: TurnStartResponse = serde_json::from_value(response)
            .map_err(|error| CodexError::Protocol(error.to_string()))?;
        Ok(())
    }

    pub fn wait_for_turn_completion(&self, timeout: Duration) -> Result<(), CodexError> {
        let deadline = Instant::now() + timeout;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(CodexError::Timeout);
            }
            let notification = self
                .notifications
                .recv_timeout(remaining)
                .map_err(|error| match error {
                    mpsc::RecvTimeoutError::Timeout => CodexError::Timeout,
                    mpsc::RecvTimeoutError::Disconnected => CodexError::ProcessExited,
                })?;
            if notification.get("method").and_then(Value::as_str) != Some("turn/completed") {
                continue;
            }
            if notification["params"]["turn"]["status"].as_str() == Some("completed") {
                return Ok(());
            }
            let message = notification["params"]["turn"]["error"]["message"]
                .as_str()
                .unwrap_or("会话轮次未完成");
            return Err(CodexError::TurnFailed(message.to_owned()));
        }
    }
}

impl Drop for CodexClient {
    fn drop(&mut self) {
        terminate_process_tree(&mut self.child);
    }
}

fn terminate_process_tree(child: &mut Child) {
    #[cfg(windows)]
    {
        let mut command = Command::new("taskkill");
        crate::process::configure_quiet_command(&mut command);
        let _ = command
            .args(["/PID", &child.id().to_string(), "/T", "/F"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
    }
    let _ = child.kill();
    let _ = child.wait();
}

fn convert_item(item: ThreadItem, timestamp: Option<i64>) -> Vec<CodexItem> {
    match item {
        ThreadItem::UserMessage { content } => {
            let mut text = Vec::new();
            let mut unsupported = Vec::new();
            for part in content {
                match part.kind.as_str() {
                    "text" => text.push(part.text.unwrap_or_default()),
                    "image" | "localImage" => unsupported.push("图片附件".to_owned()),
                    other => unsupported.push(other.to_owned()),
                }
            }
            let mut converted = Vec::new();
            if !text.is_empty() {
                converted.push(CodexItem::UserText {
                    text: text.join("\n"),
                    timestamp,
                });
            }
            converted.extend(
                unsupported
                    .into_iter()
                    .map(|kind| CodexItem::Unsupported { kind }),
            );
            converted
        }
        ThreadItem::AgentMessage { text } => vec![CodexItem::AssistantText { text, timestamp }],
        ThreadItem::Plan { text } => vec![CodexItem::AssistantText { text, timestamp }],
        ThreadItem::CommandExecution { aggregated_output } => {
            vec![aggregated_output.map_or_else(
                || CodexItem::Unsupported {
                    kind: "命令执行（无可读输出）".into(),
                },
                |text| CodexItem::ToolResult {
                    name: "命令执行".into(),
                    text,
                    timestamp,
                },
            )]
        }
        ThreadItem::McpToolCall {
            server,
            tool,
            result,
        } => vec![
            CodexItem::ToolCall {
                name: format!("{server}/{tool}"),
                arguments: String::new(),
                timestamp,
            },
            result.map_or_else(
                || CodexItem::Unsupported {
                    kind: format!("{server}/{tool}（无可读取结果）"),
                },
                |value| CodexItem::ToolResult {
                    name: format!("{server}/{tool}"),
                    text: match value {
                        serde_json::Value::String(text) => text,
                        other => other.to_string(),
                    },
                    timestamp,
                },
            ),
        ],
        ThreadItem::FileChange => vec![CodexItem::Unsupported {
            kind: "文件变更".into(),
        }],
        ThreadItem::Other => vec![CodexItem::Unsupported {
            kind: "未知 Codex 历史条目".into(),
        }],
    }
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ThreadListResponse {
    data: Vec<ThreadWire>,
    next_cursor: Option<String>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ThreadWire {
    id: String,
    name: Option<String>,
    preview: String,
    cwd: String,
    created_at: i64,
    updated_at: i64,
    git_info: Option<GitInfo>,
}

impl From<ThreadWire> for ThreadSummary {
    fn from(thread: ThreadWire) -> Self {
        Self {
            id: thread.id,
            title: thread.name,
            preview: thread.preview,
            cwd: thread.cwd,
            created_at: thread.created_at,
            updated_at: thread.updated_at,
            repository_id: thread.git_info.and_then(|info| info.repository_id),
        }
    }
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct GitInfo {
    repository_id: Option<String>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ThreadReadResponse {
    thread: Option<ThreadDetails>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ThreadDetails {
    id: String,
    name: Option<String>,
    preview: String,
    cwd: String,
    created_at: i64,
    updated_at: i64,
    git_info: Option<GitInfo>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct TurnsPage {
    data: Vec<TurnWire>,
    next_cursor: Option<String>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct TurnWire {
    id: String,
    status: String,
    #[serde(default)]
    started_at: Option<i64>,
    #[serde(default)]
    completed_at: Option<i64>,
}

impl From<TurnWire> for TurnSummary {
    fn from(turn: TurnWire) -> Self {
        Self {
            id: turn.id,
            completed: turn.status == "completed",
            started_at: turn.started_at,
            completed_at: turn.completed_at,
            items: Vec::new(),
        }
    }
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ThreadItemsPage {
    data: Vec<ThreadItemEntry>,
    next_cursor: Option<String>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ThreadItemEntry {
    item: ThreadItem,
    turn_id: String,
    #[serde(default)]
    started_at: Option<i64>,
    #[serde(default)]
    completed_at: Option<i64>,
    #[serde(default)]
    started_at_ms: Option<i64>,
    #[serde(default)]
    completed_at_ms: Option<i64>,
}

#[derive(serde::Deserialize)]
#[serde(tag = "type")]
enum ThreadItem {
    #[serde(rename = "userMessage")]
    UserMessage { content: Vec<UserInput> },
    #[serde(rename = "agentMessage")]
    AgentMessage { text: String },
    #[serde(rename = "plan")]
    Plan { text: String },
    #[serde(rename = "commandExecution")]
    CommandExecution {
        #[serde(rename = "aggregatedOutput")]
        aggregated_output: Option<String>,
    },
    #[serde(rename = "mcpToolCall")]
    McpToolCall {
        server: String,
        tool: String,
        result: Option<serde_json::Value>,
    },
    #[serde(rename = "fileChange")]
    FileChange,
    #[serde(other)]
    Other,
}

#[derive(serde::Deserialize)]
struct UserInput {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    text: Option<String>,
}

#[derive(serde::Deserialize)]
struct ThreadIdentityResponse {
    thread: ThreadIdentity,
}

#[derive(serde::Deserialize)]
struct ThreadIdentity {
    id: String,
}

#[derive(serde::Deserialize)]
struct TurnStartResponse {
    #[allow(dead_code)]
    turn: Value,
}
