# Codex 全保真会话预览实施计划

> **供自动化执行者使用：** 实施本计划时必须逐项执行，并使用 `superpowers:executing-plans`；本计划已选择 Native 执行方式。请用复选框（`- [ ]`）跟踪步骤。

**目标：** 让分享方在 AgentBridge 中以接近 Codex 的时间线核对完整会话，同时确保 `.agbridge` 原生包继续原样保留 rollout，接收方仍可像普通 Codex 会话一样导入并续接。

**架构：** 从公开 App Server 的一次历史读取结果生成两份数据：兼容局域网协议的 `SnapshotV1`，以及只用于本机分享页的 `PreviewTimelineV1`。`PreviewTimelineV1` 保留事件顺序、工具调用、diff、原始 JSON 和敏感原文，由本机 Tauri IPC 返回；局域网仍只传输 `SnapshotV1` 元数据和原始 `.agbridge`。

**技术栈：** Tauri 2、Rust 2024、Serde、React 19、TypeScript、Vitest、Codex App Server JSON-RPC。

**规格：** [2026-11-13-full-fidelity-conversation-preview-design.md](../specs/2026-11-13-full-fidelity-conversation-preview-design.md)

## 全局约束

- `.agbridge` 原生包必须包含截止轮次及分叉依赖的原始 rollout 文件，字节内容不得因预览折叠或敏感标记而改变。
- `PreviewTimelineV1` 只允许进入本机 Tauri IPC 和分享方 UI，不得进入局域网分享元数据或传输协议。
- `SnapshotV1` 的交易格式和版本保持兼容；本轮不把它扩张成完整正文协议。
- 预览保留用户消息、助手回复、计划、思考、命令、文件修改、MCP/动态工具、Web 搜索、图片引用、未知条目和错误。
- 用户消息和助手最终回复默认展开；思考、命令输出、diff、工具参数/结果、未知 JSON 和错误详情默认折叠。
- 疑似凭据默认隐藏，但用户点击“显示原文”后必须能核对完整预览文本；检测不等于清除。
- 所有折叠提示必须带 `scope: display_only`，文案必须明确“只影响预览，原生分享包仍完整保留”。
- 原有原生包 256 MiB 上限保持不变；超过时明确失败，不得裁剪 rollout。
- 所有会话正文按纯文本渲染，禁止使用 `innerHTML`，禁止执行历史工具调用或未知条目中的任何内容。
- 日志和测试输出不得打印会话正文、命令输出、diff、凭据、配对 secret 或原始 JSON。
- macOS 使用临时 `CODEX_HOME` 完成自动化验证；不得修改真实 `~/.codex`。
- 本轮不制作或更新 Windows 安装包；保留跨平台代码编译兼容。
- 完成后运行 `npm run tauri:install`，自动安装最新 macOS 构建并重新打开应用。

## 重点复核风险

- 未知或未来版本的 Codex 条目被反序列化后消失：必须保留原始 JSON，并由测试覆盖未知类型。
- 敏感原文仍存在于本地预览中：UI 必须默认隐藏、单块解锁，日志和局域网响应不得包含它。
- 复杂会话包含上千个条目时首次渲染明显卡顿：必须默认只加载最新轮次、按轮次分批显示并折叠大块内容。
- 截止轮次、rollout 路径和预览令牌不一致时错误创建邀请：必须在注册和邀请阶段同时拒绝。
- 新时间线被误带入接收方确认或传输协议：必须对 Tauri 响应和 LAN 元数据分别做结构测试。

---

## 文件结构与职责

- `src-tauri/src/codex/models.rs`：公开 App Server 读取后的线程、轮次和原始条目模型。
- `src-tauri/src/codex/client.rs`：分页读取、时间戳兼容和连接生命周期；不负责展示规范。
- `src-tauri/src/preview/schema.rs`：`PreviewTimelineV1`、轮次、条目、块和提示的数据结构。
- `src-tauri/src/preview/convert.rs`：把原始条目规范化为 Codex 风格时间线。
- `src-tauri/src/preview/sensitive.rs`：识别疑似凭据并标记，不删除原文。
- `src-tauri/src/snapshot/convert.rs`：继续生成兼容的局域网 `SnapshotV1`。
- `src-tauri/src/commands.rs`：一次生成预览、原子保存预览/快照/原生包，并返回本机时间线。
- `src/features/share/ConversationPreview.tsx`：时间线容器、最新轮次定位和按轮次分页。
- `src/features/share/PreviewTurn.tsx`：单个轮次和条目顺序。
- `src/features/share/PreviewBlock.tsx`：文本、代码、diff、JSON、图片、敏感块和错误块的展开交互。
- `src/features/share/shareTypes.ts` 与 `shareApi.ts`：本机预览 IPC 类型和调用。
- `src/features/receive/ShareApproval.tsx` 与 `src/features/import/ImportReview.tsx`：接收方只显示范围明确的预览说明。
- `src-tauri/tests/full_fidelity_preview.rs`：完整类别、原生包字节和复杂会话集成验证。

---

### Task 1：定义版本化预览时间线协议

**Files:**
- Create: `src-tauri/src/preview/mod.rs`
- Create: `src-tauri/src/preview/schema.rs`
- Create: `src-tauri/src/preview/tests.rs`
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
- Consumes: 无。
- Produces: `PreviewTimelineV1`、`PreviewTurn`、`PreviewItem`、`PreviewBlock`、`PreviewNotice`，以及枚举 `PreviewItemKind`、`PreviewBlockKind`、`PreviewNoticeScope`。
- Produces: `PreviewTimelineV1 { schema_version: u32, source_thread_id: String, cutoff_turn_id: Option<String>, turns: Vec<PreviewTurn>, notices: Vec<PreviewNotice> }`。
- Produces: `PreviewNotice { code: String, count: usize, message: String, scope: PreviewNoticeScope }`；`PreviewNoticeScope` 当前只有 `DisplayOnly`。

- [ ] **Step 1: 写失败测试，锁定序列化字段和顺序**

  测试名：`serializes_preview_timeline_v1_with_ordered_turns_and_display_only_notice`。

```rust
let value = serde_json::to_value(sample_timeline()).unwrap();
assert_eq!(value["schemaVersion"], 1);
assert_eq!(value["turns"][0]["items"][0]["kind"], "user_message");
assert_eq!(value["turns"][0]["items"][0]["blocks"][0]["kind"], "text");
assert_eq!(value["notices"][0]["scope"], "display_only");
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test --manifest-path src-tauri/Cargo.toml preview::tests::serializes_preview_timeline_v1_with_ordered_turns_and_display_only_notice`

Expected: FAIL，提示 `PreviewTimelineV1` 或预览模块尚未定义。

- [ ] **Step 3: 在 `preview/schema.rs` 定义精确类型并导出模块**

  所有结构使用 `#[serde(rename_all = "camelCase")]`；`PreviewItemKind` 和 `PreviewBlockKind` 使用 `snake_case`。块字段固定为 `kind`、`label`、`content`、`sensitive`、`defaultExpanded`。

- [ ] **Step 4: 运行协议测试确认通过**

Run: `cargo test --manifest-path src-tauri/Cargo.toml preview::tests`

Expected: PASS。

- [ ] **Step 5: 提交本任务**

```bash
git add src-tauri/src/lib.rs src-tauri/src/preview
git commit -m "feat: define preview timeline protocol"
```

### Task 2：保留 App Server 原始条目和完整时间顺序

**Files:**
- Modify: `src-tauri/src/codex/models.rs`
- Modify: `src-tauri/src/codex/client.rs`
- Modify: `src-tauri/src/snapshot/convert.rs`
- Modify: `src-tauri/src/snapshot/tests.rs`
- Modify: `src-tauri/src/bin/fake_codex_server.rs`
- Test: `src-tauri/tests/codex_app_server.rs`

**Interfaces:**
- Consumes: 无。
- Produces: `RecordedItem { id: Option<String>, kind: String, timestamp: Option<i64>, raw: serde_json::Value }`。
- Produces: `ThreadData` 和 `ThreadTurn` 从 `snapshot::convert` 迁移到 `codex::models`，字段名保持兼容。
- Produces: `RecordedThread { data: ThreadData, rollout_path: Option<String> }`。
- Produces: `CodexClient::read_thread_data(&mut self, thread_id: &str) -> Result<RecordedThread, CodexError>`。
- `CodexClient::read_snapshot` 改为调用 `read_thread_data` 后再执行 `convert_codex_items`，外部边界不变。

- [ ] **Step 1: 扩展伪造 App Server 并写失败测试**

  增加 `reasoning`、`fileChange`、`webSearch`、`imageView`、`functionCallOutput`、无输出命令和一个 `futureItem`。测试名：`read_thread_data_preserves_all_item_types_raw_json_and_order`。

```rust
let recorded = client.read_thread_data("full-fidelity-thread").unwrap();
let kinds: Vec<_> = recorded.data.turns[0].items.iter().map(|item| item.kind.as_str()).collect();
assert_eq!(kinds, ["userMessage", "reasoning", "commandExecution", "futureItem"]);
assert_eq!(recorded.data.turns[0].items[3].raw["payloadValue"], 42);
assert_eq!(recorded.data.turns[0].items[2].timestamp, Some(130));
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test codex_app_server read_thread_data_preserves_all_item_types_raw_json_and_order`

Expected: FAIL，提示 `read_thread_data`、`RecordedItem` 或 `raw` 字段不存在。

- [ ] **Step 3: 改为保存原始 JSON**

  `ThreadItemEntry.item` 使用 `serde_json::Value`；从 `item["id"]` 和 `item["type"]` 提取标识与类型，从条目的毫秒时间字段或原始条目的 `startedAtMs` 生成秒级时间戳。未知类型不得丢弃。

- [ ] **Step 4: 让旧快照转换消费新模型**

  按 `RecordedItem.kind` 和原始 JSON 路径生成原有 `CodexItem` 行为，保持 `SnapshotV1` 的 schema 和 warning 兼容；同步更新快照单元测试的构造数据。

- [ ] **Step 5: 运行客户端和快照测试**

Run: `cargo test --manifest-path src-tauri/Cargo.toml codex:: && cargo test --manifest-path src-tauri/Cargo.toml snapshot::`

Expected: PASS，现有原生导入和快照测试不回归。

- [ ] **Step 6: 提交本任务**

```bash
git add src-tauri/src/bin/fake_codex_server.rs src-tauri/src/codex src-tauri/src/snapshot src-tauri/tests/codex_app_server.rs
git commit -m "feat: preserve raw Codex history items"
```

### Task 3：规范化时间线并标记疑似敏感内容

**Files:**
- Create: `src-tauri/src/preview/convert.rs`
- Create: `src-tauri/src/preview/sensitive.rs`
- Modify: `src-tauri/src/preview/mod.rs`
- Modify: `src-tauri/src/preview/tests.rs`

**Interfaces:**
- Consumes: Task 1 的预览类型；Task 2 的 `ThreadData`、`ThreadTurn`、`RecordedItem`。
- Produces: `convert_preview_timeline(thread: &ThreadData, cutoff_turn_id: Option<&str>) -> Result<PreviewTimelineV1, SnapshotError>`。
- Produces: `detect_sensitive(content: &str) -> bool` 与 `mark_block_sensitive(block: &mut PreviewBlock)`。
- 支持条目类型：`userMessage`、`agentMessage`、`plan`、`reasoning`、`commandExecution`、`fileChange`、`mcpToolCall`、`dynamicToolCall`、`functionCallOutput`、`webSearch`、`imageView`、`imageGeneration`、错误状态和未知条目。

- [ ] **Step 1: 写失败测试，覆盖全部类别和默认折叠**

  测试名：`normalizes_every_item_kind_and_keeps_original_order`、`unknown_item_falls_back_to_raw_json`、`command_without_output_keeps_command_status_and_duration`。

```rust
let timeline = convert_preview_timeline(&full_thread(), Some("turn-2")).unwrap();
assert_eq!(timeline.turns[0].items.iter().map(|item| item.kind).collect::<Vec<_>>(), [
    PreviewItemKind::UserMessage, PreviewItemKind::Reasoning,
    PreviewItemKind::Command, PreviewItemKind::FileChange,
    PreviewItemKind::ToolCall, PreviewItemKind::WebSearch,
    PreviewItemKind::Image, PreviewItemKind::Unknown,
]);
assert_eq!(timeline.turns[0].items[2].blocks[0].default_expanded, true);
assert_eq!(timeline.turns[0].items[2].blocks[1].default_expanded, false);
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test --manifest-path src-tauri/Cargo.toml preview::tests::normalizes_every_item_kind_and_keeps_original_order`

Expected: FAIL，提示转换函数不存在。

- [ ] **Step 3: 实现保守规范化和未知 JSON 兜底**

  仅在已知结构可安全读取时生成专用标题和块；字段缺失、数组元素不是对象或未来类型统一生成 `PreviewItemKind::Unknown` 和完整 `PreviewBlockKind::Json`。

- [ ] **Step 4: 写失败测试，验证敏感标记不删除原文**

```rust
let timeline = convert_preview_timeline(&thread_with_secret(), None).unwrap();
let block = &timeline.turns[0].items[0].blocks[0];
assert!(block.sensitive);
assert_eq!(block.content, "token=sk-test-example-12345678");
assert_eq!(timeline.notices[0].scope, PreviewNoticeScope::DisplayOnly);
```

- [ ] **Step 5: 实现敏感检测和范围明确的通知**

  复用现有凭据正则，但改为返回布尔标记；生成 `sensitive_blocks`、`unknown_items`、`file_changes`、`command_outputs`、`images`、`tool_outputs` 通知，`message` 必须包含“原生分享包仍完整保留”。

- [ ] **Step 6: 运行预览测试**

Run: `cargo test --manifest-path src-tauri/Cargo.toml preview::tests`

Expected: PASS，且敏感测试断言原文仍存在。

- [ ] **Step 7: 提交本任务**

```bash
git add src-tauri/src/preview
git commit -m "feat: normalize full-fidelity preview timeline"
```

### Task 4：一次生成预览、快照和原生包并隔离 LAN 协议

**Files:**
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/transfer/protocol.rs`
- Test: `src-tauri/src/commands.rs`
- Test: `src-tauri/src/transfer/protocol.rs`

**Interfaces:**
- Consumes: `CodexClient::read_thread_data`、`convert_codex_items`、`convert_preview_timeline`、`native::build_bundle`。
- Produces: `GeneratedPreview { snapshot: SnapshotV1, timeline: PreviewTimelineV1 }`，实现 `Serialize`。
- Produces: `PreviewRegistry::insert(&PreviewSnapshotRequest, SnapshotV1, PreviewTimelineV1, Vec<u8>) -> Result<(), CommandError>`。
- Produces: `PreviewRegistry::display_for_preview(&str) -> Result<GeneratedPreview, CommandError>`。
- Produces: Tauri 命令 `generate_preview(request: PreviewSnapshotRequest) -> Result<GeneratedPreview, CommandError>`。
- 保持 `PreviewRegistry::snapshot_for_share(&str, &str) -> Result<(SnapshotV1, Vec<u8>), CommandError>`，返回类型不包含时间线。

- [ ] **Step 1: 写失败测试，验证注册项包含时间线但分享不暴露**

```rust
registry.insert(&request, snapshot("thread-1"), sample_timeline("thread-1"), vec![1, 2]).unwrap();
assert_eq!(registry.display_for_preview("preview-1").unwrap().timeline.source_thread_id, "thread-1");
assert_eq!(registry.snapshot_for_share("thread-1", "preview-1").unwrap().1, vec![1, 2]);
```

  另在 `transfer/protocol.rs` 写 `share_metadata_never_contains_preview_timeline`，断言序列化后的 JSON 不含 `timeline`、`blocks` 或会话正文。

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test --manifest-path src-tauri/Cargo.toml preview_registry_tests share_metadata_never_contains_preview_timeline`

Expected: FAIL，提示注册表签名或 `display_for_preview` 不存在。

- [ ] **Step 3: 实现原子预览生成**

  `generate_preview` 只调用一次 `read_thread_data`，分别转换时间线和旧快照；验证截止轮次一致后构建原生包。只有时间线、快照和原生包全部成功时才插入注册表并返回 `GeneratedPreview`。

- [ ] **Step 4: 写失败测试，验证失败时不留下可分享注册项**

  测试名：`failed_native_bundle_does_not_leave_shareable_preview`。使用不存在的 rollout 路径，断言命令失败后 `snapshot_for_share` 返回 `invalidPreview`。

- [ ] **Step 5: 注册新命令并移除旧分享页入口**

  在 `lib.rs` 注册 `commands::generate_preview`；保留 `cancel_preview`。删除旧的 `preview_snapshot` Tauri 注册，避免两个命令产生不同注册行为。

- [ ] **Step 6: 运行注册表和协议测试**

Run: `cargo test --manifest-path src-tauri/Cargo.toml preview_registry_tests transfer::protocol`

Expected: PASS。

- [ ] **Step 7: 提交本任务**

```bash
git add src-tauri/src/commands.rs src-tauri/src/lib.rs src-tauri/src/transfer/protocol.rs
git commit -m "feat: generate local preview and native bundle together"
```

### Task 5：实现 Codex 风格时间线和单块展开

**Files:**
- Modify: `src/features/share/shareTypes.ts`
- Modify: `src/features/share/ConversationPreview.tsx`
- Create: `src/features/share/PreviewTurn.tsx`
- Create: `src/features/share/PreviewBlock.tsx`
- Modify: `src/App.css`
- Test: `src/features/share/ConversationPreview.test.tsx`

**Interfaces:**
- Consumes: `PreviewTimelineV1` 的 camelCase JSON。
- Produces: `ConversationPreview({ timeline, snapshot, loading, error })`；`snapshot` 只提供标题、项目和截止轮次元数据。
- `PreviewTurn` 保持 `turn.items` 输入顺序；`PreviewBlock` 接收单个 `PreviewBlock`，默认用 `block.defaultExpanded` 初始化状态。
- 初始只显示最新 1 个轮次；每次“显示更早的 3 轮”增加 3 轮，并在渲染前记录滚动距离以保持位置。

- [ ] **Step 1: 写失败测试，验证最新轮次、顺序和分页**

```tsx
render(<ConversationPreview timeline={largeTimeline()} snapshot={metadataSnapshot()} loading={false} error={null} />);
expect(screen.getByText("第 102 轮用户消息")).toBeInTheDocument();
expect(screen.queryByText("第 1 轮用户消息")).not.toBeInTheDocument();
expect(screen.getByRole("button", { name: "显示更早的 3 轮（还有 101 轮）" })).toBeInTheDocument();
```

- [ ] **Step 2: 运行测试确认失败**

Run: `npm test -- --run src/features/share/ConversationPreview.test.tsx`

Expected: FAIL，旧组件仍接收 `SnapshotV1`，没有 `timeline`。

- [ ] **Step 3: 实现时间线容器、轮次和事件样式**

  用户消息与助手回复使用不同背景和宽度；事件列表按原始顺序渲染；顶部固定显示规格中的完整分享预览说明；不显示旧 `snapshot.warnings`。

- [ ] **Step 4: 写失败测试，验证折叠和敏感内容逐块解锁**

```tsx
expect(screen.queryByText("hunter2")).not.toBeInTheDocument();
fireEvent.click(screen.getAllByRole("button", { name: "显示原文" })[0]);
expect(screen.getByText("hunter2")).toBeInTheDocument();
expect(screen.queryByText("diff --git a/second.ts")).not.toBeInTheDocument();
```

- [ ] **Step 5: 实现 `PreviewBlock`**

  非敏感块使用按钮式摘要和 `aria-expanded`；敏感块始终从隐藏状态开始，点击“显示原文”时只展开该块。文本用 `<p>`，代码/diff/JSON 用 `<pre>`，diff 行按 `+`、`-` 添加增删颜色，图片显示路径占位和文件名。

- [ ] **Step 6: 运行组件测试**

Run: `npm test -- --run src/features/share/ConversationPreview.test.tsx`

Expected: PASS。

- [ ] **Step 7: 提交本任务**

```bash
git add src/features/share/ConversationPreview.tsx src/features/share/PreviewTurn.tsx src/features/share/PreviewBlock.tsx src/features/share/shareTypes.ts src/features/share/ConversationPreview.test.tsx src/App.css
git commit -m "feat: render Codex-style preview timeline"
```

### Task 6：接入分享页并消除点击后的无反馈等待

**Files:**
- Modify: `src/features/share/shareTypes.ts`
- Modify: `src/features/share/shareApi.ts`
- Modify: `src/features/share/SharePage.tsx`
- Modify: `src/features/share/SharePage.test.tsx`
- Test: `src/features/share/pairingContracts.test.ts`

**Interfaces:**
- Consumes: Tauri `generate_preview` 返回的 `GeneratedPreview`。
- Produces: `shareApi.generatePreview(threadId, cutoffTurnId, previewId): Promise<GeneratedPreview>`。
- `SharePage` 的 `PreparedPreview` 同时保存 `snapshot` 与 `timeline`；`PairingPanel` 仍只接收 `threadId`、`previewId`、`previewReady`。

- [ ] **Step 1: 写失败测试，验证点击后立即显示加载状态**

  使用永不 resolve 的 preview Promise，点击“生成分享预览”后立即断言按钮为禁用状态且文字是“正在生成预览…”，随后再 resolve。

- [ ] **Step 2: 运行测试确认失败**

Run: `npm test -- --run src/features/share/SharePage.test.tsx -t "点击后立即显示加载状态"`

Expected: FAIL，旧 API 名称和返回结构不匹配。

- [ ] **Step 3: 更新 API、状态和渲染**

  调用 `generate_preview`，在同一次状态更新中设置 loading；成功后保存 `snapshot` 与 `timeline`，将 `timeline` 传给 `ConversationPreview`。切换会话、截止轮次、重新生成或卸载时继续取消旧 previewId，并忽略迟到结果。

- [ ] **Step 4: 写失败测试，验证配对不发送时间线**

  捕获 `start_share` 参数，断言只有 `threadId`、`previewId`、`bindIp`，且序列化值不含 `timeline` 或会话正文。

- [ ] **Step 5: 更新全部分享页测试数据**

  使用 `GeneratedPreview` mock；保留会话选择、截止轮次、错误、重试、取消和迟到结果测试；检查页面显示时间线示例而不是旧“已省略”警告。

- [ ] **Step 6: 运行分享页测试和构建**

Run: `npm test -- --run src/features/share/SharePage.test.tsx src/features/share/pairingContracts.test.ts && npm run build`

Expected: PASS，TypeScript 无 `GeneratedPreview` 类型错误。

- [ ] **Step 7: 提交本任务**

```bash
git add src/features/share
git commit -m "feat: wire full preview into sharing flow"
```

### Task 7：统一接收方与导入页的“仅预览”文案

**Files:**
- Create: `src/features/share/previewWarnings.ts`
- Modify: `src-tauri/src/snapshot/convert.rs`
- Modify: `src-tauri/src/snapshot/tests.rs`
- Modify: `src/features/receive/ShareApproval.tsx`
- Modify: `src/features/receive/ReceivePage.test.tsx`
- Modify: `src/features/import/ImportReview.tsx`
- Modify: `src/features/import/ImportReview.test.tsx`

**Interfaces:**
- Consumes: `SnapshotV1.warnings`。
- Produces: `formatPreviewWarnings(warnings: string[]): string[]`，兼容旧分享并把“已省略/不支持”改写为“预览折叠/未包含；原生分享包仍完整保留”。
- 新发送方生成的 warning 直接使用范围明确的中文；接收方 UI 统一标题为“预览显示说明”。

- [ ] **Step 1: 写失败测试，验证旧文案不会误导**

```ts
expect(formatPreviewWarnings(["已省略不支持的 文件变更 内容（共 2 项）"]))
  .toEqual(["预览中 文件变更 已折叠（共 2 项）；原生分享包仍完整保留"]);
expect(formatPreviewWarnings(["检测到疑似凭据，已在快照中隐藏"]))
  .toEqual(["预览中检测到疑似凭据，默认隐藏；原生分享包仍完整保留原文"]);
```

- [ ] **Step 2: 运行测试确认失败**

Run: `npm test -- --run src/features/receive/ReceivePage.test.tsx src/features/import/ImportReview.test.tsx`

Expected: FAIL，旧提示仍包含“已省略”。

- [ ] **Step 3: 实现兼容格式化并更新两处页面**

  分享前确认和导入确认都显示“预览显示说明”，并明确接收方当前离线确认只显示元数据。

- [ ] **Step 4: 更新快照 warning 生成**

  新 warning 分别表达“预览中默认折叠/默认隐藏”和“原生分享包仍完整保留”；`SnapshotV1` 的 `schemaVersion` 与字段不改。

- [ ] **Step 5: 运行前后端文案测试**

Run: `npm test -- --run src/features/receive/ReceivePage.test.tsx src/features/import/ImportReview.test.tsx && cargo test --manifest-path src-tauri/Cargo.toml snapshot::`

Expected: PASS，测试查询不到“不支持分享”。

- [ ] **Step 6: 提交本任务**

```bash
git add src/features/share/previewWarnings.ts src/features/receive src/features/import src-tauri/src/snapshot
git commit -m "fix: clarify preview-only omissions"
```

### Task 8：完整类别、原生字节和复杂会话验收

**Files:**
- Create: `src-tauri/tests/full_fidelity_preview.rs`
- Modify: `src-tauri/src/bin/fake_codex_server.rs`
- Modify: `src-tauri/tests/codex_app_server.rs`
- Modify: `docs/testing/codex-app-server-compatibility.md`
- Modify: `docs/native-session-import.md`

**Interfaces:**
- Consumes: Tasks 1–7 的公开接口。
- Produces: 一个使用临时 `CODEX_HOME` 的集成测试，覆盖完整类别、预览时间线、`SnapshotV1` 和原生包 SHA-256。
- Produces: 一个需显式线程 ID 才运行的真实复杂会话测试，只输出类别计数，不输出正文。

- [ ] **Step 1: 写失败集成测试，验证预览与原生包来自同一会话**

```rust
let recorded = client.read_thread_data("full-fidelity-thread").unwrap();
let timeline = convert_preview_timeline(&recorded.data, None).unwrap();
let snapshot = convert_codex_items(recorded.data.clone(), None).unwrap();
let bundle = native::build_bundle(&codex_home, Path::new(&recorded.rollout_path.unwrap()), &snapshot).unwrap();
let parsed = native::parse_bundle(&bundle).unwrap();
assert_eq!(parsed.files[0], fs::read(&rollout_path).unwrap());
assert!(timeline.turns[0].items.iter().any(|item| item.kind == PreviewItemKind::FileChange));
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test full_fidelity_preview`

Expected: FAIL，伪造服务器尚未返回完整类别和可供打包的 rollout。

- [ ] **Step 3: 扩展伪造服务器**

  在临时 `CODEX_HOME/sessions` 写入带 `history_base` 的父/子 rollout；条目覆盖用户、助手、计划、思考、命令无输出、命令有输出、文件 diff、MCP、动态工具、Web 搜索、图片、错误和未知 JSON。测试日志只打印类别计数。

- [ ] **Step 4: 添加真实复杂会话的可选测试**

  测试名：`real_complex_session_preview_matches_native_bundle`。未设置 `AGENTBRIDGE_REAL_CODEX_THREAD_ID` 时标记为非运行；设置后只断言六类旧警告都有对应可展开条目或 unknown 兜底，并验证 rollout SHA 一致。

- [ ] **Step 5: 运行完整自动化检查**

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo test --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
npm test -- --run
npm run build
npm run test:e2e-config
```

Expected: 全部 PASS。

- [ ] **Step 6: 运行本机复杂会话验证**

Run: `AGENTBRIDGE_REAL_CODEX_THREAD_ID=<已验证的复杂会话 ID> cargo test --manifest-path src-tauri/Cargo.toml --test full_fidelity_preview real_complex_session_preview_matches_native_bundle -- --ignored --nocapture`

Expected: PASS；输出只有类别计数、原生文件数量和 SHA 是否一致，不输出正文。

- [ ] **Step 7: 更新中文兼容性文档**

  记录 `PreviewTimelineV1` 仅限本机、原生包字节不变、折叠文案和 Windows 安装包暂缓状态；不得粘贴真实会话正文或凭据。

- [ ] **Step 8: 安装 macOS 最新构建**

Run: `npm run tauri:install`

Expected: 构建成功并自动安装、重新打开 AgentBridge。

- [ ] **Step 9: 提交本任务**

```bash
git add src-tauri/src/bin/fake_codex_server.rs src-tauri/tests/codex_app_server.rs src-tauri/tests/full_fidelity_preview.rs docs/testing/codex-app-server-compatibility.md docs/native-session-import.md
git commit -m "test: verify byte-identical full fidelity preview"
```

---

## 最终自检

- 规格覆盖：数据模型（Task 1）；公开条目与顺序（Task 2）；全部类别、折叠规则、敏感标记与提示（Task 3、5）；本机/局域网隔离（Task 4、6）；接收方文案（Task 7）；原生包 SHA、真实复杂会话和构建（Task 8）。
- 类型一致性：后续任务统一使用 `PreviewTimelineV1`、`GeneratedPreview`、`generate_preview`、`convert_preview_timeline` 和 `read_thread_data`，不在组件或注册表中另起同义名称。
- 测试先行：每个实现任务先写失败测试，再运行指定测试确认失败，最后实现并复跑。
- 审查风险：未知条目、敏感原文、大型时间线、预览令牌约束和 LAN 协议泄漏均映射到具体测试。
- 范围控制：Windows 安装包留待用户后续指令；本轮不改变配对、TLS、原生导入和 256 MiB 限制。
