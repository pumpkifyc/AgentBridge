use agentbridge_lib::{
    codex::client::CodexClient,
    native,
    preview::{
        convert::convert_preview_timeline,
        schema::{PreviewBlockKind, PreviewItemKind},
    },
    snapshot::convert::convert_codex_items,
};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

#[cfg(test)]
const FAKE_SERVER: &str = env!("CARGO_BIN_EXE_fake_codex_server");

fn unique_dir(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("系统时间有效")
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "agentbridge-full-fidelity-{label}-{}-{nanos}",
        std::process::id()
    ));
    fs::create_dir_all(&path).expect("创建测试目录");
    path
}

fn fake_executable(root: &Path) -> PathBuf {
    let binary = root.join(if cfg!(windows) {
        "fake-codex.exe"
    } else {
        "fake-codex"
    });
    fs::copy(FAKE_SERVER, &binary).expect("复制伪造 App Server");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(&binary).expect("读取权限").permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&binary, permissions).expect("设置执行权限");
    }
    if cfg!(windows) {
        let runner = root.join("runner.cmd");
        fs::write(
            &runner,
            format!("@echo off\r\n\"{}\" full-fidelity %*\r\n", binary.display()),
        )
        .expect("写入 Windows 测试启动器");
        runner
    } else {
        let runner = root.join("runner.sh");
        let binary = binary.display().to_string().replace('\'', "'\\''");
        fs::write(
            &runner,
            format!("#!/bin/sh\nexec '{binary}' full-fidelity \"$@\"\n"),
        )
        .expect("写入测试启动器");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut permissions = fs::metadata(&runner).expect("读取权限").permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(&runner, permissions).expect("设置执行权限");
        }
        runner
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[test]
fn full_fidelity_preview_and_native_bundle_share_one_complete_history() {
    let root = unique_dir("integration");
    let codex_home = root.join("codex-home");
    let sessions = codex_home.join("sessions/2026/11/13");
    fs::create_dir_all(&sessions).expect("创建临时 CODEX_HOME");

    let source = sessions.join("rollout-2026-11-13T00-00-00-source-thread.jsonl");
    let parent = sessions.join("rollout-2026-11-12T00-00-00-parent-thread.jsonl");
    let source_bytes = concat!(
        "{\"type\":\"session_meta\",\"payload\":{\"id\":\"source-thread\",\"session_id\":\"source-thread\",\"history_base\":{\"thread_id\":\"parent-thread\",\"end_ordinal_exclusive\":1,\"end_byte_offset\":1}}}\n",
        "{\"ordinal\":1,\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"role\":\"user\",\"content\":[{\"type\":\"input_text\",\"text\":\"完整预览\"}]}}\n"
    );
    let parent_bytes = "{\"type\":\"session_meta\",\"payload\":{\"id\":\"parent-thread\",\"session_id\":\"parent-thread\"}}\n";
    fs::write(&source, source_bytes).expect("写入源 rollout");
    fs::write(&parent, parent_bytes).expect("写入父 rollout");

    let runner = fake_executable(&root);
    let mut client = CodexClient::connect_with_executable_and_home(&runner, &codex_home)
        .expect("连接伪造 App Server");
    let recorded = client
        .read_thread_data("source-thread")
        .expect("读取完整会话");
    let timeline = convert_preview_timeline(&recorded.data, None).expect("生成完整预览");
    let snapshot = convert_codex_items(recorded.data.clone(), None).expect("生成兼容快照");

    let item_kinds = timeline.turns[0]
        .items
        .iter()
        .map(|item| item.kind)
        .collect::<Vec<_>>();
    let expected_kinds = [
        PreviewItemKind::UserMessage,
        PreviewItemKind::AssistantMessage,
        PreviewItemKind::Plan,
        PreviewItemKind::Reasoning,
        PreviewItemKind::Command,
        PreviewItemKind::FileChange,
        PreviewItemKind::ToolCall,
        PreviewItemKind::WebSearch,
        PreviewItemKind::Image,
        PreviewItemKind::Error,
        PreviewItemKind::Unknown,
    ];
    for expected in expected_kinds {
        assert!(item_kinds.contains(&expected), "预览缺少类别 {expected:?}");
    }

    let future = timeline.turns[0]
        .items
        .iter()
        .find(|item| item.kind == PreviewItemKind::Unknown)
        .expect("未知条目应保留");
    assert_eq!(future.blocks[0].kind, PreviewBlockKind::Json);
    assert!(future.blocks[0].content.contains("\"payloadValue\": 42"));
    assert!(
        timeline.turns[0]
            .items
            .iter()
            .filter(|item| item.kind == PreviewItemKind::Command)
            .flat_map(|item| &item.blocks)
            .any(|block| block.sensitive
                && block.content == "token=sk-test-example-12345678 全部测试通过")
    );

    let rollout_path = recorded.rollout_path.expect("应返回源 rollout 路径");
    let bundle = native::build_bundle(&codex_home, Path::new(&rollout_path), &snapshot)
        .expect("构建原生会话包");
    let parsed = native::parse_bundle(&bundle).expect("解析原生会话包");
    assert_eq!(parsed.manifest.files.len(), 2);
    assert_eq!(parsed.files.len(), 2);

    for (descriptor, bytes) in parsed.manifest.files.iter().zip(&parsed.files) {
        assert_eq!(descriptor.size, bytes.len() as u64);
        assert_eq!(descriptor.sha256, sha256_hex(bytes));
        let source_path = codex_home.join(&descriptor.relative_path);
        assert_eq!(*bytes, fs::read(source_path).expect("读取原生 rollout"));
    }
    assert!(
        parsed
            .files
            .iter()
            .any(|bytes| bytes == source_bytes.as_bytes())
    );
    assert!(
        parsed
            .files
            .iter()
            .any(|bytes| bytes == parent_bytes.as_bytes())
    );
    assert!(
        timeline
            .notices
            .iter()
            .any(|notice| notice.code == "sensitive_blocks")
    );

    println!(
        "类别数={} 条目数={} 原生文件数={} SHA一致=true",
        expected_kinds.len(),
        timeline
            .turns
            .iter()
            .map(|turn| turn.items.len())
            .sum::<usize>(),
        parsed.files.len()
    );

    drop(client);
    let _ = fs::remove_dir_all(root);
}

#[test]
#[ignore = "需要显式提供 AGENTBRIDGE_REAL_CODEX_THREAD_ID；日志不输出会话正文"]
fn real_complex_session_preview_matches_native_bundle() {
    let Some(thread_id) = std::env::var("AGENTBRIDGE_REAL_CODEX_THREAD_ID").ok() else {
        return;
    };
    let mut client = CodexClient::connect().expect("本机应安装受支持的 Codex CLI");
    let recorded = client
        .read_thread_data(&thread_id)
        .expect("应通过公开接口读取指定会话");
    let timeline = convert_preview_timeline(&recorded.data, None).expect("应生成完整预览");
    let snapshot = convert_codex_items(recorded.data.clone(), None).expect("应生成兼容快照");
    let codex_home = native::codex_home().expect("应定位 CODEX_HOME");
    let rollout_path = recorded.rollout_path.expect("应返回 rollout 路径");
    let bundle = native::build_bundle(&codex_home, Path::new(&rollout_path), &snapshot)
        .expect("应构建原生会话包");
    let parsed = native::parse_bundle(&bundle).expect("应校验原生会话包");

    for (descriptor, bytes) in parsed.manifest.files.iter().zip(&parsed.files) {
        assert_eq!(descriptor.sha256, sha256_hex(bytes));
    }
    assert!(
        parsed
            .files
            .iter()
            .any(|bytes| bytes == &fs::read(&rollout_path).expect("读取源 rollout"))
    );

    println!(
        "轮次数={} 条目数={} 原生文件数={} SHA一致=true",
        timeline.turns.len(),
        timeline
            .turns
            .iter()
            .map(|turn| turn.items.len())
            .sum::<usize>(),
        parsed.files.len()
    );
}
