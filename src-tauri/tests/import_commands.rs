use agentbridge_lib::{
    commands::{delete_import_files, retry_import_record},
    db::ImportDatabase,
    imports::{
        ImportRecord, ImportService,
        create_thread::LocalCodexClient,
    },
    snapshot::{
        schema::{MessageRole, SnapshotMessage, SnapshotMetadata, SnapshotV1},
    },
};
use std::{
    fs,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

fn unique_dir(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("系统时间有效")
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "agentbridge-import-commands-{label}-{}-{nanos}",
        std::process::id()
    ));
    fs::create_dir_all(&path).expect("创建测试目录");
    path
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
        warnings: vec![],
    }
}

fn completed_record(id: &str, project: &str, snapshot_path: &str) -> ImportRecord {
    ImportRecord {
        id: id.into(),
        source_title: "修复登录流程".into(),
        local_thread_id: "local-thread-1".into(),
        project_path: project.into(),
        imported_at: 1_800_000_100,
        snapshot_path: snapshot_path.into(),
        status: "completed".into(),
        error: None,
        continuation_prompt: Some("不可信历史提示".into()),
        retry_count: 0,
        last_retried_at: None,
    }
}

fn failed_record(id: &str, project: &str, snapshot_path: &str, retry_count: i64) -> ImportRecord {
    let mut record = completed_record(id, project, snapshot_path);
    record.status = "failed".into();
    record.local_thread_id.clear();
    record.retry_count = retry_count;
    record
}

struct FakeCodexClient {
    fail: bool,
}

impl LocalCodexClient for FakeCodexClient {
    fn start_thread(&mut self, _cwd: &str) -> Result<String, agentbridge_lib::imports::ImportError> {
        if self.fail {
            return Err(agentbridge_lib::imports::ImportError::Codex("Codex 不可用".into()));
        }
        Ok("new-local-thread".into())
    }

    fn start_turn(
        &mut self,
        _thread_id: &str,
        _cwd: &str,
        _text: &str,
    ) -> Result<(), agentbridge_lib::imports::ImportError> {
        Ok(())
    }

    fn wait_for_turn_completion(&mut self) -> Result<(), agentbridge_lib::imports::ImportError> {
        Ok(())
    }
}

#[test]
fn retrying_a_completed_import_is_rejected_before_starting_codex() {
    let dir = unique_dir("completed-retry");
    let project = dir.join("receiver-project");
    let staged = dir.join("staged-snapshots").join("staged.json");
    fs::create_dir_all(&project).expect("创建接收方项目");
    fs::create_dir_all(staged.parent().expect("暂存目录")).expect("创建暂存目录");
    fs::write(&staged, serde_json::to_vec(&snapshot()).expect("快照可序列化"))
        .expect("写入暂存快照");
    let database = ImportDatabase::open(dir.join("imports.db")).expect("数据库可打开");
    database
        .upsert(&completed_record(
            "import-completed",
            &project.to_string_lossy(),
            &staged.to_string_lossy(),
        ))
        .expect("保存已完成记录");
    let starts = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&starts);
    let service = ImportService::with_client_factory(
        Box::new(move |_| {
            counted.fetch_add(1, Ordering::SeqCst);
            Ok(Box::new(FakeCodexClient { fail: false }) as Box<dyn LocalCodexClient>)
        }),
        Box::new(|| "replacement-import-id".into()),
    );

    let error = retry_import_record(&service, &database, "import-completed", 1_800_000_200)
        .expect_err("已完成导入不能再创建 thread");

    assert_eq!(error.code, "notRetryable");
    assert!(error.message.contains("已完成"), "应说明拒绝原因：{}", error.message);
    assert_eq!(starts.load(Ordering::SeqCst), 0, "不能启动 Codex");
    let records = database.list().expect("可列出导入记录");
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].local_thread_id, "local-thread-1");
    assert_eq!(records[0].retry_count, 0, "拒绝重试不能增加重试次数");
    fs::remove_dir_all(&dir).expect("清理测试目录");
}

#[test]
fn retrying_a_failed_import_keeps_one_record_and_updates_retry_metadata() {
    let dir = unique_dir("failed-retry");
    let project = dir.join("receiver-project");
    let staged = dir.join("staged-snapshots").join("staged.json");
    fs::create_dir_all(&project).expect("创建接收方项目");
    fs::create_dir_all(staged.parent().expect("暂存目录")).expect("创建暂存目录");
    fs::write(&staged, serde_json::to_vec(&snapshot()).expect("快照可序列化"))
        .expect("写入暂存快照");
    let database = ImportDatabase::open(dir.join("imports.db")).expect("数据库可打开");
    database
        .upsert(&failed_record(
            "import-failed",
            &project.to_string_lossy(),
            &staged.to_string_lossy(),
            2,
        ))
        .expect("保存失败记录");
    let service = ImportService::with_client_factory(
        Box::new(|_| {
            Ok(Box::new(FakeCodexClient { fail: true }) as Box<dyn LocalCodexClient>)
        }),
        Box::new(|| "unexpected-new-id".into()),
    );

    let error = retry_import_record(&service, &database, "import-failed", 1_800_000_300)
        .expect_err("Codex 不可用时应报告失败");
    assert_eq!(error.code, "codexUnavailable");

    let records = database.list().expect("可列出导入记录");
    let record = records
        .iter()
        .find(|record| record.id == "import-failed")
        .expect("重试应更新原记录");
    assert_eq!(record.status, "failed");
    assert_eq!(record.retry_count, 3);
    assert_eq!(record.last_retried_at, Some(1_800_000_300));
    assert_eq!(records.len(), 2, "当前导入失败模型保留独立的失败记录");
    assert_eq!(
        records
            .iter()
            .filter(|record| record.id == "unexpected-new-id")
            .count(),
        1,
        "每次重试都应保留可诊断的新失败记录"
    );
    fs::remove_dir_all(&dir).expect("清理测试目录");
}

#[test]
fn delete_import_command_removes_the_staged_snapshot_inside_app_data_only() {
    let dir = unique_dir("command-delete");
    let staged_dir = dir.join("staged-snapshots");
    let staged = staged_dir.join("staged.json");
    let outside = dir.join("outside.json");
    fs::create_dir_all(&staged_dir).expect("创建暂存目录");
    fs::write(&staged, b"{\"schemaVersion\":1}").expect("写入暂存快照");
    fs::write(&outside, b"{\"schemaVersion\":1}").expect("写入外部文件");
    let database = ImportDatabase::open(dir.join("imports.db")).expect("数据库可打开");
    for (id, snapshot_path) in [
        ("import-inside", staged.to_string_lossy().into_owned()),
        ("import-outside", outside.to_string_lossy().into_owned()),
    ] {
        database
            .upsert(&completed_record(id, &dir.to_string_lossy(), &snapshot_path))
            .expect("保存导入记录");
    }
    let staged_entries: Mutex<std::collections::HashMap<String, String>> = Mutex::new(
        [("import-inside".to_owned(), staged.to_string_lossy().into_owned())]
            .into_iter()
            .collect(),
    );

    delete_import_files(&database, &dir, "import-inside", |snapshot_path| {
        staged_entries
            .lock()
            .expect("暂存锁可读取")
            .retain(|_, path| path != snapshot_path);
    })
    .expect("应删除应用数据目录内的暂存快照");
    delete_import_files(&database, &dir, "import-outside", |snapshot_path| {
        staged_entries
            .lock()
            .expect("暂存锁可读取")
            .retain(|_, path| path != snapshot_path);
    })
    .expect("应允许删除记录并跳过目录外的文件");

    assert!(!staged.exists(), "应用数据目录内的暂存快照应被清理");
    assert!(outside.exists(), "目录外的文件不能被删除");
    assert!(database.list().expect("可列出记录").is_empty());
    assert!(staged_entries.lock().expect("暂存锁可读取").is_empty());
    fs::remove_dir_all(&dir).expect("清理测试目录");
}

#[test]
fn delete_import_command_refuses_staged_paths_that_escape_the_app_data_directory() {
    let dir = unique_dir("traversal-delete");
    let staged_dir = dir.join("staged-snapshots");
    let outside = dir.join("outside.json");
    fs::create_dir_all(&staged_dir).expect("创建暂存目录");
    fs::write(&outside, b"{\"schemaVersion\":1}").expect("写入外部文件");
    let traversal = staged_dir.join("..").join("outside.json");
    let database = ImportDatabase::open(dir.join("imports.db")).expect("数据库可打开");
    database
        .upsert(&completed_record(
            "import-traversal",
            &dir.to_string_lossy(),
            &traversal.to_string_lossy(),
        ))
        .expect("保存导入记录");

    delete_import_files(&database, &dir, "import-traversal", |_| {})
        .expect("记录仍应可删除");

    assert!(outside.exists(), "不能通过 .. 删除暂存目录外的文件");
    assert!(database.list().expect("可列出记录").is_empty());
    fs::remove_dir_all(&dir).expect("清理测试目录");
}

#[test]
fn deleting_an_import_removes_its_local_snapshot_but_keeps_the_codex_thread() {
    let dir = unique_dir("delete");
    let snapshot = dir.join("staged-snapshots").join("staged.json");
    fs::create_dir_all(snapshot.parent().expect("暂存目录")).expect("创建暂存目录");
    fs::write(&snapshot, b"{\"schemaVersion\":1}").expect("写入暂存快照");
    let database = ImportDatabase::open(dir.join("imports.db")).expect("数据库可打开");
    database
        .upsert(&completed_record(
            "import-1",
            &dir.to_string_lossy(),
            &snapshot.to_string_lossy(),
        ))
        .expect("保存记录");
    database.delete("import-1").expect("删除记录");
    assert!(database.list().expect("可列出记录").is_empty());
    assert!(
        snapshot.exists(),
        "纯数据库删除不能直接删除文件；命令层负责受限路径清理"
    );
    fs::remove_dir_all(dir).expect("清理测试目录");
}
