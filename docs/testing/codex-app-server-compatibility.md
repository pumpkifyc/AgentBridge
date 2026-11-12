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

## 局域网传输

第一版传输协议版本为 1。分享方每次创建一个短时邀请：随机 256-bit Bearer 凭据、临时自签名 TLS 证书及其 SHA-256 指纹、5 分钟有效期。接收方需要在配对信息中同时获得凭据与证书指纹；TLS 握手固定校验证书指纹，握手后才发送凭据。接收方先只获取标题、项目、消息数和警告等元数据，明确批准后才开始分块传输。快照最大 20 MiB，逐块确认并以最终 SHA-256 摘要校验。错误凭据、过期、撤销、取消、重放、指纹不匹配和连接中断都会拒绝或保留为未完成状态。

邀请只允许一个活跃传输连接。预览或分块传输中断后，服务端把邀请标记为“失败”，但不消耗凭据；只要邀请尚未过期或被撤销，接收方仍可用相同配对信息重新预览和传输。明确拒绝、完整传输被接收方确认、撤销和取消仍会消耗凭据。完整传输后会停止监听并移除临时证书；过期或分享方停止监听时同样会清理证书，分享方状态轮询还会移除已停止的服务端句柄。服务端显示“传输完成”只表示快照已校验送达，和接收方创建本机 thread 后的“已导入”是两个独立状态。

分享方界面显示可复制的配对 JSON 和二维码。配对信息包含 `endpoint`、`secret`、`fingerprint`、`verificationCode` 和 `expiresAt`，其中 `secret` 是一次性凭据；界面错误信息使用固定中文说明，不把凭据或会话正文写入日志。配对二维码仅供可信渠道短时传递，接收方仍需核对验证码和证书指纹。

## 接收、暂存与取消

接收方粘贴完整配对 JSON 后，前端和 Rust 命令都会检查字段、证书指纹格式，以及 `verificationCode` 是否由 `sha256:<64 位十六进制指纹>` 按 SHA-256 派生；不一致时不会建立连接。预览响应只含标题、项目、消息数、截止轮次、警告、大小和分块数。用户明确接受后才下载会话正文，逐块报告进度，并二次校验快照后写入接收方应用数据目录的 `staged-snapshots/`。此时只显示“已暂存，等待选择本地项目后导入”，不会创建 Codex thread。

取消会中止正在运行的下载任务，并阻止迟到的结果被标记为成功。传输中断可显式重试；删除暂存会移除本机文件和内存记录。成功暂存结果只返回分享标识、端点、指纹、元数据和快照路径，不再把一次性 `secret` 回传给界面。
