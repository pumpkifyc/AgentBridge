use super::{ImportError, ImportRecord, store::ImportStore};
use crate::{
    codex::{client::CodexClient, error::CodexError},
    native::{self, InstallConflictPolicy, NativeBundleError},
};
use getrandom::fill as fill_random;
use std::path::{Path, PathBuf};

pub trait LocalCodexClient {
    fn fork_thread(
        &mut self,
        source_thread_id: &str,
        cwd: &str,
        last_turn_id: Option<&str>,
    ) -> Result<String, ImportError>;
    fn set_thread_name(&mut self, thread_id: &str, name: &str) -> Result<(), ImportError>;
}

struct CodexAdapter(CodexClient);

impl LocalCodexClient for CodexAdapter {
    fn fork_thread(
        &mut self,
        source_thread_id: &str,
        cwd: &str,
        last_turn_id: Option<&str>,
    ) -> Result<String, ImportError> {
        self.0
            .fork_thread(source_thread_id, cwd, last_turn_id)
            .map_err(map_codex_error)
    }

    fn set_thread_name(&mut self, thread_id: &str, name: &str) -> Result<(), ImportError> {
        self.0
            .set_thread_name(thread_id, name)
            .map_err(map_codex_error)
    }
}

fn map_codex_error(error: CodexError) -> ImportError {
    ImportError::Codex(error.to_string())
}

pub type ClientFactory =
    Box<dyn Fn(&Path) -> Result<Box<dyn LocalCodexClient>, ImportError> + Send + Sync>;
pub type RecordIdFactory = Box<dyn Fn() -> String + Send + Sync>;
const MAX_SESSION_NAME_CHARS: usize = 120;

pub struct ImportService {
    client_factory: ClientFactory,
    record_id_factory: RecordIdFactory,
    thread_checker: Box<dyn Fn(&str) -> bool + Send + Sync>,
    codex_home: Option<PathBuf>,
}

impl Default for ImportService {
    fn default() -> Self {
        Self {
            client_factory: Box::new(|_| {
                Ok(Box::new(CodexAdapter(
                    CodexClient::connect().map_err(map_codex_error)?,
                )))
            }),
            record_id_factory: Box::new(new_random_record_id),
            thread_checker: Box::new(|thread_id| {
                CodexClient::connect()
                    .and_then(|mut client| client.find_thread(thread_id))
                    .ok()
                    .flatten()
                    .is_some()
            }),
            codex_home: None,
        }
    }
}

fn new_random_record_id() -> String {
    let mut bytes = [0u8; 16];
    fill_random(&mut bytes).expect("操作系统随机数生成器失败");
    format!("import-{}", hex::encode(bytes))
}

impl ImportService {
    pub fn with_client_factory(
        client_factory: ClientFactory,
        record_id_factory: RecordIdFactory,
    ) -> Self {
        Self {
            client_factory,
            record_id_factory,
            thread_checker: Box::new(|_| true),
            codex_home: None,
        }
    }

    pub fn with_codex_home(mut self, codex_home: impl Into<PathBuf>) -> Self {
        self.codex_home = Some(codex_home.into());
        self
    }

    pub fn with_thread_checker(
        mut self,
        thread_checker: Box<dyn Fn(&str) -> bool + Send + Sync>,
    ) -> Self {
        self.thread_checker = thread_checker;
        self
    }

    pub fn thread_exists(&self, thread_id: &str) -> bool {
        (self.thread_checker)(thread_id)
    }

    pub fn new_record_id(&self) -> String {
        (self.record_id_factory)()
    }

    pub fn create_local_thread(
        &self,
        bundle_bytes: &[u8],
        project_path: &Path,
        bundle_path: &Path,
        store: &dyn ImportStore,
    ) -> Result<ImportRecord, ImportError> {
        self.create_local_thread_with_options(
            bundle_bytes,
            project_path,
            bundle_path,
            store,
            None,
            InstallConflictPolicy::PreserveLocal,
        )
    }

    pub fn create_local_thread_with_options(
        &self,
        bundle_bytes: &[u8],
        project_path: &Path,
        bundle_path: &Path,
        store: &dyn ImportStore,
        session_name: Option<&str>,
        conflict_policy: InstallConflictPolicy,
    ) -> Result<ImportRecord, ImportError> {
        let bundle = native::parse_bundle(bundle_bytes).map_err(map_native_error)?;
        let session_name = normalize_session_name(&bundle.manifest.title, session_name)?;
        if !project_path.is_dir() {
            return Err(ImportError::InvalidProject(
                "请选择接收方电脑上存在的项目目录".into(),
            ));
        }
        let project = project_path.to_string_lossy().into_owned();
        let source_path = bundle
            .manifest
            .source_working_directory
            .as_deref()
            .unwrap_or_default();
        if !source_path.is_empty()
            && (project == source_path || Path::new(&project).ends_with(Path::new(source_path)))
        {
            return Err(ImportError::InvalidProject(
                "分享方工作目录只作说明，请选择接收方自己的项目目录".into(),
            ));
        }
        if project_path == bundle_path {
            return Err(ImportError::InvalidProject(
                "请选择接收方自己的项目目录，不能使用暂存会话包".into(),
            ));
        }
        let id = self.new_record_id();
        let imported_at = crate::pairing::invitation::unix_seconds();
        let mut record = ImportRecord {
            id,
            source_title: bundle.manifest.title.clone(),
            session_name: session_name.clone(),
            local_thread_id: String::new(),
            project_path: project.clone(),
            imported_at,
            snapshot_path: bundle_path.to_string_lossy().into_owned(),
            status: "pending".into(),
            error: None,
            continuation_prompt: None,
            retry_count: 0,
            last_retried_at: None,
        };
        store.upsert(&record).map_err(map_store_error)?;

        let result = (|| {
            let codex_home = self
                .codex_home
                .clone()
                .map(Ok)
                .unwrap_or_else(native::codex_home)
                .map_err(map_native_error)?;
            let mut installation =
                native::install_bundle_with_policy(&codex_home, &bundle, conflict_policy)
                    .map_err(map_native_error)?;
            let mut client = (self.client_factory)(project_path)?;
            let fork_turn_id = bundle
                .manifest
                .cutoff_turn_id
                .clone()
                .or(native::source_latest_terminal_turn_id(&bundle).map_err(map_native_error)?);
            let thread_id = client.fork_thread(
                &bundle.manifest.source_thread_id,
                &project,
                fork_turn_id.as_deref(),
            )?;
            if thread_id == bundle.manifest.source_thread_id || thread_id.is_empty() {
                return Err(ImportError::Codex(
                    "本机 thread ID 无效或与源会话冲突".into(),
                ));
            }
            client.set_thread_name(&thread_id, &session_name)?;
            native::make_thread_visible(
                &codex_home,
                &thread_id,
                &session_name,
                &bundle.manifest.preview,
            )
            .map_err(map_native_error)?;
            drop(client);
            installation.complete().map_err(map_native_error)?;
            Ok(thread_id)
        })();
        match result {
            Ok(thread_id) => {
                record.local_thread_id = thread_id;
                record.status = "completed".into();
                record.error = None;
                store.upsert(&record).map_err(map_store_error)?;
                Ok(record)
            }
            Err(error) => {
                record.status = "failed".into();
                record.error = Some(error.to_string());
                store.upsert(&record).map_err(map_store_error)?;
                Err(error)
            }
        }
    }
}

fn normalize_session_name(
    source_title: &str,
    requested: Option<&str>,
) -> Result<String, ImportError> {
    let name = requested
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| source_title.trim());
    let name = if name.is_empty() {
        "AgentBridge 导入会话"
    } else {
        name
    };
    if name.chars().count() > MAX_SESSION_NAME_CHARS {
        return Err(ImportError::InvalidName(format!(
            "最多允许 {MAX_SESSION_NAME_CHARS} 个字符"
        )));
    }
    if name.chars().any(char::is_control) {
        return Err(ImportError::InvalidName("不能包含控制字符".into()));
    }
    if requested.is_some_and(|value| value.trim().is_empty()) {
        return Err(ImportError::InvalidName("名称不能为空".into()));
    }
    Ok(name.to_owned())
}

fn map_store_error(error: super::store::StoreError) -> ImportError {
    ImportError::Store(error.to_string())
}

fn map_native_error(error: NativeBundleError) -> ImportError {
    ImportError::Native(error.to_string())
}
