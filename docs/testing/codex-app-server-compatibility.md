# Codex App Server 公开接口与本机兼容性记录

日期：2026-11-13
测试环境：macOS 15.6.0、Codex CLI 0.167.2。

## 结论

Codex 提供了公开的 App Server JSON-RPC 接口。AgentBridge 使用 `initialize` / `initialized` 握手、`thread/list`、`thread/read`、`thread/turns/list`、`thread/items/list`、`thread/fork` 和 `thread/name/set`。`thread/read` 返回的 `thread.path` 指向本机 rollout 文件；原生分享会复制该文件及其 `history_base` 依赖，接收方再通过公开 `thread/fork` 创建本机会话分支。

参考：[OpenAI 官方 App Server 文档](https://developers.openai.com/codex/app-server)。本机可使用 `codex app-server generate-json-schema --experimental --out <目录>` 导出与当前 CLI 版本一致的协议 schema。

## 本机实测

- `codex app-server --stdio` 可启动并完成握手；握手后还会发送 `remoteControl/status/changed` 等通知，客户端必须按 JSON-RPC 请求 ID 关联响应。
- `thread/list` 可返回本机 Codex 会话；AgentBridge 使用 `updated_at` 倒序分页，并把每一页合并为前端会话列表。
- `thread/read` 只用于读取 thread 元数据，`thread/turns/list` 用于读取轮次。历史条目改用当前公开的 `thread/items/list`，按整个 thread 分页读取后再按 `turnId` 分组。
- `thread/read` 的 `thread.path` 可用于定位 rollout。已确认 `.jsonl` 和 `.jsonl.zst` 会出现在 `sessions/` 或 `archived_sessions/`。
- `thread/fork` 支持按 thread ID 分叉，`lastTurnId` 可把分叉截止到指定已完成轮次，`thread/name/set` 可设置会话名称。
- 在临时 `CODEX_HOME` 中复制 rollout 后，`thread/read` 可读取该会话，`thread/fork` 可创建新的原生 thread；只传 `thread/fork.path` 而不先安装文件不会自动复制 rollout。
- `thread/items/list` 当前返回条目级时间字段 `startedAtMs` 和 `completedAtMs`，单位为毫秒。AgentBridge 将其转换为界面使用的秒级时间戳，同时兼容旧的秒级字段。
- 旧接口 `thread/turns/items/list` 已不在当前导出 schema 中；对当前 App Server 调用会返回 `-32600 unknown variant`。代码不再依赖该接口。
- MCP 工具结果等 `result` 字段可能是对象而非固定字符串。快照转换会先安全序列化对象，再生成只读历史摘要，避免把结构化结果误判为空。

## 第一版行为

应用启动时检查公开接口是否可用，并把 macOS 上已验证的 Codex CLI 0.167.2 设为当前最低兼容版本。低于该版本时，界面显示“当前 Codex 版本不兼容”及升级说明；若 `thread/list` 为空或历史分页接口不受支持，也不会继续预览。

macOS 从 Finder 启动的应用不会可靠继承终端 `PATH`。AgentBridge 会依次查找 `AGENTBRIDGE_CODEX_EXECUTABLE`、进程 `PATH`、`~/.local/bin`、`~/.npm-global/bin`、Volta、Cargo、Bun、NVM 当前安装目录，以及 Homebrew 和 macOS ChatGPT/Codex 桌面版的公开内置 CLI 位置。Windows 同时查找 `PATH`、`%APPDATA%\\npm` 和 `%LOCALAPPDATA%\\npm`。版本检查与 App Server 启动共用同一个解析结果，避免界面检查通过后却启动失败。

会话列表只加载轻量元数据，并提供按标题、预览、工作目录和仓库标识即时搜索。选中会话只加载截止轮次；用户明确点击“生成分享预览”后才读取完整历史。每次预览由前端生成独立 `previewId`，Rust 将快照与会话 ID 绑定保存；切换会话、截止轮次、重新生成预览或离开页面时会取消旧令牌，迟到的异步结果也会被丢弃并清理。创建邀请必须同时提交匹配的 `threadId` 与 `previewId`，不能复用其他会话的旧预览。

全保真预览使用同一次 `thread/items/list` 分页结果生成版本化的 `PreviewTimelineV1` 和兼容的 `SnapshotV1`。时间线保留用户消息、助手回复、计划、思考、命令、文件 diff、MCP/动态工具、Web 搜索、图片、错误和未知 JSON，并按 Codex 返回顺序展示。分享页默认从最新轮次开始，每次可加载更早 3 轮；命令输出、diff、工具结果和未知 JSON 默认折叠，疑似凭据默认隐藏，用户可逐块展开本地原文。

`PreviewTimelineV1` 只经本机 Tauri IPC 返回分享方界面，不进入局域网元数据、配对信息或传输协议。局域网仍只传兼容元数据和原始 `.agbridge`；原生包继续原样保存 rollout 字节及其分叉依赖。接收方和导入页会说明折叠只影响在线预览，原始会话包仍完整保留。

分享方界面自动检测非回环、非链路本地 IPv4 地址并优先选择局域网地址。只有检测不到局域网地址时才回退到 `127.0.0.1`，并明确提示该地址只适合本机测试。Rust 命令错误以结构化 `{ code, message }` 返回，前端提取中文 `message` 显示，不再出现 `[object Object]`。

接收方保留通过有效分享收到的 `.agbridge` 原生会话包；导入时先安装 rollout，再使用公开 `thread/fork`、`lastTurnId` 和 `thread/name/set` 创建本地分支。核心会话读写通过 App Server 完成。由于 `thread/fork` 创建的文件不会总被桌面版索引立即发现，AgentBridge 会在本机 Codex 状态索引中同步标题、预览和用户会话可见性字段；该兼容步骤只修改接收方本机索引，不上传数据。

发布前需要在同一台机器上保证 Codex CLI 与桌面版来自同一受支持版本，并重新验证：列出会话、选择截止轮次、读取消息、构建原生包、`thread/fork`、桌面版历史可见性和后续轮次。

## 局域网传输

第一版传输协议版本为 1。分享方每次创建一个短时邀请：随机 256-bit Bearer 凭据、临时自签名 TLS 证书及其 SHA-256 指纹、5 分钟有效期。接收方需要在配对信息中同时获得凭据与证书指纹；TLS 握手固定校验证书指纹，握手后才发送凭据。接收方先只获取标题、项目、消息数和摘要预览警告等元数据，明确批准后才开始分块传输。原生会话包最大 256 MiB，逐块确认并以最终 SHA-256 摘要校验。错误凭据、过期、撤销、取消、重放、指纹不匹配和连接中断都会拒绝或保留为未完成状态。

邀请只允许一个活跃传输连接。预览或分块传输中断后，服务端把邀请标记为“失败”，但不消耗凭据；只要邀请尚未过期或被撤销，接收方仍可用相同配对信息重新预览和传输。明确拒绝、完整传输被接收方确认、撤销和取消仍会消耗凭据。完整传输后会停止监听并移除临时证书；过期或分享方停止监听时同样会清理证书，分享方状态轮询还会移除已停止的服务端句柄。服务端显示“传输完成”只表示快照已校验送达，和接收方创建本机 thread 后的“已导入”是两个独立状态。

分享方界面显示可复制的配对 JSON 和二维码。配对信息包含 `endpoint`、`secret`、`fingerprint`、`verificationCode` 和 `expiresAt`，其中 `secret` 是一次性凭据；界面错误信息使用固定中文说明，不把凭据或会话正文写入日志。配对二维码仅供可信渠道短时传递，接收方仍需核对验证码和证书指纹。

## 接收、暂存与取消

接收方粘贴完整配对 JSON 后，前端和 Rust 命令都会检查字段、证书指纹格式，以及 `verificationCode` 是否由 `sha256:<64 位十六进制指纹>` 按 SHA-256 派生；不一致时不会建立连接。预览响应只含标题、项目、消息数、截止轮次、摘要预览警告、大小和分块数。用户明确接受后才下载原生会话包，逐块报告进度，并在写入接收方应用数据目录的 `staged-snapshots/` 前校验整体摘要和原生包结构。此时不会创建 Codex thread。

取消会中止正在运行的下载任务，并阻止迟到的结果被标记为成功。传输中断可显式重试；删除暂存会移除本机文件和内存记录。成功暂存结果只返回分享标识、端点、指纹、元数据和原生包路径，不再把一次性 `secret` 回传给界面。

## 本机原生会话与导入记录

接收方在导入页面通过 Tauri 原生目录选择器选择自己的项目目录，也可手动输入；分享方工作目录和本机暂存会话包路径都不能当成项目目录。导入命令只读取应用数据目录内、以分享标识哈希命名且再次通过完整性与路径校验的 `.agbridge` 原生会话包。

`ImportService` 先把原始 rollout 安装到本机 `CODEX_HOME`，再通过公开 `thread/fork` 创建新的本机 thread，`cwd` 使用接收方目录，并拒绝复用分享方 thread ID。成功、待处理和失败状态连同重试次数和错误保存在接收方应用数据目录的 SQLite `imports` 表中；失败时保留原生会话包以便重试。删除记录会删除记录及受应用数据目录约束的暂存会话包，不删除 Codex thread。

原生包可能包含原始工具输出、文件路径、系统提示或其他敏感内容；界面会在接收和导入前提示用户只处理可信来源。导入不会重新执行历史工具调用。

目前没有可依赖的公开 deep link 能直接在 Codex 桌面版打开指定 thread。打开操作先通过公开 `thread/list` 检查本机会话是否存在；自动打开失败时提示用户重启 Codex 并在历史列表中查找该会话。

### 双平台待验收事项

当前 macOS 环境已确认 Codex CLI 0.167.2 的会话列表、历史分页、rollout 路径和 `thread/fork` 可用，并在临时 `CODEX_HOME` 完成原生导入验证。发布前仍需在 macOS 和 Windows 上分别使用版本匹配的 Codex CLI 与桌面版，执行「安装 rollout → `thread/fork` → 桌面版历史可见 → 继续新轮次」，记录双平台最低测试版本和状态索引兼容性。

本轮全保真预览只在 macOS 构建并安装。Windows 代码路径保持编译兼容，Windows 安装包按当前范围暂缓，待后续明确要求后单独构建和验收。
