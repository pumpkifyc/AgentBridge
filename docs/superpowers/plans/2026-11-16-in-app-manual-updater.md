# AgentBridge 应用内手动更新实施计划

> **面向代理执行者：** 必须使用 `superpowers:executing-plans` 逐项执行本计划。本计划已经指定 Native 执行方式；按任务顺序实施、验证和提交。

**目标：** 为 AgentBridge 增加由用户主动检查、查看中文说明、二次确认、下载签名验证并安装的 macOS 和 Windows 完整应用更新。

**架构：** React 更新控件通过独立的更新服务适配层调用 Tauri 官方 Updater 与 Process 插件。GitHub Actions 在版本标签发布时构建 macOS universal 和 Windows x64 更新制品，从同一份中文 `CHANGELOG.md` 生成 Release 正文与 `latest.json`。

**技术栈：** React 19、TypeScript、Tauri 2、Rust、Tauri Updater、Tauri Process、GitHub Actions、GitHub Releases、Node.js 内置测试。

**设计：** [AgentBridge 应用内手动更新设计](../specs/2026-11-16-in-app-manual-updater-design.md)

状态：待用户审阅

## 全局约束

- 所有新增和修改的项目文档使用中文。
- 只使用用户主动触发的更新；应用启动、运行中和退出时不得自动检查、下载或安装。
- 更新来源固定为 `https://github.com/pumpkifyc/AgentBridge/releases/latest/download/latest.json`。
- 用户必须先在“发现新版本”弹窗看到版本、日期和更新说明，再通过“下载并更新”和“确认更新”两个明确动作开始下载。
- Tauri 更新私钥及其密码不得进入仓库、日志或客户端；只使用 GitHub Actions Secrets。
- 正式 macOS 制品为 universal 应用；`latest.json` 同时映射 `darwin-aarch64` 与 `darwin-x86_64`。Windows 正式制品为 x64 NSIS。
- 普通分支 CI 与正式发布隔离；只有 `vX.Y.Z` 标签创建正式 Release。
- 版本标签、`package.json`、`Cargo.toml`、`tauri.conf.json` 和 `CHANGELOG.md` 必须一致。
- 首个可更新版本使用 `0.1.1`，上一正式测试版本使用当前 `0.1.0`。
- 每次桌面版更新完成并通过验证后，执行 `npm run tauri:install` 安装当前平台最新构建。
- 不修改现有局域网分享、原生会话导入、应用数据目录和权限模型。

## 审查重点

- 用户没有点击按钮时发生更新网络请求：启动后只读取当前版本，不调用 `check`；由 Task 2 的“启动时不检查更新”测试覆盖。
- 用户只打开更新说明就直接下载：第一次点击不得下载，必须经过第二确认；由 Task 2 的“二次确认前不下载”测试覆盖。
- 快速重复点击创建并行检查或下载：同一阶段只能存在一个任务；由 Task 1 和 Task 2 的重复操作测试覆盖。
- 下载事件没有总大小、顺序异常或签名错误被当成普通网络错误：进度保持可解释，签名失败阻止安装；由 Task 1 的下载事件和错误分类测试覆盖。
- 标签、应用版本或更新日志不一致却生成正式清单：发布前直接失败，且不公开 Release；由 Task 4 和 Task 5 的版本一致性测试覆盖。

## 文件结构

新增文件：

- `src/features/updater/updateTypes.ts`：更新服务的公开类型、状态联合类型和错误代码。
- `src/features/updater/updaterService.ts`：Tauri Updater/Process 适配、进度累计和错误包装。
- `src/features/updater/updaterService.test.ts`：适配层单元测试。
- `src/features/updater/updateErrors.ts`：错误代码到中文用户文案的稳定映射。
- `src/features/updater/updateErrors.test.ts`：错误文案测试。
- `src/features/updater/UpdateControl.tsx`：顶栏版本入口、流程状态和插件调用。
- `src/features/updater/UpdateDialog.tsx`：更新说明、二次确认、进度和错误界面。
- `src/features/updater/UpdateControl.test.tsx`：完整交互测试。
- `src-tauri/tauri.release.conf.json`：正式发布时开启更新制品的配置覆盖。
- `scripts/release-metadata.mjs`：版本校验、中文更新说明提取和清单生成 CLI。
- `e2e/release-scripts.test.mjs`：发布脚本单元测试。
- `.github/workflows/release.yml`：标签触发的双平台构建与正式 Release。
- `CHANGELOG.md`：中文版本说明。
- `docs/testing/in-app-updater-acceptance.md`：双平台真实升级验收记录。

修改文件：

- `package.json`、`package-lock.json`：更新依赖、发布脚本和版本号。
- `src/App.tsx`、`src/App.css`：顶栏挂载更新控件及响应式样式。
- `src-tauri/Cargo.toml`、`src-tauri/Cargo.lock`：Updater/Process Rust 插件和版本号。
- `src-tauri/src/lib.rs`：注册 Tauri 插件。
- `src-tauri/tauri.conf.json`：更新 endpoint、公钥和 `0.1.1` 版本。
- `src-tauri/capabilities/default.json`：最小更新与重启权限。
- `e2e/package-config.test.mjs`：运行时配置、发布配置和工作流契约。
- `README.md`：用户更新流程、发布流程和两类签名说明。

---

### Task 1：更新服务适配层

**文件：**

- 创建：`src/features/updater/updateTypes.ts`
- 创建：`src/features/updater/updaterService.ts`
- 创建：`src/features/updater/updaterService.test.ts`
- 创建：`src/features/updater/updateErrors.ts`
- 创建：`src/features/updater/updateErrors.test.ts`
- 修改：`package.json`
- 修改：`package-lock.json`

**接口：**

- 使用：`check(): Promise<Update | null>`、`update.download(onEvent)`、`update.install({ restartAfterInstall: true })`、`update.close()`、`relaunch()`、`getVersion(): Promise<string>`。
- 产出：

```ts
export type UpdateErrorCode =
  | "network"
  | "manifest"
  | "download"
  | "signature"
  | "install"
  | "restart"
  | "unknown";

export type UpdaterDownloadEvent =
  | { event: "Started"; data: { contentLength?: number } }
  | { event: "Progress"; data: { chunkLength: number } }
  | { event: "Finished" };

export interface UpdateHandle {
  download(onEvent: (event: UpdaterDownloadEvent) => void): Promise<void>;
  install(options: { restartAfterInstall: boolean }): Promise<void>;
  close(): Promise<void>;
}

export interface AvailableUpdate {
  currentVersion: string;
  version: string;
  date?: string;
  notes: string;
  handle: UpdateHandle;
}

export interface UpdateProgress {
  downloadedBytes: number;
  totalBytes?: number;
  percent?: number;
}

export interface ManualUpdater {
  currentVersion(): Promise<string>;
  check(): Promise<AvailableUpdate | null>;
  download(
    update: AvailableUpdate,
    onProgress: (progress: UpdateProgress) => void,
  ): Promise<void>;
  installAndRestart(update: AvailableUpdate): Promise<void>;
  discard(update: AvailableUpdate): Promise<void>;
}

export class UpdaterError extends Error {
  constructor(public readonly code: UpdateErrorCode, message: string);
}

export function createTauriUpdater(): ManualUpdater;
export function formatUpdateError(error: unknown): string;
```

- [ ] **Step 1：编写更新服务失败测试**

在 `updaterService.test.ts` 中 mock `@tauri-apps/api/app`、`@tauri-apps/plugin-updater` 和 `@tauri-apps/plugin-process`，测试名称与断言为：

```ts
it("读取当前版本但不检查更新", async () => {
  getVersion.mockResolvedValue("0.1.0");
  expect(await updater.currentVersion()).toBe("0.1.0");
  expect(check).not.toHaveBeenCalled();
});

it("将 Tauri Update 转换为带默认说明的可展示对象", async () => {
  check.mockResolvedValue(fakeUpdate({ version: "0.1.1", body: undefined }));
  expect(await updater.check()).toMatchObject({
    currentVersion: "0.1.0",
    version: "0.1.1",
    notes: "",
  });
});

it("累计下载字节并在已知总大小时计算百分比", async () => {
  check.mockResolvedValue(fakeUpdate());
  const update = await updater.check();
  const events: UpdateProgress[] = [];
  await updater.download(update!, (progress) => events.push(progress));
  expect(events.at(-1)).toEqual({
    downloadedBytes: 512,
    totalBytes: 1024,
    percent: 50,
  });
});

it("未知总大小时不伪造下载百分比", async () => {
  check.mockResolvedValue(fakeUpdate({ contentLength: undefined }));
  const update = await updater.check();
  const events: UpdateProgress[] = [];
  await updater.download(update!, (progress) => events.push(progress));
  expect(events.at(-1)).toEqual({ downloadedBytes: 512 });
});

it("下载阶段识别签名错误并包装为 signature", async () => {
  check.mockResolvedValue(fakeUpdate({ downloadError: new Error("invalid signature") }));
  const update = await updater.check();
  await expect(updater.download(update!, () => {})).rejects.toMatchObject({
    code: "signature",
  });
});

it("安装时要求 Windows 重启并随后调用 relaunch", async () => {
  const update = await updater.check();
  await updater.installAndRestart(update!);
  expect(update!.handle.install).toHaveBeenCalledWith({ restartAfterInstall: true });
  expect(relaunch).toHaveBeenCalledOnce();
});

it("放弃更新时关闭 Tauri 资源", async () => {
  const update = await updater.check();
  await updater.discard(update!);
  expect(update!.handle.close).toHaveBeenCalledOnce();
});
```

`updateErrors.test.ts` 验证每个错误代码的中文文案：

```ts
expect(formatUpdateError(new UpdaterError("signature", "bad signature")))
  .toBe("更新包签名校验失败，当前版本未更改。");
expect(formatUpdateError(new Error("ECONNRESET")))
  .toBe("更新失败，请稍后重试。");
```

- [ ] **Step 2：运行测试确认失败**

运行：`npm test -- --run src/features/updater/updaterService.test.ts src/features/updater/updateErrors.test.ts`

预期：FAIL，模块或导出尚不存在。

- [ ] **Step 3：安装依赖并实现适配层**

执行：

```bash
npm install @tauri-apps/plugin-updater@2.14.1 @tauri-apps/plugin-process@2.4.0
```

实现 `createTauriUpdater()`：

- `currentVersion` 直接调用 `getVersion()`。
- `check` 在返回更新时包装 `Update`，`notes` 使用 `body ?? ""`。
- `download` 从 `Started` 读取 `contentLength`，从 `Progress` 累加 `chunkLength`，`Finished` 保持 100%。
- 未知总大小时不伪造百分比。
- `installAndRestart` 先调用 `install({ restartAfterInstall: true })`，再调用 `relaunch()`；Windows 在安装器接管后退出时不再继续调用后续代码。
- `discard` 调用 `handle.close()`，即使关闭失败也不向 UI 暴露底层资源标识。
- 对检查、下载、安装和重启分别包装 `UpdaterError`；下载或安装错误文本包含 `signature`、`invalid signature` 或 `签名` 时映射为 `signature`。

`formatUpdateError` 只返回固定中文文案：

```ts
network: "无法连接更新服务器，请检查网络后重试。"
manifest: "更新清单无效或暂无适用于当前平台的安装包。"
download: "更新下载失败，请检查网络和磁盘空间后重试。"
signature: "更新包签名校验失败，当前版本未更改。"
install: "安装更新失败，当前版本未更改。"
restart: "更新已安装，但应用未能自动重启，请手动重新打开 AgentBridge。"
unknown: "更新失败，请稍后重试。"
```

- [ ] **Step 4：运行单元测试确认通过**

运行：`npm test -- --run src/features/updater/updaterService.test.ts src/features/updater/updateErrors.test.ts`

预期：PASS，所有下载事件、安装和错误映射测试通过。

- [ ] **Step 5：提交**

```bash
git add package.json package-lock.json src/features/updater
git commit -m "feat: add updater service adapter"
```

---

### Task 2：顶栏版本入口与确认式更新界面

**文件：**

- 创建：`src/features/updater/UpdateControl.tsx`
- 创建：`src/features/updater/UpdateDialog.tsx`
- 创建：`src/features/updater/UpdateControl.test.tsx`
- 修改：`src/features/updater/updateTypes.ts`
- 修改：`src/App.tsx`
- 修改：`src/App.css`

**接口：**

- 使用：Task 1 的 `ManualUpdater`、`AvailableUpdate`、`UpdateProgress` 和 `formatUpdateError`。
- 产出：`<UpdateControl updater?: ManualUpdater />`；未传 `updater` 时使用 `createTauriUpdater()`。

```ts
export type UpdateUiState =
  | { kind: "idle"; currentVersion: string }
  | { kind: "checking"; currentVersion: string }
  | { kind: "current"; currentVersion: string; checkedAt: Date }
  | { kind: "available"; update: AvailableUpdate }
  | { kind: "confirm"; update: AvailableUpdate }
  | { kind: "downloading"; update: AvailableUpdate; progress: UpdateProgress }
  | { kind: "installing"; update: AvailableUpdate }
  | { kind: "error"; message: string; retry: "check" | "download" };
```

- [ ] **Step 1：编写界面失败测试**

在 `UpdateControl.test.tsx` 使用内存 `ManualUpdater`。至少包含以下断言：

```ts
it("启动时只读取版本，不检查更新", async () => {
  render(<UpdateControl updater={fakeUpdater} />);
  expect(await screen.findByText("当前版本 v0.1.0")).toBeInTheDocument();
  expect(fakeUpdater.check).not.toHaveBeenCalled();
});

it("显示更新说明，并经过二次确认后才下载", async () => {
  render(<UpdateControl updater={fakeUpdaterWithUpdate} />);
  fireEvent.click(await screen.findByRole("button", { name: "检查更新" }));
  const dialog = await screen.findByRole("dialog", { name: "发现新版本" });
  expect(dialog).toHaveTextContent("v0.1.0");
  expect(dialog).toHaveTextContent("v0.1.1");
  expect(dialog).toHaveTextContent("支持应用内更新");

  fireEvent.click(screen.getByRole("button", { name: "下载并更新" }));
  expect(fakeUpdaterWithUpdate.download).not.toHaveBeenCalled();
  expect(screen.getByText(/更新过程中应用将退出并重启/)).toBeInTheDocument();

  fireEvent.click(screen.getByRole("button", { name: "确认更新" }));
  expect(fakeUpdaterWithUpdate.download).toHaveBeenCalledOnce();
  expect(fakeUpdaterWithUpdate.installAndRestart).toHaveBeenCalledOnce();
});

it("显示下载进度且安装阶段禁止重复操作", async () => {
  render(<UpdateControl updater={deferredUpdater} />);
  await startConfirmedUpdate();
  deferredUpdater.emitProgress({ downloadedBytes: 512, totalBytes: 1024, percent: 50 });
  expect(await screen.findByText("正在下载 50%")).toBeInTheDocument();
  expect(screen.queryByRole("button", { name: "检查更新" })).toBeDisabled();
});
```

另需覆盖：

- `check()` 返回 `null` 时显示“当前已是最新版本”和检查时间。
- 检查和下载期间再次点击不会增加调用次数。
- “稍后再说”和取消第二确认都会关闭弹窗并调用 `discard`。
- 下载失败显示对应中文文案和“重试下载”。
- 检查失败显示“重试检查”。
- 更新说明中的 `<script>` 以文本显示，不产生脚本节点。
- 按 Escape 只能在非检查、非下载、非安装阶段关闭。

- [ ] **Step 2：运行测试确认失败**

运行：`npm test -- --run src/features/updater/UpdateControl.test.tsx`

预期：FAIL，`UpdateControl` 尚不存在。

- [ ] **Step 3：实现更新控件和弹窗**

实现约束：

- `UpdateControl` 挂载时调用一次 `updater.currentVersion()`，失败时顶栏显示“版本未知”，不自动重试。
- 顶栏区域结构为“当前版本 vX.Y.Z”和“检查更新”按钮；检查中按钮文字为“正在检查…”，并设置 `aria-busy="true"`。
- `UpdateDialog` 使用 `role="dialog"`、`aria-modal="true"` 和依状态变化的 `aria-labelledby`。
- 标题分别为“正在检查更新”“当前已是最新版本”“发现新版本”“确认更新”“正在下载”“正在安装更新”“更新失败”。
- 发现新版本时显示当前版本、新版本、RFC 3339 日期本地化结果和 `notes` 纯文本。
- 第一按钮“下载并更新”只进入 `confirm`；第二确认按钮“确认更新”才开始下载。
- `download` 完成后进入 `installing`，随后 `installAndRestart`。若安装返回但未重启，显示 restart 错误文案。
- 关闭或取消时 `void updater.discard(update.handle)`。
- 把所有按钮连接为可键盘访问，下载和检查阶段移除可关闭操作。

在 `App.tsx` 的 `.topbar` 右侧加入：

```tsx
<div className="topbar-status">
  <span className="connection-status">点对点 · 无云端中继</span>
  <UpdateControl />
</div>
```

在 `App.css` 增加顶栏状态组、更新按钮、固定遮罩、弹窗、说明滚动区、进度条和 640px 响应式布局；弹窗不改变首页现有滚动策略。

- [ ] **Step 4：运行前端测试确认通过**

运行：

```bash
npm test -- --run src/features/updater/UpdateControl.test.tsx src/App.test.tsx
npm run build
```

预期：PASS，TypeScript 与 Vite 构建成功。

- [ ] **Step 5：提交**

```bash
git add src/features/updater src/App.tsx src/App.css
git commit -m "feat: add manual update interface"
```

---

### Task 3：Tauri 更新运行时与最小权限

**文件：**

- 修改：`src-tauri/Cargo.toml`
- 修改：`src-tauri/Cargo.lock`
- 修改：`src-tauri/src/lib.rs`
- 修改：`src-tauri/tauri.conf.json`
- 修改：`src-tauri/capabilities/default.json`
- 创建：`src-tauri/tauri.release.conf.json`
- 修改：`e2e/package-config.test.mjs`

**接口：**

- 使用：Task 1 安装的 npm 插件和 Task 2 已构建的更新界面。
- 产出：运行时可调用 `plugin:updater|check`、`plugin:updater|download`、`plugin:updater|install` 和 `plugin:process|restart`；普通构建不生成更新签名制品，正式发布配置生成。

- [ ] **Step 1：编写配置契约失败测试**

在 `e2e/package-config.test.mjs` 新增：

```js
test("应用配置公开更新来源并保留最小权限", () => {
  assert.deepEqual(config.plugins.updater.endpoints, [
    "https://github.com/pumpkifyc/AgentBridge/releases/latest/download/latest.json",
  ]);
  assert.ok(config.plugins.updater.pubkey.length > 20);
  const capability = JSON.parse(readFileSync(capabilityPath, "utf8"));
  assert.ok(capability.permissions.includes("updater:default"));
  assert.ok(capability.permissions.includes("process:allow-restart"));
  assert.equal(capability.permissions.includes("process:default"), false);
});

test("普通构建与正式更新构建分离", () => {
  assert.equal(config.bundle.createUpdaterArtifacts, false);
  assert.equal(releaseConfig.bundle.createUpdaterArtifacts, true);
  assert.match(cargo, /tauri-plugin-updater/);
  assert.match(cargo, /tauri-plugin-process/);
  assert.match(lib, /tauri_plugin_updater::Builder/);
  assert.match(lib, /tauri_plugin_process::init/);
});
```

- [ ] **Step 2：运行配置测试确认失败**

运行：`npm run test:e2e-config`

预期：FAIL，更新配置、插件和权限尚不存在。

- [ ] **Step 3：生成签名密钥并配置 Tauri 插件**

由执行者在交互终端运行 Tauri CLI 生成密钥，密码从终端隐藏输入，不把私钥或密码写进命令、日志和仓库：

```bash
npm run tauri -- signer generate -w "$HOME/.tauri/agentbridge.key"
```

将 CLI 输出的公钥写入 `tauri.conf.json` 的 `plugins.updater.pubkey`，确认 `$HOME/.tauri/agentbridge.key` 存在且仅当前用户可读。把私钥内容与密码录入 GitHub Secrets `TAURI_SIGNING_PRIVATE_KEY`、`TAURI_SIGNING_PRIVATE_KEY_PASSWORD`，录入过程不得回显。

修改：

- `Cargo.toml` 增加 `tauri-plugin-updater = "2"`、`tauri-plugin-process = "2"`。
- `lib.rs` 注册 Updater Builder 和 Process 插件。
- `tauri.conf.json` 增加 endpoint 与公钥，保持 `createUpdaterArtifacts: false`。
- `tauri.release.conf.json` 只覆盖 `bundle.createUpdaterArtifacts: true`。
- capability 只增加 `updater:default` 与 `process:allow-restart`。

- [ ] **Step 4：验证 Rust、权限与配置**

运行：

```bash
node --test e2e/package-config.test.mjs
cargo test --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
```

预期：全部 PASS；Rust 能解析插件配置且没有权限警告。

- [ ] **Step 5：提交**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/lib.rs \
  src-tauri/tauri.conf.json src-tauri/tauri.release.conf.json \
  src-tauri/capabilities/default.json e2e/package-config.test.mjs
git commit -m "feat: enable signed Tauri updater runtime"
```

---

### Task 4：中文版本说明、版本校验与更新清单

**文件：**

- 创建：`CHANGELOG.md`
- 创建：`scripts/release-metadata.mjs`
- 创建：`e2e/release-scripts.test.mjs`
- 修改：`package.json`
- 修改：`package-lock.json`

**接口：**

- 使用：版本文件和 `CHANGELOG.md` 的 Node.js 文件路径。
- 产出：

```js
export function validateRelease({
  tag,
  packageVersion,
  cargoVersion,
  tauriVersion,
  changelog,
}) {}
// => { version: string, notes: string, releaseDate: string }

export function createLatestManifest({
  version,
  notes,
  pubDate,
  artifacts,
}) {}
// => { version, notes, pub_date, platforms }
```

CLI：

```text
node scripts/release-metadata.mjs validate --tag v0.1.1
node scripts/release-metadata.mjs notes --tag v0.1.1 --output release-notes.md
node scripts/release-metadata.mjs manifest --artifacts ./release-assets --output latest.json
```

- [ ] **Step 1：编写发布脚本失败测试**

在 `e2e/release-scripts.test.mjs` 使用内存中文字符串：

```js
test("从中文 CHANGELOG 精确提取标签版本", () => {
  assert.deepEqual(
    validateRelease({
      tag: "v0.1.1",
      packageVersion: "0.1.1",
      cargoVersion: "0.1.1",
      tauriVersion: "0.1.1",
      changelog: FIXTURE_CHANGELOG,
    }),
    {
      version: "0.1.1",
      releaseDate: "2026-11-16",
      notes: "### 新增\n\n- 支持在应用内检查、确认并安装新版本。",
    },
  );
});

test("标签与版本不一致时拒绝发布", () => {
  assert.throws(
    () => validateRelease({ ...validRelease, tag: "v0.1.2" }),
    /发布标签 v0\.1\.2 与 package\.json 版本 0\.1\.1 不一致/,
  );
});

test("缺少版本段落时拒绝发布", () => {
  assert.throws(
    () => validateRelease({ ...validRelease, changelog: "# 更新日志\n" }),
    /CHANGELOG\.md 缺少 0\.1\.1 版本段落/,
  );
});

test("macOS 两种架构共享 universal 制品，Windows 使用 NSIS", () => {
  const manifest = createLatestManifest(validManifestInput);
  assert.equal(
    manifest.platforms["darwin-aarch64"].url,
    manifest.platforms["darwin-x86_64"].url,
  );
  assert.match(manifest.platforms["windows-x86_64"].url, /x64-setup\.exe$/);
  assert.equal(manifest.pub_date, "2026-11-16T08:00:00.000Z");
});
```

另需断言：缺失任一平台、空签名、重复平台、非法版本号或 URL 不来自当前仓库 Release 时失败。

- [ ] **Step 2：运行脚本测试确认失败**

运行：`node --test e2e/release-scripts.test.mjs`

预期：FAIL，脚本尚不存在。

- [ ] **Step 3：实现发布脚本并添加中文更新日志**

在 `package.json` 增加：

```json
"release:validate": "node scripts/release-metadata.mjs validate",
"release:notes": "node scripts/release-metadata.mjs notes",
"release:manifest": "node scripts/release-metadata.mjs manifest"
```

`CHANGELOG.md` 添加：

```markdown
## [0.1.1] - 2026-11-16

### 新增

- 支持在应用内检查、查看中文更新说明、确认、下载并安装新版本。
- 支持 macOS 与 Windows 完整应用更新，并在签名校验失败时保留原版本。
```

发布脚本使用 Node 内置模块，不新增依赖。`manifest` 从 `release-assets/*/updater-target.json` 读取平台、文件名和签名；只接受 `darwin-aarch64`、`darwin-x86_64`、`windows-x86_64`，生成指向 `https://github.com/pumpkifyc/AgentBridge/releases/download/{tag}/{fileName}` 的 URL。

- [ ] **Step 4：运行脚本测试确认通过**

运行：

```bash
node --test e2e/release-scripts.test.mjs
npm run release:validate -- --tag v0.1.1
```

预期：测试 PASS；当前仓库版本仍为 `0.1.0`，因此第二条命令明确失败，确认发布门禁生效。

- [ ] **Step 5：提交**

```bash
git add CHANGELOG.md scripts/release-metadata.mjs e2e/release-scripts.test.mjs \
  package.json package-lock.json
git commit -m "feat: add release metadata pipeline"
```

---

### Task 5：双平台签名发布工作流与 0.1.1 版本基线

**文件：**

- 创建：`.github/workflows/release.yml`
- 修改：`package.json`
- 修改：`package-lock.json`
- 修改：`src-tauri/Cargo.toml`
- 修改：`src-tauri/Cargo.lock`
- 修改：`src-tauri/tauri.conf.json`
- 修改：`e2e/package-config.test.mjs`

**接口：**

- 使用：Task 3 的发布配置与密钥 Secrets、Task 4 的发布脚本。
- 产出：推送 `v0.1.1` 后生成草稿 Release，包含 macOS universal、Windows x64 安装包、更新制品、签名和 `latest.json`；全部校验通过后公开。

- [ ] **Step 1：编写发布工作流契约失败测试**

在 `e2e/package-config.test.mjs` 读取 `.github/workflows/release.yml` 并断言：

```js
test("版本标签触发双平台签名发布", () => {
  assert.match(releaseWorkflow, /tags:\s*\n\s*- "v\*\.\*\.\*"/);
  assert.match(releaseWorkflow, /contents: write/);
  assert.match(releaseWorkflow, /TAURI_SIGNING_PRIVATE_KEY/);
  assert.match(releaseWorkflow, /TAURI_SIGNING_PRIVATE_KEY_PASSWORD/);
  assert.match(releaseWorkflow, /universal-apple-darwin/);
  assert.match(releaseWorkflow, /windows-2022/);
  assert.match(releaseWorkflow, /tauri\.release\.conf\.json/);
  assert.match(releaseWorkflow, /release-metadata\.mjs/);
  assert.match(releaseWorkflow, /--draft/);
  assert.doesNotMatch(releaseWorkflow, /-----BEGIN.*PRIVATE KEY-----/s);
});
```

另需断言发布工作流先运行全部测试，普通 `build.yml` 不创建 Release，且正式版本与三处配置均为 `0.1.1`。

- [ ] **Step 2：运行测试确认失败**

运行：`node --test e2e/package-config.test.mjs`

预期：FAIL，`release.yml` 尚不存在且版本仍为 `0.1.0`。

- [ ] **Step 3：更新版本并实现标签发布工作流**

使用：

```bash
npm version 0.1.1 --no-git-tag-version
```

再同步修改 `src-tauri/Cargo.toml` 与 `src-tauri/tauri.conf.json` 为 `0.1.1`，并通过一次 Cargo 命令刷新 `Cargo.lock` 中的根包版本。

实现 `.github/workflows/release.yml`：

- 仅 `push.tags` 匹配 `v*.*.*`。
- 顶层权限继承只读，发布 job 单独 `contents: write`。
- 在 macOS 和 Windows matrix 中安装 Node 22、Rust stable 和前端依赖。
- macOS 安装 `aarch64-apple-darwin`、`x86_64-apple-darwin`，用 `--target universal-apple-darwin --config src-tauri/tauri.release.conf.json` 构建。
- Windows 用默认 x64 target 和 `src-tauri/tauri.release.conf.json` 构建。
- 每个平台收集更新制品与 `.sig`，生成一个 `updater-target.json`，先用 `actions/upload-artifact@v4` 上传。
- 最终 job 下载全部平台制品，运行 `release:validate`、`release:notes` 和 `release:manifest`。
- 从同一标签和说明创建草稿 Release，上传 DMG、应用归档、NSIS、`.app.tar.gz`、`.exe` 与签名文件。
- 本地校验文件数量、签名非空、`latest.json` 三平台映射和 URL 后公开 Release。
- 验证/构建 job 不继承 `contents: write`；普通 CI 不引用签名 Secrets。

- [ ] **Step 4：运行配置与发布脚本验证**

运行：

```bash
node --test e2e/package-config.test.mjs e2e/release-scripts.test.mjs
npm run release:validate -- --tag v0.1.1
npm run release:notes -- --tag v0.1.1 --output /tmp/agentbridge-release-notes.md
```

预期：全部 PASS；发布说明文件内容来自 `CHANGELOG.md` 的 `0.1.1` 段落。

- [ ] **Step 5：提交**

```bash
git add .github/workflows/release.yml package.json package-lock.json \
  src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/tauri.conf.json \
  e2e/package-config.test.mjs
git commit -m "ci: publish signed desktop releases"
```

---

### Task 6：用户文档、验收矩阵与最终配置契约

**文件：**

- 创建：`docs/testing/in-app-updater-acceptance.md`
- 修改：`README.md`
- 修改：`e2e/package-config.test.mjs`

**接口：**

- 使用：Task 1 至 Task 5 的最终命令行、版本和更新流程。
- 产出：用户可照做的中文更新与发布说明，以及 macOS、Windows 可填写的真机升级验收表。

- [ ] **Step 1：编写文档契约失败测试**

在 `e2e/package-config.test.mjs` 读取 README 和新验收文档：

```js
test("中文文档说明手动更新、签名和双平台验收", () => {
  for (const text of [
    "检查更新",
    "下载并更新",
    "确认更新",
    "不会自动安装",
    "Tauri 更新签名",
    "Apple 签名与公证",
    "Windows 代码签名",
    "0.1.0 → 0.1.1",
  ]) {
    assert.ok(`${readme}\n${acceptance}`.includes(text), `文档缺少：${text}`);
  }
  assert.match(acceptance, /Apple Silicon/);
  assert.match(acceptance, /Intel Mac/);
  assert.match(acceptance, /Windows x64/);
  assert.match(acceptance, /网络中断/);
  assert.match(acceptance, /签名校验失败/);
  assert.match(acceptance, /磁盘空间不足/);
  assert.match(acceptance, /应用文件被占用/);
  assert.match(acceptance, /应用数据保持不变/);
});
```

- [ ] **Step 2：运行测试确认失败**

运行：`npm run test:e2e-config`

预期：FAIL，新验收文档和 README 段落尚不存在。

- [ ] **Step 3：编写中文用户与验收文档**

`README.md` 新增“应用内更新”：

- 顶栏按钮、说明、两次确认、下载进度、安装重启。
- 明确“不会自动检查或安装”。
- 写明 `latest.json` 地址和仅由 `vX.Y.Z` 标签发布。
- 写明 Tauri 更新签名与 Apple/Windows 操作系统签名的区别。
- 写明 `npm run tauri:install` 仅用于当前开发机安装最新构建。

`in-app-updater-acceptance.md` 包含：

- 运行记录表头：日期、执行人、系统架构、当前/目标版本、安装来源。
- Apple Silicon macOS、Intel Mac、Windows x64 各一行。
- 正常升级：`0.1.0 → 0.1.1`、说明、两次确认、进度、重启、数据完整。
- 失败场景：网络中断、坏签名、磁盘不足、安装文件被占用、重启失败。
- 发布核对：匿名下载、三平台清单、签名、CHANGELOG 与 Release 正文一致。
- 未执行项必须保留并明确记录，不能把 CI 构建当成真机升级通过。

- [ ] **Step 4：运行文档与配置测试**

运行：

```bash
npm run test:e2e-config
npm test -- --run
```

预期：全部 PASS，现有分享、导入、导航和更新测试无回归。

- [ ] **Step 5：提交**

```bash
git add README.md docs/testing/in-app-updater-acceptance.md e2e/package-config.test.mjs
git commit -m "docs: document in-app update acceptance"
```

---

### Task 7：双平台构建、真实升级和本机安装

**文件：**

- 修改：`docs/testing/in-app-updater-acceptance.md`

**接口：**

- 使用：Task 5 的 `v0.1.1` Release 和 Task 6 的验收表。
- 产出：CI 全绿、`latest.json` 可匿名访问、macOS 和 Windows 从 `0.1.0` 到 `0.1.1` 的真实升级记录；macOS 最终安装到 `/Applications/AgentBridge.app`。

- [ ] **Step 1：运行完整本地验证**

执行：

```bash
npm ci
npm run test:e2e-config
npm test -- --run
cargo test --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
npm run build
```

预期：全部 PASS，无 TypeScript、Rust、clippy 或测试失败。

- [ ] **Step 2：推送分支并确认普通 CI 不发布**

执行：

```bash
git push origin agentbridge-base
gh run list --workflow build.yml --branch agentbridge-base --limit 1
```

预期：macOS 与 Windows 验证 job 全绿；GitHub Releases 没有新增正式版本。

- [ ] **Step 3：创建并推送 `v0.1.1` 标签**

执行：

```bash
git tag v0.1.1
git push origin v0.1.1
gh run list --workflow release.yml --limit 1
```

签发密钥已经由 Task 3 写入 Secrets。预期：发布工作流构建 universal macOS 与 Windows x64，先创建草稿并上传，全部校验通过后公开 `v0.1.1`。

- [ ] **Step 4：验证 Release 与匿名更新清单**

不使用 GitHub 登录信息下载：

```bash
curl -fsSL https://github.com/pumpkifyc/AgentBridge/releases/latest/download/latest.json
```

预期：版本为 `0.1.1`，说明与 `CHANGELOG.md` 的 `0.1.1` 段落一致，包含 `darwin-aarch64`、`darwin-x86_64`、`windows-x86_64`，三项均有非空签名和可下载 URL。

- [ ] **Step 5：执行 macOS 真实升级**

保留并启动已安装的 `0.1.0`，点击“检查更新”升级到 `0.1.1`。验证两次确认、进度、签名校验、安装和重启；升级后确认版本显示 `0.1.1`，现有导入历史、设置、暂存会话和分享功能可读取。

在 Apple Silicon 和 Intel Mac 各完成一次，或在验收文档记录尚未满足的平台。网络中断和坏签名场景至少各验证一次，确认原版本仍可启动。

- [ ] **Step 6：执行 Windows 真实升级**

在 Windows x64 真机或 VM 上保留 `0.1.0`，通过应用内按钮升级到 `0.1.1`。验证 NSIS current-user 安装、应用重启、数据保留和签名失败不安装。若当前没有 Windows 运行环境，保留表项并明确标记“待真机验收”，不得声称已通过。

- [ ] **Step 7：安装当前平台最新构建**

完成桌面版验证后运行：

```bash
npm run tauri:install
```

预期：macOS 构建并覆盖安装 `/Applications/AgentBridge.app`，重新打开后版本为 `0.1.1`。

- [ ] **Step 8：记录结果并提交**

把真实结果、系统架构、版本、失败用例和未完成项写入 `docs/testing/in-app-updater-acceptance.md`，然后：

```bash
git add docs/testing/in-app-updater-acceptance.md
git commit -m "test: record in-app updater acceptance"
git push origin agentbridge-base
```
