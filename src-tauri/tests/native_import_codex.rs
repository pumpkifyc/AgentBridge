use agentbridge_lib::{
    codex::{client::CodexClient, executable::resolve_codex_executable},
    native,
    snapshot::schema::{MessageRole, SnapshotMessage, SnapshotMetadata, SnapshotV1},
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
        "agentbridge-native-codex-{label}-{}-{nanos}",
        std::process::id()
    ));
    fs::create_dir_all(&path).expect("创建测试目录");
    path
}

fn snapshot() -> SnapshotV1 {
    SnapshotV1 {
        schema_version: 1,
        metadata: SnapshotMetadata {
            source_thread_id: "019c0000-0000-7000-8000-000000000001".into(),
            title: "AgentBridge 原生导入验证".into(),
            created_at: 1_800_000_000,
            updated_at: 1_800_000_010,
            cutoff_turn_id: None,
            project_name: Some("agentbridge".into()),
            repository_id: None,
            source_working_directory: Some("/tmp/source-project".into()),
        },
        messages: vec![SnapshotMessage {
            role: MessageRole::User,
            text: "验证原生会话导入".into(),
            turn_index: 0,
            timestamp: Some(1_800_000_000),
            historical_tool_summary: None,
        }],
        warnings: Vec::new(),
    }
}

#[test]
#[ignore = "需要本机已安装 Codex CLI；验证时在临时 CODEX_HOME 中执行 thread/fork"]
fn installs_a_bundle_and_forks_a_native_thread_without_touching_real_codex_home() {
    let root = unique_dir("fork");
    let codex_home = root.join("codex-home");
    let sessions = codex_home.join("sessions/2026/11/13");
    fs::create_dir_all(&sessions).expect("创建临时 CODEX_HOME");
    let source =
        sessions.join("rollout-2026-11-13T00-00-00-019c0000-0000-7000-8000-000000000001.jsonl");
    fs::write(
        &source,
        concat!(
            "{\"ordinal\":0,\"type\":\"session_meta\",\"timestamp\":\"2026-11-13T00:00:00.000Z\",\"payload\":{\"id\":\"019c0000-0000-7000-8000-000000000001\",\"session_id\":\"019c0000-0000-7000-8000-000000000001\",\"timestamp\":\"2026-11-13T00:00:00.000Z\",\"cwd\":\"/tmp/source-project\",\"originator\":\"AgentBridge test\",\"cli_version\":\"0.167.2\",\"base_instructions\":{\"text\":\"test\",\"provenance\":{\"type\":\"model\",\"model\":\"gpt-5\"}},\"context_window\":{\"window_id\":\"019c0000-0000-7000-8000-000000000002\"},\"git\":null,\"history_mode\":\"paginated\",\"model_provider\":\"openai\",\"runtime_workspace_roots\":[\"/tmp/source-project\"],\"source\":\"cli\",\"thread_source\":\"user\"}}\n",
            "{\"ordinal\":1,\"type\":\"response_item\",\"timestamp\":\"2026-11-13T00:00:01.000Z\",\"payload\":{\"type\":\"message\",\"id\":\"019c0000-0000-7000-8000-000000000003\",\"role\":\"user\",\"content\":[{\"type\":\"input_text\",\"text\":\"验证原生会话导入\"}],\"internal_chat_message_metadata_passthrough\":{}}}\n"
        ),
    )
    .expect("写入测试 rollout");
    let bundle = native::build_bundle(&codex_home, &source, &snapshot()).expect("构建会话包");
    let parsed = native::parse_bundle(&bundle).expect("解析会话包");

    fs::remove_file(&source).expect("移除源文件以验证安装步骤");
    native::install_bundle(&codex_home, &parsed).expect("安装会话包");
    assert!(source.exists(), "安装后应恢复 rollout 文件");

    let project = root.join("receiver-project");
    fs::create_dir_all(&project).expect("创建接收方项目");
    let mut client = CodexClient::connect_with_executable_and_home(
        resolve_codex_executable().expect("找不到 Codex CLI，无法验证原生 thread/fork"),
        &codex_home,
    )
    .expect("启动临时 CODEX_HOME 的 Codex App Server");
    let fork_id = client
        .fork_thread(
            &parsed.manifest.source_thread_id,
            project.to_str().expect("项目路径应为 UTF-8"),
            None,
        )
        .expect("公开 thread/fork 应创建原生会话");
    assert_ne!(fork_id, parsed.manifest.source_thread_id);
    client
        .set_thread_name(&fork_id, &parsed.manifest.title)
        .expect("应设置本机会话名称");
    native::make_thread_visible(
        &codex_home,
        &fork_id,
        &parsed.manifest.title,
        &parsed.manifest.preview,
    )
    .expect("应修复本机会话可见性");

    let _ = fs::remove_dir_all(root);
}

#[test]
#[ignore = "设置 AGENTBRIDGE_REAL_CODEX_THREAD_ID 后，验证真实多层历史分叉会物化全部祖先轮次"]
fn full_history_fork_materializes_all_ancestor_rollout_turns_for_real_thread() {
    let source_thread_id = std::env::var("AGENTBRIDGE_REAL_CODEX_THREAD_ID")
        .expect("需要设置 AGENTBRIDGE_REAL_CODEX_THREAD_ID");
    let executable = resolve_codex_executable().expect("找不到 Codex CLI，无法读取真实多层历史");
    let real_home = native::codex_home().expect("无法定位真实 CODEX_HOME");
    let mut real_client =
        CodexClient::connect_with_executable_and_home(executable.clone(), &real_home)
            .expect("启动真实 CODEX_HOME 的 Codex App Server");
    let source_rollout_path = real_client
        .thread_rollout_path(&source_thread_id)
        .expect("读取真实会话 rollout 路径");
    let source_turns = real_client
        .list_turns(&source_thread_id)
        .expect("读取真实分享源轮次");
    assert!(source_turns.len() > 1, "真实分享源应包含多个祖先轮次");
    let mut source_snapshot = snapshot();
    source_snapshot.metadata.source_thread_id = source_thread_id.clone();
    let bundle = native::build_bundle(
        &real_home,
        std::path::Path::new(&source_rollout_path),
        &source_snapshot,
    )
    .expect("构建真实完整历史包");
    let parsed = native::parse_bundle(&bundle).expect("解析真实完整历史包");
    assert!(
        parsed.files.len() > 1,
        "真实多层会话应包含多个 rollout 文件"
    );

    let root = unique_dir("full-history");
    let codex_home = root.join("codex-home");
    fs::create_dir_all(&codex_home).expect("创建临时 CODEX_HOME");
    native::install_bundle(&codex_home, &parsed).expect("安装真实完整历史包");
    let project = root.join("receiver-project");
    fs::create_dir_all(&project).expect("创建接收方项目");
    let mut client = CodexClient::connect_with_executable_and_home(executable, &codex_home)
        .expect("启动临时 CODEX_HOME 的 Codex App Server");
    let latest_terminal_turn = native::source_latest_terminal_turn_id(&parsed)
        .expect("读取最新终态轮次")
        .expect("完整历史应包含终态轮次");
    let fork_id = client
        .fork_thread(
            &parsed.manifest.source_thread_id,
            project.to_str().expect("项目路径应为 UTF-8"),
            Some(&latest_terminal_turn),
        )
        .expect("按最新终态轮次创建原生历史分支");
    let turns = client.list_turns(&fork_id).expect("读取本机分支轮次");

    assert_eq!(
        turns
            .iter()
            .map(|turn| turn.id.as_str())
            .collect::<Vec<_>>(),
        source_turns
            .iter()
            .map(|turn| turn.id.as_str())
            .collect::<Vec<_>>(),
        "原生分叉必须包含完整祖先轮次"
    );

    let _ = fs::remove_dir_all(root);
}
