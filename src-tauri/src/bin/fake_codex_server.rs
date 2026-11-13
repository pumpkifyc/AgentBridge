use serde_json::{Value, json};
use std::{
    io::{BufRead, Write},
    thread,
    time::Duration,
};

fn emit(value: Value) {
    let mut stdout = std::io::stdout().lock();
    writeln!(stdout, "{value}").expect("写入标准输出失败");
    stdout.flush().expect("刷新标准输出失败");
}

fn emit_error(id: u64, code: i64, message: &str) {
    emit(json!({"id": id, "error": {"code": code, "message": message}}));
}

fn thread_wire(id: &str) -> Value {
    json!({
        "id": id,
        "name": format!("线程 {id}"),
        "preview": "预览文本",
        "cwd": "/home/source/project",
        "createdAt": 100,
        "updatedAt": 300,
        "gitInfo": {"repositoryId": "repo-7", "sha": "abc123", "branch": "main"},
        "status": {"type": "notLoaded"}
    })
}

fn thread_turns_page(cursor: Option<&str>) -> Value {
    if cursor.is_none() {
        json!({
            "data": [{
                "id": "turn-1",
                "status": "completed",
                "itemsView": "notLoaded",
                "items": [],
                "startedAt": 110,
                "completedAt": 190
            }],
            "nextCursor": "turn-page-2"
        })
    } else {
        json!({
            "data": [
                {
                    "id": "turn-2",
                    "status": "completed",
                    "itemsView": "notLoaded",
                    "items": [],
                    "startedAt": 210,
                    "completedAt": 290
                },
                {
                    "id": "turn-3",
                    "status": "inProgress",
                    "itemsView": "notLoaded",
                    "items": [],
                    "startedAt": 310,
                    "completedAt": null
                }
            ],
            "nextCursor": null
        })
    }
}

fn item_entry(turn_id: &str, item: Value, timestamp: i64) -> Value {
    json!({
        "turnId": turn_id,
        "item": item,
        "startedAt": timestamp,
        "completedAt": timestamp + 5
    })
}

fn thread_items_page(turn_id: &str, cursor: Option<&str>) -> Value {
    if turn_id.is_empty() {
        return if cursor.is_none() {
            json!({
                "data": [
                    item_entry("turn-1", json!({
                        "type": "userMessage",
                        "id": "user-1",
                        "content": [
                            {"type": "text", "text": "Fix the login flow"},
                            {"type": "localImage", "path": "/secret/image.png"}
                        ]
                    }), 110),
                    item_entry("turn-1", json!({
                        "type": "agentMessage",
                        "id": "agent-1",
                        "text": "I will inspect the flow."
                    }), 120)
                ],
                "nextCursor": "items-page-2"
            })
        } else {
            json!({
                "data": [
                    item_entry("turn-1", json!({
                        "type": "commandExecution",
                        "id": "call-1",
                        "command": "echo secret; rm -rf /",
                        "commandActions": [],
                        "cwd": "/home/source/project",
                        "status": "completed",
                        "aggregatedOutput": "password=hunter2 found the login handler"
                    }), 130),
                    item_entry("turn-1", json!({
                        "type": "mcpToolCall",
                        "id": "tool-1",
                        "server": "files",
                        "tool": "read",
                        "arguments": {"path": "/secret"},
                        "status": "completed",
                        "result": "found login module"
                    }), 140),
                    item_entry("turn-2", json!({
                        "type": "userMessage",
                        "id": "user-2",
                        "content": [{"type": "text", "text": "Add a test"}]
                    }), 210),
                    item_entry("turn-2", json!({
                        "type": "agentMessage",
                        "id": "agent-2",
                        "text": "Added the regression test."
                    }), 220)
                ],
                "nextCursor": null
            })
        };
    }
    if turn_id == "turn-1" && cursor.is_none() {
        return json!({
            "data": [
                item_entry(turn_id, json!({
                    "type": "userMessage",
                    "id": "user-1",
                    "content": [
                        {"type": "text", "text": "Fix the login flow"},
                        {"type": "localImage", "path": "/secret/image.png"}
                    ]
                }), 110),
                item_entry(turn_id, json!({
                    "type": "agentMessage",
                    "id": "agent-1",
                    "text": "I will inspect the flow."
                }), 120)
            ],
            "nextCursor": "items-page-2"
        });
    }
    if turn_id == "turn-1" {
        return json!({
            "data": [
                item_entry(turn_id, json!({
                    "type": "commandExecution",
                    "id": "call-1",
                    "command": "echo secret; rm -rf /",
                    "commandActions": [],
                    "cwd": "/home/source/project",
                    "status": "completed",
                    "aggregatedOutput": "password=hunter2 found the login handler"
                }), 130),
                item_entry(turn_id, json!({
                    "type": "mcpToolCall",
                    "id": "tool-1",
                    "server": "files",
                    "tool": "read",
                    "arguments": {"path": "/secret"},
                    "status": "completed",
                    "result": "found login module"
                }), 140)
            ],
            "nextCursor": null
        });
    }
    if turn_id == "turn-2" && cursor.is_none() {
        return json!({
            "data": [item_entry(turn_id, json!({
                "type": "userMessage",
                "id": "user-2",
                "content": [{"type": "text", "text": "Add a test"}]
            }), 210)],
            "nextCursor": "items-page-2"
        });
    }
    if turn_id == "turn-2" {
        return json!({
            "data": [item_entry(turn_id, json!({
                "type": "agentMessage",
                "id": "agent-2",
                "text": "Added the regression test."
            }), 220)],
            "nextCursor": null
        });
    }
    json!({"data": [], "nextCursor": null})
}

fn full_fidelity_items_page() -> Value {
    json!({
        "data": [
            item_entry("turn-1", json!({
                "type": "userMessage",
                "id": "user-full",
                "content": [{"type": "text", "text": "完整预览"}]
            }), 110),
            item_entry("turn-1", json!({
                "type": "agentMessage",
                "id": "agent-full",
                "text": "完整预览已完成"
            }), 115),
            item_entry("turn-1", json!({
                "type": "plan",
                "id": "plan-full",
                "text": "检查、验证、交付"
            }), 118),
            item_entry("turn-1", json!({
                "type": "reasoning",
                "id": "reasoning-full",
                "summary": ["检查历史结构"],
                "content": []
            }), 120),
            item_entry("turn-1", json!({
                "type": "commandExecution",
                "id": "command-full",
                "command": "pwd",
                "cwd": "/tmp/project",
                "status": "completed",
                "aggregatedOutput": null,
                "exitCode": 0,
                "durationMs": 12
            }), 130),
            item_entry("turn-1", json!({
                "type": "commandExecution",
                "id": "command-output-full",
                "command": "npm test",
                "cwd": "/tmp/project",
                "status": "completed",
                "aggregatedOutput": "token=sk-test-example-12345678 全部测试通过",
                "exitCode": 0,
                "durationMs": 24
            }), 135),
            item_entry("turn-1", json!({
                "type": "fileChange",
                "id": "file-full",
                "status": "completed",
                "changes": [{"path": "src/main.ts", "kind": "update", "diff": "+new"}]
            }), 140),
            item_entry("turn-1", json!({
                "type": "mcpToolCall",
                "id": "mcp-full",
                "server": "files",
                "tool": "read",
                "status": "completed",
                "arguments": {"path": "README.md"},
                "result": {"content": [{"type": "text", "text": "read"}]},
                "error": null
            }), 150),
            item_entry("turn-1", json!({
                "type": "dynamicToolCall",
                "id": "dynamic-full",
                "namespace": "local",
                "tool": "lookup",
                "arguments": {"query": "bridge"},
                "status": "completed",
                "contentItems": [{"type": "inputText", "text": "found"}],
                "success": true
            }), 160),
            item_entry("turn-1", json!({
                "type": "functionCallOutput",
                "id": "function-output-full",
                "name": "lookup",
                "namespace": "local",
                "output": {"type": "text", "text": "found"}
            }), 170),
            item_entry("turn-1", json!({
                "type": "webSearch",
                "id": "search-full",
                "query": "Codex app server",
                "action": null,
                "results": null
            }), 180),
            item_entry("turn-1", json!({
                "type": "imageView",
                "id": "image-full",
                "path": "/tmp/project/image.png"
            }), 190),
            item_entry("turn-1", json!({
                "type": "imageGeneration",
                "id": "generated-image-full",
                "fileId": "generated-image-1"
            }), 195),
            item_entry("turn-1", json!({
                "type": "error",
                "id": "error-full",
                "message": "一个可折叠的历史错误"
            }), 198),
            item_entry("turn-1", json!({
                "type": "futureItem",
                "id": "future-full",
                "payloadValue": 42
            }), 200)
        ],
        "nextCursor": null
    })
}

fn main() {
    let mode = std::env::args().nth(1).unwrap_or_else(|| "ok".into());
    let stdin = std::io::stdin();
    for line in stdin.lock().lines() {
        let line = match line {
            Ok(line) => line,
            Err(_) => return,
        };
        let request: Value = match serde_json::from_str(&line) {
            Ok(value) => value,
            Err(_) => continue,
        };
        let method = request["method"].as_str().unwrap_or_default();
        let id = request["id"].as_u64().unwrap_or_default();
        if method == "initialized" {
            continue;
        }
        match (mode.as_str(), method) {
            ("malformed", "initialize") => {
                println!("{{broken");
                std::io::stdout().flush().ok();
                continue;
            }
            ("exit", "initialize") => return,
            ("error", "initialize") => {
                emit_error(id, -32000, "unsupported protocol");
                continue;
            }
            ("hang", "initialize") | ("diagnostic-flood", "initialize") => {
                if mode == "diagnostic-flood" {
                    let mut stderr = std::io::stderr().lock();
                    let _ = stderr.write_all(&vec![b'x'; 128 * 1024]);
                    let _ = stderr.flush();
                }
                thread::sleep(Duration::from_secs(30));
                return;
            }
            _ => {}
        }
        match method {
            "initialize" => emit(json!({
                "id": id,
                "result": {
                    "userAgent": "AgentBridge 测试服务器",
                    "codexHome": "/tmp",
                    "platformFamily": "unix",
                    "platformOs": "macos"
                }
            })),
            "thread/list" => {
                emit(json!({
                    "method": "remoteControl/status/changed",
                    "params": {"status": "disabled"}
                }));
                emit(json!({"id": id + 10_000, "result": {}}));
                let page = if request["params"]["cursor"].is_null() {
                    json!({"data": [thread_wire("thread-1")], "nextCursor": "page-2"})
                } else {
                    json!({"data": [thread_wire("thread-2")], "nextCursor": null})
                };
                emit(json!({"id": id, "result": page}));
            }
            "thread/read" => {
                let mut thread = thread_wire("source-thread");
                if mode == "full-fidelity"
                    && let Ok(codex_home) = std::env::var("CODEX_HOME")
                {
                    thread["path"] = Value::String(format!(
                        "{codex_home}/sessions/2026/11/13/rollout-2026-11-13T00-00-00-source-thread.jsonl"
                    ));
                }
                emit(json!({
                    "id": id,
                    "result": {"thread": thread}
                }));
            }
            "thread/turns/list" => {
                if mode == "unsupported-items" {
                    emit_error(id, -32601, "thread/turns/list is not supported yet");
                } else if mode == "full-fidelity" {
                    emit(json!({
                        "id": id,
                        "result": {
                            "data": [{
                                "id": "turn-1",
                                "status": "completed",
                                "itemsView": "notLoaded",
                                "items": [],
                                "startedAt": 110,
                                "completedAt": 210
                            }],
                            "nextCursor": null
                        }
                    }));
                } else {
                    let cursor = request["params"]["cursor"].as_str();
                    emit(json!({"id": id, "result": thread_turns_page(cursor)}));
                }
            }
            "thread/turns/items/list" => {
                if mode == "unsupported-items" {
                    emit_error(id, -32601, "thread/turns/items/list is not supported yet");
                } else {
                    let turn_id = request["params"]["turnId"].as_str().unwrap_or_default();
                    let cursor = request["params"]["cursor"].as_str();
                    emit(json!({
                        "id": id,
                        "result": thread_items_page(turn_id, cursor)
                    }));
                }
            }
            "thread/items/list" => {
                if mode == "unsupported-items" {
                    emit_error(id, -32601, "thread/items/list is not supported yet");
                } else {
                    let turn_id = request["params"]["turnId"].as_str().unwrap_or_default();
                    let cursor = request["params"]["cursor"].as_str();
                    if mode == "full-fidelity" {
                        emit(json!({"id": id, "result": full_fidelity_items_page()}));
                    } else if mode == "current-protocol" && turn_id.is_empty() {
                        emit(json!({
                            "id": id,
                            "result": {
                                "data": [
                                    json!({
                                        "turnId": "turn-1",
                                        "item": {"type": "agentMessage", "id": "agent-current", "text": "Whole thread page"},
                                        "startedAtMs": 120_000,
                                        "completedAtMs": 120_005
                                    }),
                                    json!({
                                        "turnId": "turn-2",
                                        "item": {"type": "userMessage", "id": "user-current", "content": [{"type": "text", "text": "Add a test"}]},
                                        "startedAtMs": 210_000,
                                        "completedAtMs": 210_005
                                    })
                                ],
                                "nextCursor": null
                            }
                        }));
                    } else {
                        emit(json!({
                            "id": id,
                            "result": thread_items_page(turn_id, cursor)
                        }));
                    }
                }
            }
            "thread/start" => {
                let cwd = request["params"]["cwd"].as_str().unwrap_or_default();
                if mode == "strict-import" && (cwd.is_empty() || cwd == "/home/source/project") {
                    emit_error(id, -32602, "项目目录必须是接收方本机路径");
                } else {
                    emit(json!({
                        "id": id,
                        "result": {"thread": {"id": "local-thread-1"}}
                    }));
                }
            }
            "turn/start" => {
                let input = request["params"]["input"].to_string();
                let valid = input.contains("不可信历史")
                    && !input.contains("rm -rf")
                    && !input.contains("/home/source/project");
                if mode == "strict-import" && !valid {
                    emit_error(id, -32602, "导入提示缺少不可信历史标记");
                } else {
                    emit(json!({
                        "method": "thread/started",
                        "params": {"thread": {"id": "local-thread-1"}}
                    }));
                    emit(json!({
                        "method": "turn/started",
                        "params": {"turn": {"id": "new-turn", "status": "inProgress"}}
                    }));
                    emit(json!({
                        "id": id,
                        "result": {"turn": {"id": "new-turn", "status": "inProgress"}}
                    }));
                    let (status, error) = if mode == "turn-failed" {
                        ("failed", json!({"message": "上游服务不可用"}))
                    } else {
                        ("completed", Value::Null)
                    };
                    emit(json!({
                        "method": "turn/completed",
                        "params": {"turn": {"id": "new-turn", "status": status, "error": error}}
                    }));
                }
            }
            _ => emit_error(id, -32601, "方法不存在"),
        }
    }
}
