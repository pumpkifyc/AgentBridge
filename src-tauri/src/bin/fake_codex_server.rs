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
            "thread/read" => emit(json!({
                "id": id,
                "result": {"thread": thread_wire("source-thread")}
            })),
            "thread/turns/list" => {
                if mode == "unsupported-items" {
                    emit_error(id, -32601, "thread/turns/list is not supported yet");
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
