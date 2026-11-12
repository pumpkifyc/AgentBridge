# Codex App Server 公开接口与本机兼容性记录

日期：2026-11-12  
测试环境：macOS 15.6.0、Codex CLI 0.142.2、Codex 桌面版会话由更高版本写入。

## 结论

Codex 提供了公开的 App Server JSON-RPC 接口。官方文档说明了 `initialize` / `initialized` 握手、`thread/list`、`thread/read`、`thread/start` 和 `turn/start`，并明确要求使用 `thread/turns/list` 与历史条目分页接口读取会话历史。AgentBridge 只调用这些公开接口，不读取桌面版未公开数据库或内部文件。

参考：[OpenAI 官方 App Server 文档](https://developers.openai.com/codex/app-server)。本机可使用 `codex app-server generate-json-schema --out <目录>` 导出与当前 CLI 版本一致的协议 schema；历史分页接口需要 `--experimental` 才会出现在导出的 schema 中。

## 本机实测

- `codex app-server --stdio` 可启动并完成握手；握手后还会发送 `remoteControl/status/changed` 等通知，客户端必须按 JSON-RPC 请求 ID 关联响应。
- `thread/list` 返回空列表，即使本机桌面版状态库已有 282 条会话记录；这与 CLI 0.142.2 和桌面状态库由更高版本写入的版本差异有关。
- 对已知 thread ID 调用 `thread/read` 可以读取元数据。`includeTurns: true` 返回的 turns 的 `items` 均为空；`thread/turns/list` 可分页返回 turn 元数据。
- 本机 0.142.2 的 `thread/turns/items/list` 返回 `-32601 is not supported yet`。因此 AgentBridge 在读取历史条目时明确报出兼容性错误，不把空历史误当成完整快照。
- `thread/fork` 创建的新 thread 会写入本机 state DB，但 `thread/read` 仍未返回历史 items。该行为不能作为完整的会话导入方案。

## 第一版行为

应用启动时检查公开接口是否可用。若 `thread/list` 为空或历史分页接口不受支持，界面显示“当前 Codex 版本不兼容”及建议升级到匹配的 Codex 桌面版/CLI。不会静默读取内部 SQLite，也不会改动桌面版数据库。

接收方仍可保留通过有效分享收到的快照；创建本地续接会话使用 `thread/start` 和 `turn/start`，把快照作为一条明确标注的不可信历史用户消息传入。如果本机 Codex 不支持这些接口，应用保留快照与可直接粘贴的续接提示，由用户在 Codex 中手动新建会话。

发布前需要在同一台机器上保证 Codex CLI 与桌面版来自同一受支持版本，并重新验证：列出会话、选择截止轮次、读取消息、创建 thread、后续在桌面版历史中打开续接会话。
