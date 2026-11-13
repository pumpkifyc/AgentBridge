use super::{ImportError, ImportRecord, store::ImportStore};
use crate::{
    codex::{client::CodexClient, error::CodexError},
    native::{self, NativeBundleError},
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
        let bundle = native::parse_bundle(bundle_bytes).map_err(map_native_error)?;
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
            native::install_bundle(&codex_home, &bundle).map_err(map_native_error)?;
            let mut client = (self.client_factory)(project_path)?;
            let thread_id = client.fork_thread(
                &bundle.manifest.source_thread_id,
                &project,
                bundle.manifest.cutoff_turn_id.as_deref(),
            )?;
            if thread_id == bundle.manifest.source_thread_id || thread_id.is_empty() {
                return Err(ImportError::Codex(
                    "本机 thread ID 无效或与源会话冲突".into(),
                ));
            }
            client.set_thread_name(&thread_id, &bundle.manifest.title)?;
            native::make_thread_visible(
                &codex_home,
                &thread_id,
                &bundle.manifest.title,
                &bundle.manifest.preview,
            )
            .map_err(map_native_error)?;
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

fn map_store_error(error: super::store::StoreError) -> ImportError {
    ImportError::Store(error.to_string())
}

fn map_native_error(error: NativeBundleError) -> ImportError {
    ImportError::Native(error.to_string())
}
