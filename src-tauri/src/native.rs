use crate::snapshot::schema::SnapshotV1;
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    ffi::OsStr,
    fs,
    io::{BufRead, BufReader, Cursor, Read},
    path::{Component, Path, PathBuf},
};

const MAGIC: &[u8] = b"AGBRIDGE_NATIVE_BUNDLE_V1\n";
pub const MAX_BUNDLE_BYTES: usize = 256 * 1024 * 1024;
const MAX_ROLLOUT_FILES: usize = 64;
const MAX_PREVIEW_CHARS: usize = 500;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct NativeBundleManifest {
    pub schema_version: u32,
    pub source_thread_id: String,
    pub source_session_id: String,
    pub title: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub cutoff_turn_id: Option<String>,
    pub source_working_directory: Option<String>,
    pub message_count: usize,
    pub preview: String,
    pub files: Vec<NativeRolloutFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct NativeRolloutFile {
    pub relative_path: String,
    pub size: u64,
    pub sha256: String,
}

#[derive(Debug)]
pub struct ParsedNativeBundle {
    pub manifest: NativeBundleManifest,
    pub files: Vec<Vec<u8>>,
}

#[derive(Debug, thiserror::Error)]
pub enum NativeBundleError {
    #[error("原生会话包无效：{0}")]
    Invalid(String),
    #[error("读取 Codex 会话文件失败：{0}")]
    Io(String),
    #[error("无法解析 Codex 会话文件：{0}")]
    Rollout(String),
    #[error("原生会话包超过大小限制")]
    TooLarge,
    #[error("Codex 会话文件发生冲突：{0}")]
    Conflict(String),
    #[error("更新 Codex 会话索引失败：{0}")]
    Index(String),
}

#[derive(Debug, Deserialize)]
struct RolloutLine {
    payload: RolloutPayload,
}

#[derive(Debug, Deserialize)]
struct RolloutPayload {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    session_id: Option<String>,
    #[serde(default)]
    history_base: Option<HistoryBase>,
}

#[derive(Debug, Deserialize)]
struct HistoryBase {
    thread_id: String,
}

#[derive(Debug, Clone)]
struct RolloutMeta {
    id: String,
    session_id: String,
    history_base_thread_id: Option<String>,
}

pub fn codex_home() -> Result<PathBuf, NativeBundleError> {
    if let Some(path) = std::env::var_os("CODEX_HOME")
        && !path.is_empty()
    {
        return Ok(PathBuf::from(path));
    }
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .ok_or_else(|| NativeBundleError::Io("无法定位用户主目录".into()))?;
    Ok(PathBuf::from(home).join(".codex"))
}

pub fn build_bundle(
    codex_home: &Path,
    source_rollout_path: &Path,
    snapshot: &SnapshotV1,
) -> Result<Vec<u8>, NativeBundleError> {
    let canonical_home = codex_home
        .canonicalize()
        .map_err(|error| NativeBundleError::Io(error.to_string()))?;
    let source = source_rollout_path
        .canonicalize()
        .map_err(|error| NativeBundleError::Io(error.to_string()))?;
    if !source.starts_with(&canonical_home) {
        return Err(NativeBundleError::Invalid(
            "Codex 返回的会话文件不在 CODEX_HOME 中".into(),
        ));
    }

    let candidates = rollout_candidates(&canonical_home)?;
    let mut pending = vec![source];
    let mut visited = HashSet::new();
    let mut paths = Vec::new();
    let mut source_session_id = snapshot.metadata.source_thread_id.clone();

    while let Some(path) = pending.pop() {
        if !visited.insert(path.clone()) {
            continue;
        }
        if paths.len() >= MAX_ROLLOUT_FILES {
            return Err(NativeBundleError::Invalid(
                "会话依赖的文件数量过多，无法安全打包".into(),
            ));
        }
        let meta = read_meta(&path)?;
        if paths.is_empty() {
            if meta.id != snapshot.metadata.source_thread_id {
                return Err(NativeBundleError::Rollout(format!(
                    "会话文件属于 {}，与请求的 {} 不一致",
                    meta.id, snapshot.metadata.source_thread_id
                )));
            }
            source_session_id = meta.session_id.clone();
        }
        if let Some(parent_id) = meta.history_base_thread_id {
            let parent = find_rollout_by_id(&candidates, &parent_id)?;
            pending.push(parent);
        }
        paths.push(path);
    }

    let mut files = Vec::with_capacity(paths.len());
    let mut payloads = Vec::with_capacity(paths.len());
    for path in paths {
        let bytes = fs::read(&path).map_err(|error| NativeBundleError::Io(error.to_string()))?;
        let relative_path = portable_relative(&canonical_home, &path)?;
        files.push(NativeRolloutFile {
            relative_path,
            size: bytes.len() as u64,
            sha256: sha256_hex(&bytes),
        });
        payloads.push(bytes);
    }

    let manifest = NativeBundleManifest {
        schema_version: 1,
        source_thread_id: snapshot.metadata.source_thread_id.clone(),
        source_session_id,
        title: snapshot.metadata.title.clone(),
        created_at: snapshot.metadata.created_at,
        updated_at: snapshot.metadata.updated_at,
        cutoff_turn_id: snapshot.metadata.cutoff_turn_id.clone(),
        source_working_directory: snapshot.metadata.source_working_directory.clone(),
        message_count: snapshot.messages.len(),
        preview: preview_text(snapshot),
        files,
    };
    let manifest_bytes = serde_json::to_vec(&manifest)
        .map_err(|error| NativeBundleError::Invalid(error.to_string()))?;
    let manifest_len =
        u32::try_from(manifest_bytes.len()).map_err(|_| NativeBundleError::TooLarge)?;
    let payload_len: usize = payloads.iter().map(Vec::len).sum();
    let total = MAGIC
        .len()
        .checked_add(4)
        .and_then(|value| value.checked_add(manifest_bytes.len()))
        .and_then(|value| value.checked_add(payload_len))
        .ok_or(NativeBundleError::TooLarge)?;
    if total > MAX_BUNDLE_BYTES {
        return Err(NativeBundleError::TooLarge);
    }

    let mut bundle = Vec::with_capacity(total);
    bundle.extend_from_slice(MAGIC);
    bundle.extend_from_slice(&manifest_len.to_le_bytes());
    bundle.extend_from_slice(&manifest_bytes);
    for payload in payloads {
        bundle.extend_from_slice(&payload);
    }
    Ok(bundle)
}

pub fn parse_bundle(bytes: &[u8]) -> Result<ParsedNativeBundle, NativeBundleError> {
    if bytes.len() > MAX_BUNDLE_BYTES {
        return Err(NativeBundleError::TooLarge);
    }
    if !bytes.starts_with(MAGIC) {
        return Err(NativeBundleError::Invalid("文件标识不匹配".into()));
    }
    let mut cursor = Cursor::new(&bytes[MAGIC.len()..]);
    let mut length = [0u8; 4];
    cursor
        .read_exact(&mut length)
        .map_err(|error| NativeBundleError::Invalid(error.to_string()))?;
    let manifest_len = u32::from_le_bytes(length) as usize;
    let manifest_start = MAGIC.len() + 4;
    let manifest_end = manifest_start
        .checked_add(manifest_len)
        .ok_or(NativeBundleError::TooLarge)?;
    if manifest_end > bytes.len() {
        return Err(NativeBundleError::Invalid("清单长度超出文件范围".into()));
    }
    let manifest: NativeBundleManifest =
        serde_json::from_slice(&bytes[manifest_start..manifest_end])
            .map_err(|error| NativeBundleError::Invalid(error.to_string()))?;
    if manifest.schema_version != 1 {
        return Err(NativeBundleError::Invalid(format!(
            "不支持的原生会话包版本 {}",
            manifest.schema_version
        )));
    }
    if manifest.files.is_empty() || manifest.files.len() > MAX_ROLLOUT_FILES {
        return Err(NativeBundleError::Invalid("会话文件数量无效".into()));
    }

    let mut offset = manifest_end;
    let mut files = Vec::with_capacity(manifest.files.len());
    let mut paths = HashSet::new();
    let mut found_source_thread = false;
    for file in &manifest.files {
        validate_relative_path(&file.relative_path)?;
        if !paths.insert(file.relative_path.clone()) {
            return Err(NativeBundleError::Invalid(format!(
                "会话文件路径重复：{}",
                file.relative_path
            )));
        }
        let size = usize::try_from(file.size)
            .map_err(|_| NativeBundleError::Invalid("会话文件大小无效".into()))?;
        let end = offset
            .checked_add(size)
            .ok_or(NativeBundleError::TooLarge)?;
        if end > bytes.len() {
            return Err(NativeBundleError::Invalid("会话文件数据不完整".into()));
        }
        let payload = bytes[offset..end].to_vec();
        if sha256_hex(&payload) != file.sha256 {
            return Err(NativeBundleError::Invalid(format!(
                "{} 的完整性校验失败",
                file.relative_path
            )));
        }
        let meta = read_meta_bytes(&payload, Path::new(&file.relative_path))?;
        found_source_thread |=
            meta.id == manifest.source_thread_id || meta.session_id == manifest.source_thread_id;
        files.push(payload);
        offset = end;
    }
    if offset != bytes.len() {
        return Err(NativeBundleError::Invalid("会话包包含多余数据".into()));
    }
    if !found_source_thread {
        return Err(NativeBundleError::Invalid(
            "会话包不包含清单指定的源会话".into(),
        ));
    }
    Ok(ParsedNativeBundle { manifest, files })
}

pub fn install_bundle(
    codex_home: &Path,
    bundle: &ParsedNativeBundle,
) -> Result<Vec<PathBuf>, NativeBundleError> {
    let mut installed = Vec::with_capacity(bundle.files.len());
    for (file, bytes) in bundle.manifest.files.iter().zip(&bundle.files) {
        let relative = Path::new(&file.relative_path);
        validate_relative_path(&file.relative_path)?;
        let target = codex_home.join(relative);
        if target.exists() {
            let existing =
                fs::read(&target).map_err(|error| NativeBundleError::Io(error.to_string()))?;
            if sha256_hex(&existing) != file.sha256 {
                return Err(NativeBundleError::Conflict(file.relative_path.clone()));
            }
            installed.push(target);
            continue;
        }
        let parent = target
            .parent()
            .ok_or_else(|| NativeBundleError::Invalid("目标路径无效".into()))?;
        fs::create_dir_all(parent).map_err(|error| NativeBundleError::Io(error.to_string()))?;
        let temporary = parent.join(format!(
            ".agentbridge-{}-{}.tmp",
            std::process::id(),
            installed.len()
        ));
        fs::write(&temporary, bytes).map_err(|error| NativeBundleError::Io(error.to_string()))?;
        fs::rename(&temporary, &target)
            .map_err(|error| NativeBundleError::Io(error.to_string()))?;
        installed.push(target);
    }
    Ok(installed)
}

pub fn make_thread_visible(
    codex_home: &Path,
    thread_id: &str,
    title: &str,
    preview: &str,
) -> Result<(), NativeBundleError> {
    let database_path = codex_home.join("state_5.sqlite");
    if !database_path.exists() {
        return Ok(());
    }
    let connection = Connection::open(database_path)
        .map_err(|error| NativeBundleError::Index(error.to_string()))?;
    let preview = truncate_chars(preview, MAX_PREVIEW_CHARS);
    let title = if title.trim().is_empty() {
        "AgentBridge 导入会话"
    } else {
        title.trim()
    };
    connection
        .execute(
            "UPDATE threads
             SET title = CASE WHEN title = '' THEN ?1 ELSE title END,
                 preview = CASE WHEN preview = '' THEN ?2 ELSE preview END,
                 first_user_message = CASE
                   WHEN first_user_message = '' THEN ?2
                   ELSE first_user_message
                 END,
                 has_user_event = 1,
                 thread_source = 'user'
             WHERE id = ?3",
            params![title, preview, thread_id],
        )
        .map_err(|error| NativeBundleError::Index(error.to_string()))?;
    Ok(())
}

pub fn preview_text(snapshot: &SnapshotV1) -> String {
    snapshot
        .messages
        .iter()
        .find(|message| message.role == crate::snapshot::schema::MessageRole::User)
        .map(|message| message.text.clone())
        .filter(|text| !text.trim().is_empty())
        .unwrap_or_else(|| snapshot.metadata.title.clone())
}

fn rollout_candidates(codex_home: &Path) -> Result<Vec<PathBuf>, NativeBundleError> {
    let mut candidates = Vec::new();
    for root in [
        codex_home.join("sessions"),
        codex_home.join("archived_sessions"),
    ] {
        if root.is_dir() {
            collect_rollout_files(&root, &mut candidates)?;
        }
    }
    Ok(candidates)
}

fn collect_rollout_files(root: &Path, output: &mut Vec<PathBuf>) -> Result<(), NativeBundleError> {
    for entry in fs::read_dir(root).map_err(|error| NativeBundleError::Io(error.to_string()))? {
        let entry = entry.map_err(|error| NativeBundleError::Io(error.to_string()))?;
        let path = entry.path();
        let file_type = entry
            .file_type()
            .map_err(|error| NativeBundleError::Io(error.to_string()))?;
        if file_type.is_dir() {
            collect_rollout_files(&path, output)?;
        } else if file_type.is_file() && is_rollout_file(&path) {
            output.push(path);
        }
    }
    Ok(())
}

fn is_rollout_file(path: &Path) -> bool {
    path.file_name()
        .and_then(OsStr::to_str)
        .is_some_and(|name| {
            name.starts_with("rollout-")
                && (name.ends_with(".jsonl") || name.ends_with(".jsonl.zst"))
        })
}

fn find_rollout_by_id(
    candidates: &[PathBuf],
    thread_id: &str,
) -> Result<PathBuf, NativeBundleError> {
    let direct: Vec<_> = candidates
        .iter()
        .filter(|path| {
            path.file_name()
                .and_then(OsStr::to_str)
                .is_some_and(|name| name.contains(thread_id))
        })
        .cloned()
        .collect();
    for path in direct {
        if read_meta(&path)
            .map(|meta| meta.id == thread_id || meta.session_id == thread_id)
            .unwrap_or(false)
        {
            return Ok(path);
        }
    }
    for path in candidates {
        if read_meta(path)
            .map(|meta| meta.id == thread_id || meta.session_id == thread_id)
            .unwrap_or(false)
        {
            return Ok(path.clone());
        }
    }
    Err(NativeBundleError::Rollout(format!(
        "找不到分叉依赖会话 {thread_id}"
    )))
}

fn read_meta(path: &Path) -> Result<RolloutMeta, NativeBundleError> {
    let file = fs::File::open(path).map_err(|error| NativeBundleError::Io(error.to_string()))?;
    let mut reader: Box<dyn BufRead> = if path.extension() == Some(OsStr::new("zst")) {
        Box::new(BufReader::new(
            zstd::stream::read::Decoder::new(file)
                .map_err(|error| NativeBundleError::Rollout(error.to_string()))?,
        ))
    } else {
        Box::new(BufReader::new(file))
    };
    read_meta_reader(&mut reader)
}

fn read_meta_bytes(bytes: &[u8], path: &Path) -> Result<RolloutMeta, NativeBundleError> {
    let cursor = Cursor::new(bytes);
    let mut reader: Box<dyn BufRead> = if path.extension() == Some(OsStr::new("zst")) {
        Box::new(BufReader::new(
            zstd::stream::read::Decoder::new(cursor)
                .map_err(|error| NativeBundleError::Rollout(error.to_string()))?,
        ))
    } else {
        Box::new(BufReader::new(cursor))
    };
    read_meta_reader(&mut reader)
}

fn read_meta_reader(reader: &mut dyn BufRead) -> Result<RolloutMeta, NativeBundleError> {
    let mut first = String::new();
    reader
        .read_line(&mut first)
        .map_err(|error| NativeBundleError::Rollout(error.to_string()))?;
    let line: RolloutLine = serde_json::from_str(&first)
        .map_err(|error| NativeBundleError::Rollout(error.to_string()))?;
    let id = line
        .payload
        .id
        .ok_or_else(|| NativeBundleError::Rollout("会话文件缺少 thread id".into()))?;
    let session_id = line.payload.session_id.unwrap_or_else(|| id.clone());
    Ok(RolloutMeta {
        id,
        session_id,
        history_base_thread_id: line.payload.history_base.map(|history| history.thread_id),
    })
}

fn validate_relative_path(relative_path: &str) -> Result<(), NativeBundleError> {
    let path = Path::new(relative_path);
    if path.is_absolute() {
        return Err(NativeBundleError::Invalid(
            "会话文件路径不能是绝对路径".into(),
        ));
    }
    let mut components = path.components();
    let first = components.next();
    if !matches!(
        first,
        Some(Component::Normal(value))
            if value == OsStr::new("sessions") || value == OsStr::new("archived_sessions")
    ) {
        return Err(NativeBundleError::Invalid(
            "会话文件必须位于 sessions 或 archived_sessions 目录".into(),
        ));
    }
    if components.any(|component| !matches!(component, Component::Normal(_))) {
        return Err(NativeBundleError::Invalid(
            "会话文件路径包含非法片段".into(),
        ));
    }
    if !is_rollout_file(path) {
        return Err(NativeBundleError::Invalid("会话文件类型无效".into()));
    }
    Ok(())
}

fn portable_relative(root: &Path, path: &Path) -> Result<String, NativeBundleError> {
    let relative = path
        .strip_prefix(root)
        .map_err(|_| NativeBundleError::Invalid("会话文件不在 CODEX_HOME 中".into()))?;
    let parts: Vec<String> = relative
        .components()
        .filter_map(|component| match component {
            Component::Normal(value) => Some(value.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect();
    Ok(parts.join("/"))
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn truncate_chars(value: &str, limit: usize) -> String {
    value.chars().take(limit).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundle_round_trip_preserves_rollout_bytes() {
        let root =
            std::env::temp_dir().join(format!("agentbridge-native-bundle-{}", std::process::id()));
        let sessions = root.join("sessions/2026/11/13");
        fs::create_dir_all(&sessions).expect("创建测试目录");
        let path = sessions.join("rollout-2026-11-13T00-00-00-thread-1.jsonl");
        fs::write(
            &path,
            b"{\"type\":\"session_meta\",\"payload\":{\"id\":\"thread-1\",\"session_id\":\"thread-1\"}}\n",
        )
        .expect("写入会话");
        let snapshot = SnapshotV1 {
            schema_version: 1,
            metadata: crate::snapshot::schema::SnapshotMetadata {
                source_thread_id: "thread-1".into(),
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
        };
        let bundle = build_bundle(&root, &path, &snapshot).expect("构建会话包");
        let parsed = parse_bundle(&bundle).expect("解析会话包");
        assert_eq!(parsed.manifest.source_thread_id, "thread-1");
        assert_eq!(parsed.files[0], fs::read(&path).expect("读取会话"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn rejects_path_traversal_in_bundle() {
        assert!(validate_relative_path("sessions/../../secret.jsonl").is_err());
        assert!(validate_relative_path("/tmp/sessions/rollout-x.jsonl").is_err());
    }
}
