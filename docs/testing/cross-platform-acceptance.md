# 跨平台打包与验收记录

文档日期：2026-11-16
适用范围：AgentBridge 0.1.1，局域网 Codex 会话分享与应用内手动更新。

## 构建与自动化验证

| 项目 | macOS | Windows | 当前状态 |
| --- | --- | --- | --- |
| 自动化环境 | `macos-15`（GitHub Actions） | `windows-2022`（GitHub Actions） | 工作流已配置，尚未在 GitHub runner 执行 |
| 本机开发构建 | macOS 15.6.0、Apple Silicon、Tauri CLI 2.12.4；`npm run tauri:build` 生成 `.app` 8.65 MiB 和 `.dmg` 4.22 MiB | 未执行 | macOS 已通过，Windows 须在原生 runner 执行 |
| 前端测试 | `npm test -- --run` | 同左 | 双平台 CI 步骤已配置；本机结果见实施账本 |
| Rust 测试 | `cargo test --manifest-path src-tauri/Cargo.toml --features test-fixtures` | 同左 | 双平台 CI 步骤已配置；本机结果见实施账本 |
| 静态分析 | `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --features test-fixtures -- -D warnings` | 同左 | 双平台 CI 步骤已配置；本机结果见实施账本 |
| 打包配置断言 | `npm run test:e2e-config` | 同左 | 本机已通过；双平台 CI 步骤已配置 |
| 未签名制品 | `macos/*.app`、`dmg/*.dmg` | `nsis/*.exe` | macOS 本机已生成；Windows 制品待 runner 验证 |

常规 CI 位于 `.github/workflows/build.yml`，只上传未签名开发制品。版本标签发布位于 `.github/workflows/release.yml`，使用 Tauri 更新签名生成 `latest.json` 和双平台更新制品；详细流程见 [应用内更新真机验收](in-app-updater-acceptance.md)。macOS 构建目标为 `app` 和 `dmg`，最低系统版本为 macOS 11.0；Windows 构建目标为 NSIS `currentUser` 安装包。Apple 签名与公证、Windows 代码签名仍须在正式稳定分发前配置和验证。

自动化命令 `npm run test:e2e-config` 检查安装包目标、NSIS 安装模式、CI 矩阵、无签名秘密，以及存在测试辅助二进制时是否明确指定应用主二进制。真正验证 Tauri 能否解析配置仍需 `npm run tauri:build`；该命令已在本机 macOS 完整运行成功。

## 兼容性前提

分享方和接收方都需要安装版本匹配的 Codex CLI 与 Codex 桌面版，并且本机 `codex` 命令可用。当前 macOS 开发环境使用 Codex CLI 0.167.2，已确认 `thread/list`、`thread/turns/list`、`thread/items/list`、rollout 路径、`thread/fork` 和 `thread/name/set` 可用。分享方界面会自动选择检测到的局域网 IPv4 地址，会话列表支持搜索；只有点击生成预览后才读取完整历史和构建原生会话包。详细结果见 [Codex App Server 公开接口与本机兼容性记录](codex-app-server-compatibility.md) 与 [原生会话包与导入机制](../native-session-import.md)。

核心会话读写使用公开 App Server 接口。导入时复制原始 rollout 并通过 `thread/fork` 创建接收方自己的 thread；为了让桌面版索引立即显示该分支，AgentBridge 会在本机同步标题、预览和用户会话可见性字段。当前没有可依赖的公开 deep link，自动打开失败时提示用户重启 Codex 并在历史列表中查找。

**经双平台验证的最低 Codex 版本：待填写。** 发布前必须在 macOS 和 Windows 分别记录桌面版版本、`codex --version`、`generate-json-schema` 导出的协议版本，以及下列流程的实际结果；只有两个平台都完成，才能填写最低版本。

## 双向真机验收矩阵

执行步骤见 [分享与导入验收清单](../e2e/share-import-checklist.md)。每一行的「分享方」和「接收方」都要填写操作系统版本、Codex 桌面版版本和 CLI 版本；结果写「通过 / 失败 / 未执行」并记录截图或日志位置。

| 场景 | 分享方 | 接收方 | 配对与预览 | 传输与暂存 | 本机导入 | 桌面版可见 / 后备 | 结果 |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 正向：原生分享 | macOS | Windows | 待执行 | 待执行 | 待执行 | 待执行 | 未执行 |
| 反向：原生分享 | Windows | macOS | 待执行 | 待执行 | 待执行 | 待执行 | 未执行 |
| 同平台回归：原生分享 | macOS | macOS | 待执行 | 待执行 | 待执行 | 待执行 | 未执行 |
| 同平台回归：原生分享 | Windows | Windows | 待执行 | 待执行 | 待执行 | 待执行 | 未执行 |

真机验收至少使用两台处于同一可信局域网的电脑，并由分享方通过可信渠道传递短时配对 JSON。Windows 接收方分别检查「原生目录选择器」和「手动输入 `C:\...` 路径」；macOS 接收方检查 `/Users/...` 路径。分享方的绝对路径只允许显示为说明，不能被直接用作接收方项目目录。

## 故障与清理验收

下表两个平台都要执行：macOS 作为接收方一次，Windows 作为接收方一次。传输中断后应保留仍有效的邀请供显式重试；成功、拒绝、过期、撤销和停止分享后应清理临时证书与监听句柄。

| 检查项 | 预期 | macOS 接收 | Windows 接收 |
| --- | --- | --- | --- |
| 配对 JSON 缺字段、验证码错误 | 建立连接前拒绝，不泄露会话元数据 | 待执行 | 待执行 |
| 过期配对、撤销配对、重放凭据 | 拒绝连接或下载 | 待执行 | 待执行 |
| 预览后明确拒绝分享 | 不写暂存会话包，不创建 thread，凭据失效 | 待执行 | 待执行 |
| 下载中取消或断网 | 中止任务，不标记为已导入；有效邀请可重试 | 待执行 | 待执行 |
| 拒绝或破坏最终摘要 | 内容校验失败，仍可明确重试 | 待执行 | 待执行 |
| 已完成导入再次重试 | 命令拒绝重复创建 Codex thread | 待执行 | 待执行 |
| Codex CLI 缺失或版本不兼容 | 保留暂存原生会话包与失败记录，显示可执行错误 | 待执行 | 待执行 |
| 接收方项目目录不存在 | 拒绝导入，不启动新 thread | 待执行 | 待执行 |
| 分享方工作目录当作本机目录 | 拒绝，要求选择接收方自己的目录 | 待执行 | 待执行 |
| 删除导入记录 | 删除 SQLite 记录和应用数据目录内的暂存原生会话包，目录外文件不受影响 | 待执行 | 待执行 |
| 邀请过期或停止分享 | 临时证书与监听句柄清理；不删除已保存的导入历史 | 待执行 | 待执行 |
| 关闭并重启应用 | 已保存的导入历史仍可读取；内存中的短时邀请消失 | 待执行 | 待执行 |

## 桌面版历史与后备方案

在两个平台分别完成：

1. 用版本匹配的 CLI 与桌面版启动 AgentBridge，确认分享方能列出会话、选择截止轮次并读取消息。
2. 接收方选择自己的项目目录，导入原生会话包并检查 SQLite 导入记录状态为已完成。
3. 通过公开 `thread/list` 检查新 thread 是否存在；刷新 Codex 桌面版历史，确认能看到并继续该会话。
4. 检查会话轮次、工具记录和标题是否与分享方截止轮次一致，并确认新会话使用接收方项目目录。
5. 点击「打开会话」。若没有公开打开方式，确认页面提示重启 Codex 并在历史列表中查找。
6. 记录桌面版版本、CLI 版本、是否可见、原生历史完整性和后续轮次结果。

**发布门槛：** 上述双向矩阵、两平台故障矩阵和桌面版可见性验收均须填完。若桌面版无法显示 fork 后的 thread，记录该平台的状态索引版本与失败日志；不得宣称原生导入通过。当前单机协议验证不能替代双平台真机验收。

## 已知限制

- 应用使用 Tauri 更新签名和 GitHub Releases 手动更新；不包含自动检查、自动下载或后台安装。
- Tauri 更新签名不替代 Apple 签名与公证或 Windows 代码签名；正式稳定分发前需验证系统安全警告。
- 仅在可信局域网使用；配对凭据需通过可信渠道短时传递，接收方需核对验证码。
- 原生会话包保留原始 rollout，可能含工具输出、文件路径、系统提示或其他敏感内容；分享前必须检查会话。
- 单个原生会话包最大 256 MiB，最多包含 64 个 rollout 文件。
- 没有可依赖的公开 Codex 桌面版 deep link 来打开指定第三方 thread。
- 删除导入记录不会删除已创建的 Codex thread，用户需在 Codex 中自行管理。
- Windows 原生目录选择器、NSIS 安装包和 Codex 桌面版行为尚未在本会话验证；需要 Windows 主机执行验收清单。
