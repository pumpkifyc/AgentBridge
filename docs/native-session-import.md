# Codex 原生会话包与导入机制

更新日期：2026-11-13

## 目标

AgentBridge 分享的是 Codex 可继续读取的原始会话记录，不再把历史拼接成一条约 100 万字符的提示。接收方导入后，Codex 会显示真正的历史轮次、工具记录和会话结构，并可在接收方自己的项目目录中继续对话。

## 原生会话包

原生包扩展名为 `.agbridge`，结构为：

```text
AGBRIDGE_NATIVE_BUNDLE_V1
manifest 长度（4 字节小端序）
JSON manifest
一个或多个 rollout 文件原始字节
```

Manifest 记录源 thread/session ID、标题、时间、截止轮次、源工作目录、消息数、预览文本，以及每个 rollout 的相对路径、大小和 SHA-256。构建时会从 `thread/read` 返回的 rollout 路径开始，递归收集 `history_base.thread_id` 指向的依赖文件；因此从已分叉会话继续分享时不会丢失更早的父历史。

限制如下：

- 单个原生包最大 256 MiB。
- 最多打包 64 个 rollout 文件。
- 文件只能来自 `CODEX_HOME/sessions` 或 `CODEX_HOME/archived_sessions`。
- 接收方会校验清单、相对路径、文件数量、每文件 SHA-256、整体传输摘要和源 thread 一致性。

## 与本机预览的关系

分享方点击“生成分享预览”时，AgentBridge 只读取一次公开 App Server 历史，并同时生成两份结果：

- `PreviewTimelineV1`：仅用于分享方本机界面，保留事件顺序、命令、diff、工具调用、错误、未知 JSON 和敏感原文标记。
- `SnapshotV1` 与 `.agbridge`：前者维持现有局域网兼容协议，后者原样保存 rollout 文件及其 `history_base` 依赖。

预览中的折叠和隐藏只影响展示。敏感块必须由用户逐块点击后才会进入页面 DOM；`PreviewTimelineV1` 不会进入配对信息、局域网元数据或传输协议。原生包中的 rollout 字节不会因预览处理而重写。

自动化集成测试在临时 `CODEX_HOME` 中构造包含父会话依赖的复杂历史，验证用户、助手、计划、思考、命令、diff、MCP、动态工具、Web 搜索、图片、错误和未知条目均进入时间线，同时核对包内每个 rollout 的大小、SHA-256 和原始字节。可选的真实本机会话测试只在显式设置 `AGENTBRIDGE_REAL_CODEX_THREAD_ID` 后运行，日志只输出类别和数量。

## 导入流程

1. 接收方将原生包保存到应用数据目录的 `staged-snapshots/`，文件名由分享 ID 的 SHA-256 派生。
2. 选择接收方本机项目目录后，解析并校验原生包。
3. 将 rollout 文件安装到接收方 `CODEX_HOME`。已存在且内容完全相同时复用；同路径不同内容会拒绝覆盖。
4. 启动接收方本机 `codex app-server --stdio`，调用公开 `thread/fork`。
5. 通过 `lastTurnId` 将分叉截止到分享时选择的轮次，并用 `thread/name/set` 设置源会话标题。
6. 在接收方本机同步 Codex 会话索引的标题、预览和用户会话可见性，使新分支能出现在桌面版历史中。

分叉会生成新的本机 thread ID，原 thread ID 只作为只读来源。`cwd` 使用接收方选择的项目目录，后续轮次不会在分享方目录中执行。

## 兼容性与安全边界

- Codex CLI 最低兼容版本为 0.167.2；该版本已公开 `thread/fork`、`lastTurnId`、`thread/read` 和 `thread/name/set`。
- 已在 macOS 使用临时 `CODEX_HOME` 验证“复制 rollout → `thread/fork` → 设置名称”的完整链路。
- 历史工具调用作为既有记录展示，不会因导入而重新执行。
- 原生包保留原始 rollout，可能含工具输出、文件路径、系统提示、环境信息或其他敏感文本。界面会在接收前提示用户只导入可信来源。
- AgentBridge 不上传配对凭据、Codex 登录凭据或配置，但无法保证会话正文中从未出现过敏感信息。
- 删除 AgentBridge 导入记录只会删除 SQLite 记录和暂存原生包，不会删除已经创建的 Codex thread。

## 当前限制

Codex 桌面版没有公开 deep link 可直接定位任意 thread。AgentBridge 可通过公开 `thread/list` 检查会话是否存在，并尝试系统打开方式；若桌面版没有立即刷新，重启 Codex 后会在历史列表中出现该原生会话。

当前版本完成 macOS 构建与安装验收。Windows 保持代码兼容，但本轮不生成 Windows 安装包。
