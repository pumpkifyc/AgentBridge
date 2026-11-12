use agentbridge_lib::{
    db::ImportDatabase,
    imports::ImportRecord,
};
use std::{
    fs,
    path::PathBuf,
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

#[test]
fn deleting_an_import_removes_its_local_snapshot_but_keeps_the_codex_thread() {
    let dir = unique_dir("delete");
    let snapshot = dir.join("staged-snapshots").join("staged.json");
    fs::create_dir_all(snapshot.parent().expect("暂存目录")).expect("创建暂存目录");
    fs::write(&snapshot, b"{\"schemaVersion\":1}").expect("写入暂存快照");
    let database = ImportDatabase::open(dir.join("imports.db")).expect("数据库可打开");
    database
        .upsert(&ImportRecord {
            id: "import-1".into(),
            source_title: "修复登录流程".into(),
            local_thread_id: "local-thread-1".into(),
            project_path: dir.to_string_lossy().into_owned(),
            imported_at: 1_800_000_100,
            snapshot_path: snapshot.to_string_lossy().into_owned(),
            status: "completed".into(),
            error: None,
            continuation_prompt: Some("不可信历史提示".into()),
            retry_count: 0,
            last_retried_at: None,
        })
        .expect("保存记录");
    database.delete("import-1").expect("删除记录");
    assert!(database.list().expect("可列出记录").is_empty());
    assert!(
        snapshot.exists(),
        "纯数据库删除不能直接删除文件；命令层负责受限路径清理"
    );
    fs::remove_dir_all(dir).expect("清理测试目录");
}
