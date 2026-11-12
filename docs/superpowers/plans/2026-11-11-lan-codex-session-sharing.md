# AgentBridge 局域网 Codex 会话分享实施计划

> **供自动化执行者使用：** 实施时必须按任务逐项执行本计划。建议使用 `superpowers:subagent-driven-development` 子技能，或使用 `superpowers:executing-plans` 子技能。请用复选框（`- [ ]`）跟踪步骤。

**目标：** 构建一款 macOS 和 Windows 版 AgentBridge 应用，让用户通过经过配对的局域网连接传输明确选定的 Codex 会话，并在接收方本机创建独立的 Codex 续接会话。

**架构：** Tauri 2 应用包含 React/TypeScript 界面和 Rust 领域服务。Rust 端负责管理 Codex App Server 进程、版本化快照转换、点对点加密传输、输入校验和本地导入记录。接收方选择自己的项目代码目录；AgentBridge 在该目录下启动新的 Codex thread，并把导入的对话作为明确标记的历史上下文。无需云端中继，也不使用未公开的桌面版存储接口。

**技术栈：** Tauri 2、Rust、React、TypeScript、通过 stdio 进行 JSON-RPC 的 Codex App Server、本地导入记录使用 SQLite、局域网传输使用 TLS、通过 Tauri bundler 打包各平台应用。

**规格文档：** `docs/superpowers/specs/2026-11-11-lan-codex-session-sharing-design.md`

## 全局约束

- 产出 macOS 和 Windows 桌面应用。
- 每次分享只传输一个不可变快照，使用局域网直连，不使用托管中继服务。
- 分享方发起的配对必须有时效、只能使用一次；传输前必须由接收方明确确认。
- 绝不传输凭据、环境变量、本地 Codex 配置、源代码文件或可执行工具请求。
- 绝不重放历史工具调用，也不修改分享方的 thread。
- 使用接收方选择的本地项目代码目录；分享方绝对路径仅作为说明信息。
- 绝不写入未公开的 Codex 桌面版数据库或内部文件。
- 如果 Codex 桌面版不支持列出或继续新会话，则保留上下文，并提供可直接粘贴或打开 Codex 的后备方案。

## 重点复核风险

- 格式错误或过大的快照：在渲染或持久化之前拒绝；通过解析器上限和 schema 校验测试验证。
- 未认证、过期、重放或已撤销的配对：在泄露元数据或内容前拒绝；覆盖所有状态测试。
- 不可信的对话指令和历史工具调用：按无操作的文本渲染，并作为外部历史上下文传入；测试确认不会重放工具调用。
- 传输中断或数据块损坏：不得标记为已导入；允许用户从仍有效的分享中明确重试；测试完整性和状态转换。
- 不同平台上的 Codex 进程与协议差异：显示可执行的兼容性说明并保留快照；在 macOS 和 Windows 上测试支持版本与最低版本。

---

## 仓库结构

当前工作区为空，也不是 Git 仓库。请在任务 1 中初始化项目。文件按职责拆分：

```text
src/                         React 页面和界面状态
src-tauri/src/               Rust 应用入口和 Tauri 命令
src-tauri/src/codex/         App Server 进程和 JSON-RPC 适配器
src-tauri/src/snapshot/      版本化快照 schema 和转换逻辑
src-tauri/src/pairing/       邀请生命周期和对端认证
src-tauri/src/transfer/      局域网传输和完整性校验
src-tauri/src/imports/       接收方导入流程和 SQLite 记录
src-tauri/tests/             使用伪造 Codex Server 的 Rust 集成测试
src/**/*.test.tsx            界面行为测试
e2e/                         打包应用的跨平台验收检查
```

## Task 1：初始化跨平台桌面应用

**文件：**
- 新建：`package.json`、`vite.config.ts`、`tsconfig.json`、`index.html`
- 新建：`src/main.tsx`、`src/App.tsx`、`src/styles.css`
- 新建：`src-tauri/Cargo.toml`、`src-tauri/tauri.conf.json`、`src-tauri/src/main.rs`、`src-tauri/src/lib.rs`
- 新建：`.gitignore`、`README.md`

**接口：**
- 可运行的 Tauri 桌面窗口，包含占位首页和应用标识。
- 开发命令：`npm run tauri dev`；发布构建命令：`npm run tauri build`。

- [x] 生成启用 macOS 和 Windows 目标平台的 Tauri 2 React/TypeScript 项目框架。
- [x] 添加冒烟测试，打开应用后检查窗口标题和空状态标题。
- [x] 运行 `npm run build` 和 `cargo check --manifest-path src-tauri/Cargo.toml`。
- [ ] 在对应原生 Runner 上构建平台安装包；macOS 本地包已生成，Windows 包待 Windows 原生 Runner 验证；开发构建与正式签名、公证的要求分开记录。

## Task 2：定义并校验快照协议

**文件：**
- 新建：`src-tauri/src/snapshot/mod.rs`
- 新建：`src-tauri/src/snapshot/schema.rs`
- 新建：`src-tauri/src/snapshot/validate.rs`
- 新建：`src-tauri/src/snapshot/convert.rs`
- 测试：`src-tauri/src/snapshot/tests.rs`

**接口：**
- `SnapshotV1 { metadata, messages, warnings }` 为序列化传输对象。
- `SnapshotMessage { role, text, turn_index, timestamp, historical_tool_summary }` 只包含可展示的文本。
- `validate_snapshot(bytes: &[u8]) -> Result<SnapshotV1, SnapshotError>` 校验 schema 版本；序列化数据最大 20 MiB；最多 10,000 条消息；单个文本字段最大 1 MiB；时间顺序必须有效。
- `convert_codex_items(thread: ThreadData, cutoff_turn_id: Option<String>) -> Result<SnapshotV1, SnapshotError>` 排除二进制内容、凭据/配置和可执行工具参数；对不支持或被截断的条目添加警告。

- [x] 为有效的有序对话、较早的已完成轮次截止、不支持条目警告、工具调用排除、分享方路径只作为元数据，以及消息正文和工具结果中常见 API Key/token/password 格式脱敏编写测试。
- [x] 编写未知 schema 版本、格式错误 JSON、超大数据、消息数量超限、单条消息超长和顺序无效的拒绝测试。
- [x] 实现带类型的 serde 模型，并用文本条目白名单进行转换；持久化快照前过滤已知凭据格式，若发生脱敏则在预览中提示。
- [x] 运行 `cargo test --manifest-path src-tauri/Cargo.toml snapshot`。

## Task 3：读取本机 Codex 会话

**文件：**
- 新建：`src-tauri/src/codex/mod.rs`
- 新建：`src-tauri/src/codex/client.rs`
- 新建：`src-tauri/src/codex/protocol.rs`
- 新建：`src-tauri/src/codex/models.rs`
- 新建：`src-tauri/src/codex/error.rs`
- 测试：`src-tauri/tests/codex_app_server.rs`

**接口：**
- `CodexClient::connect() -> Result<CodexClient, CodexError>` 通过 stdio 启动已安装的 `codex app-server`，并完成 initialize/initialized 握手。
- `CodexClient::list_threads(cursor: Option<String>) -> Result<Page<ThreadSummary>, CodexError>` 分页列出本机 threads。
- `CodexClient::read_snapshot(thread_id: &str, cutoff_turn_id: Option<&str>) -> Result<SnapshotV1, CodexError>` 分页读取 turns/items，并调用任务 2 的转换逻辑。
- Codex 子进程生命周期由 client 管理；stdout 仅用于 JSON-RPC，stderr 仅保存有大小上限的诊断信息。

- [x] 创建伪造的换行分隔 JSON-RPC App Server 测试程序，覆盖握手、分页、找不到可执行文件、协议错误和进程退出。
- [x] 实现请求 ID 关联、分页、超时、取消和结构化错误映射。
- [x] 验证启动 Codex 时不使用 shell 字符串插值。
- [ ] 运行 `cargo test --manifest-path src-tauri/Cargo.toml codex`，并在本机 Codex 安装上手动读取会话；伪造 App Server 测试已通过，本机版本匹配后的手动验证待发布前完成。

## Task 4：实现分享方会话选择和预览

**文件：**
- 新建：`src/features/share/SharePage.tsx`
- 新建：`src/features/share/ThreadPicker.tsx`
- 新建：`src/features/share/ConversationPreview.tsx`
- 新建：`src/features/share/shareApi.ts`
- 新建：`src/features/share/shareTypes.ts`
- 测试：`src/features/share/SharePage.test.tsx`
- 修改：`src/App.tsx`

**接口：**
- Tauri 命令：`codex_status`、`list_threads`、`preview_snapshot(thread_id, cutoff_turn_id)`。
- 界面状态：加载中、可用、空列表、不支持、出错。
- `ConversationPreview` 显示源标题、项目名称/路径元数据、选择的截止轮次、消息顺序和警告；路径需明确标注为说明信息。

- [x] 测试加载状态、空列表、会话选择、截止轮次选择、预览警告和 Codex 不可用错误。
- [x] 实现带类型的 Tauri 命令调用与会话预览，不暴露原始工具参数。
- [x] 运行 `npm test -- --run src/features/share/SharePage.test.tsx` 和 `npm run build`。

## Task 5：实现局域网邀请和加密传输

**文件：**
- 新建：`src-tauri/src/pairing/mod.rs`
- 新建：`src-tauri/src/pairing/invitation.rs`
- 新建：`src-tauri/src/transfer/mod.rs`
- 新建：`src-tauri/src/transfer/server.rs`
- 新建：`src-tauri/src/transfer/client.rs`
- 新建：`src-tauri/src/transfer/protocol.rs`
- 新建：`src-tauri/src/transfer/integrity.rs`
- 测试：`src-tauri/tests/lan_transfer.rs`

**接口：**
- `InvitationManager::create(snapshot_id) -> Invitation` 返回局域网 endpoint、一次性 256-bit CSPRNG secret、由临时 TLS 证书指纹派生的短验证码和过期时间；默认 5 分钟后过期。
- `InvitationManager::revoke(invitation_id)` 立即使未使用的邀请失效。
- `TransferServer::serve(invitation, snapshot)` 仅绑定到选择的局域网接口，使用 rustls 和临时自签名服务端证书（由邀请信息中的指纹固定验证），通过 TLS 承载一次性 Bearer 凭据认证，对失败认证限速，在内容传输前等待接收方批准，并发送有大小限制的数据块及最终 SHA-256 摘要。
- `TransferClient::preview(endpoint, invitation_secret) -> ShareMetadata` 只返回分享方/项目/快照元数据；`accept()` 下载并校验快照。

- [x] 编写 loopback 集成测试，覆盖证书指纹固定验证、有效配对、元数据预览、接收方拒绝/接受、重放、过期、撤销、凭据错误、限速、摘要损坏和连接中断。
- [x] 使用 `rustls` 和维护中的异步网络库实现有版本号的传输协议；256-bit 邀请凭据必须由操作系统 CSPRNG 生成，禁止自行实现密码学原语。
- [x] 只绑定到所选局域网接口，并提供明确的取消/过期状态转换。
- [x] 运行 `cargo test --manifest-path src-tauri/Cargo.toml lan_transfer`。

## Task 6：构建配对与分享方界面

**文件：**
- 新建：`src/features/share/PairingPanel.tsx`
- 新建：`src/features/share/ShareStatus.tsx`
- 新建：`src/features/share/pairingApi.ts`
- 测试：`src/features/share/PairingPanel.test.tsx`
- 修改：`src/features/share/SharePage.tsx`

**接口：**
- Tauri 命令：`start_share(snapshot)`、`revoke_share(invitation_id)`、`share_status(invitation_id)`、`cancel_share(invitation_id)`。
- 分享方状态：等待、已配对、预览中、传输中、已导入、已过期、已取消、失败。

- [x] 测试所有状态转换、取消、过期显示和可重试的传输错误。
- [x] 实现创建分享、显示连接地址/二维码或手动连接信息、显示验证码及立即撤销控制。
- [x] 确认前端日志和错误信息中不包含邀请凭据或会话正文。
- [x] 运行分享方界面测试和 `npm run build`。

## Task 7：接收、预览并批准分享快照

**文件：**
- 新建：`src/features/receive/ReceivePage.tsx`
- 新建：`src/features/receive/ShareApproval.tsx`
- 新建：`src/features/receive/TransferProgress.tsx`
- 新建：`src/features/receive/receiveApi.ts`
- 测试：`src/features/receive/ReceivePage.test.tsx`
- 修改：`src/App.tsx`

**接口：**
- Tauri 命令：`preview_share(endpoint, secret)`、`accept_share(share_id)`、`decline_share(share_id)`、`cancel_transfer(transfer_id)`。
- 预览包括分享方身份、项目/仓库元数据、消息数、截止轮次、不支持/截断警告，以及明确的接受/拒绝操作。
- 快照校验后先暂存本地；只有接收方确认项目并按下“导入”后才创建 Codex thread。

- [ ] 测试先预览再下载、拒绝、错误元数据、不支持内容警告、进度、取消和中断后重试。
- [ ] 实现接收方确认和快照本地暂存，并提供明确的重试/删除操作。
- [ ] 运行接收界面测试和快照校验测试。

## Task 8：创建接收方本机 Codex 续接会话

**文件：**
- 新建：`src-tauri/src/imports/mod.rs`
- 新建：`src-tauri/src/imports/create_thread.rs`
- 新建：`src-tauri/src/imports/history.rs`
- 新建：`src-tauri/src/imports/prompt.rs`
- 新建：`src-tauri/src/imports/store.rs`
- 新建：`src-tauri/src/db.rs`
- 测试：`src-tauri/tests/import_thread.rs`
- 新建：`src/features/import/ProjectPicker.tsx`
- 新建：`src/features/import/ImportReview.tsx`
- 新建：`src/features/import/ImportHistory.tsx`
- 测试：`src/features/import/ImportReview.test.tsx`

**接口：**
- `ImportService::create_local_thread(snapshot, project_path) -> Result<ImportRecord, ImportError>` 通过本地 App Server 启动新 thread，将 `cwd` 设置为 `project_path`，并用一条有长度上限的用户消息导入对话；提示必须把 transcript 全部标为历史且不可信。
- `ImportRecord { id, source_title, local_thread_id, project_path, imported_at, snapshot_path, status }` 保存于 SQLite。
- `CodexOpener::open_thread(local_thread_id) -> Result<(), OpenError>` 在可用时使用受支持的系统启动或 deep link；否则返回已说明的后备方案，展示可直接粘贴的续接提示。
- 数据库操作包括列表、重试和删除；删除会移除本地快照和导入记录。除非另行确认，不删除 Codex thread。

- [ ] 使用伪造 App Server 测试在所选代码目录中创建 thread、源 ID 不复用、历史工具不执行、不可信上下文包装、thread 创建失败和打开 Codex 的后备方案。
- [ ] 添加 SQLite migration，并测试导入历史、重试元数据和删除行为。
- [ ] 实现项目目录选择/确认；绝不把分享方绝对路径用作本地路径。
- [ ] 验证新建 thread 在 macOS 和 Windows Codex 桌面版中是否可见且可继续；记录最低测试版本，不支持时启用后备方案。
- [ ] 运行 `cargo test --manifest-path src-tauri/Cargo.toml imports` 和导入界面测试。

## Task 9：导入历史、重试与数据删除

**文件：**
- 修改：`src-tauri/src/imports/history.rs`
- 修改：`src-tauri/src/imports/store.rs`
- 新建：`src/features/history/HistoryPage.tsx`
- 新建：`src/features/history/HistoryRow.tsx`
- 测试：`src/features/history/HistoryPage.test.tsx`

**接口：**
- Tauri 命令：`list_imports()`、`retry_import(import_id)`、`delete_import(import_id)`。
- 历史页面显示已完成、中断、失败和待处理的导入；默认不展示会话正文。

- [ ] 测试失败导入重试、打开已成功导入、删除记录与暂存快照，以及本地 Codex thread 丢失时的安全处理。
- [ ] 实现导入历史页面和删除确认。
- [ ] 运行历史测试及完整 Rust/前端测试。

## Task 10：跨平台打包与验收

**文件：**
- 修改：`src-tauri/tauri.conf.json`
- 新建：`.github/workflows/build.yml`
- 新建：`docs/testing/cross-platform-acceptance.md`
- 新建：`e2e/share-import-checklist.md`

**接口：**
- CI 在 macOS 和 Windows runner 上构建未签名的开发制品。
- 手工验收矩阵记录分享方系统、接收方系统、Codex 版本、配对、传输、导入、桌面版可见性、后备方案和清理结果。

- [ ] 配置 Tauri 安装包和 CI macOS/Windows 任务，不提交签名密钥或其他秘密。
- [ ] 在两个 runner 上运行静态分析、Rust 测试、前端测试和发布构建。
- [ ] 双向执行验收流程：macOS 分享到 Windows，以及 Windows 分享到 macOS。
- [ ] 两个平台都验证错误/过期配对、拒绝分享、传输中断、重试、Codex 缺失和桌面版可见性后备方案。
- [ ] 记录已知平台限制和最低测试 Codex 版本。

## 依赖顺序

任务 1 是其他任务的前置条件。任务 2 是任务 3 和任务 5 的前置条件。任务 3 是任务 4 和任务 8 的前置条件。任务 5 是任务 6 和任务 7 的前置条件。任务 6 和任务 7 是任务 8 的前置条件。任务 8 是任务 9 的前置条件。任务 10 在任务 1 至 9 集成后进行验证。

## 最终自检

- 规格覆盖情况：会话列表/预览/截止轮次（任务 2 至 4）；配对、加密、确认、过期/撤销（任务 5 至 7）；接收方本地代码目录和独立续接（任务 8）；本地历史/重试/删除（任务 9）；隐私排除项、快照大小限制和不支持内容警告（任务 2、5、7）；故障和中断恢复（任务 3、5、7 至 10）；macOS/Windows 打包及桌面版兼容性（任务 8、10）。
- 跨任务接口统一使用 `SnapshotV1`、`Invitation` 和 `ImportRecord`。
- 安全敏感的传输算法在任务 5 中以具体测试为前提；密码学原语必须使用维护中的库。
- 没有任务会写入未公开的 Codex 桌面版存储或传输凭据/配置。
- 当前工作区不是 Git 仓库，因此没有提交步骤；如果项目需要通过 Git 维护，请在实施前初始化版本控制。
