use super::{
    ImportError, ImportRecord,
    prompt::{MAX_PROMPT_CHARS, build_continuation_prompt},
    store::ImportStore,
};
use crate::{
    codex::{client::CodexClient, error::CodexError},
    snapshot::schema::SnapshotV1,
};
use getrandom::fill as fill_random;
use std::{
    path::Path,
    time::Duration,
};

const TURN_TIMEOUT: Duration = Duration::from_secs(120);

pub trait LocalCodexClient {
    fn start_thread(&mut self, cwd: &str) -> Result<String, ImportError>;
    fn start_turn(
        &mut self,
        thread_id: &str,
        cwd: &str,
        text: &str,
    ) -> Result<(), ImportError>;
    fn wait_for_turn_completion(&mut self) -> Result<(), ImportError>;
}

struct CodexAdapter(CodexClient);

impl LocalCodexClient for CodexAdapter {
    fn start_thread(&mut self, cwd: &str) -> Result<String, ImportError> {
        self.0.start_thread(cwd).map_err(map_codex_error)
    }

    fn start_turn(
        &mut self,
        thread_id: &str,
        cwd: &str,
        text: &str,
    ) -> Result<(), ImportError> {
        self.0
            .start_turn(thread_id, cwd, text)
            .map_err(map_codex_error)
    }

    fn wait_for_turn_completion(&mut self) -> Result<(), ImportError> {
        self.0
            .wait_for_turn_completion(TURN_TIMEOUT)
            .map_err(map_codex_error)
    }
}

fn map_codex_error(error: CodexError) -> ImportError {
    ImportError::Codex(error.to_string())
}

pub type ClientFactory = Box<dyn Fn(&Path) -> Result<Box<dyn LocalCodexClient>, ImportError> + Send + Sync>;
pub type RecordIdFactory = Box<dyn Fn() -> String + Send + Sync>;

pub struct ImportService {
    client_factory: ClientFactory,
    record_id_factory: RecordIdFactory,
    thread_checker: Box<dyn Fn(&str) -> bool + Send + Sync>,
}

impl Default for ImportService {
    fn default() -> Self {
        Self {
            client_factory: Box::new(|_| {
                Ok(Box::new(CodexAdapter(CodexClient::connect().map_err(map_codex_error)?)))
            }),
            record_id_factory: Box::new(new_random_record_id),
            thread_checker: Box::new(|thread_id| {
                CodexClient::connect()
                    .and_then(|mut client| client.find_thread(thread_id))
                    .ok()
                    .flatten()
                    .is_some()
            }),
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
        }
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
        snapshot: &SnapshotV1,
        project_path: &Path,
        snapshot_path: &Path,
        store: &dyn ImportStore,
    ) -> Result<ImportRecord, ImportError> {
        if !project_path.is_dir() {
            return Err(ImportError::InvalidProject("请选择接收方电脑上存在的项目目录".into()));
        }
        let project = project_path.to_string_lossy().into_owned();
        let source_path = snapshot
            .metadata
            .source_working_directory
            .as_deref()
            .unwrap_or_default();
        if !source_path.is_empty()
            && (project == source_path
                || Path::new(&project).ends_with(Path::new(source_path)))
        {
            return Err(ImportError::InvalidProject(
                "分享方工作目录只作说明，请选择接收方自己的项目目录".into(),
            ));
        }
        if project_path == snapshot_path {
            return Err(ImportError::InvalidProject(
                "请选择接收方自己的项目目录，不能使用暂存快照文件".into(),
            ));
        }
        let prompt = build_continuation_prompt(snapshot);
        if prompt.chars().count() > MAX_PROMPT_CHARS {
            return Err(ImportError::PromptTooLarge);
        }
        let id = self.new_record_id();
        let imported_at = crate::pairing::invitation::unix_seconds();
        let mut record = ImportRecord {
            id,
            source_title: snapshot.metadata.title.clone(),
            local_thread_id: String::new(),
            project_path: project.clone(),
            imported_at,
            snapshot_path: snapshot_path.to_string_lossy().into_owned(),
            status: "pending".into(),
            error: None,
            continuation_prompt: Some(prompt.clone()),
            retry_count: 0,
            last_retried_at: None,
        };
        store.upsert(&record).map_err(map_store_error)?;

        let result = (|| {
            let mut client = (self.client_factory)(project_path)?;
            let thread_id = client.start_thread(&project)?;
            if thread_id == snapshot.metadata.source_thread_id || thread_id.is_empty() {
                return Err(ImportError::Codex("本机 thread ID 无效或与源会话冲突".into()));
            }
            client.start_turn(&thread_id, &project, &prompt)?;
            client.wait_for_turn_completion()?;
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
