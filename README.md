# AgentBridge

AgentBridge 是一款在局域网内原生分享 Codex 会话的 macOS 和 Windows 桌面应用。

## 开发环境

- Node.js、npm、Rust stable 工具链，以及当前平台所需的 Tauri 2 桌面开发依赖。

## 常用命令

- `npm install`：安装前端依赖。
- `npm run tauri:dev`：启动桌面开发版。
- `npm test -- --run`：运行前端测试。
- `npm run test:e2e-config`：检查双平台打包配置、CI 步骤和中文验收文档。
- `npm run build`：构建前端资源。
- `npm run tauri:build`：构建当前平台桌面安装包。
- `npm run tauri:install`：构建当前平台最新版并自动安装、重新打开；macOS 覆盖 `/Applications/AgentBridge.app`，Windows 静默运行最新 NSIS 安装包。

Windows 安装包需在 Windows 主机或 CI runner 上构建。

## 找不到 Codex CLI

从 Finder 或开始菜单启动桌面应用时，进程通常不会继承终端里的完整 `PATH`。AgentBridge 会依次检查显式配置、当前 `PATH`、用户级 npm/Homebrew/Cargo 安装目录，以及 macOS ChatGPT/Codex 桌面版的公开内置 CLI 位置。当前默认优先支持 npm 全局安装在 `~/.local/bin/codex` 或 `~/.npm-global/bin/codex` 的情况。

如果 Codex 安装在其他位置，可在启动 AgentBridge 前设置 `AGENTBRIDGE_CODEX_EXECUTABLE` 为 `codex` 可执行文件的完整路径。应用只启动该 CLI 的公开 `app-server --stdio` 接口，不读取桌面版内部数据库。

## 原生会话分享流程

1. 分享方在「分享会话」搜索并选择本机 Codex 会话和截止轮次，明确生成预览后创建一次性邀请。应用会优先自动选择局域网 IPv4 地址，`127.0.0.1` 仅用于本机测试。
2. 分享预览同时读取会话元数据，并把 Codex 返回的原始 rollout 文件及其分叉依赖打包为版本化 `.agbridge` 原生会话包。单包上限为 256 MiB，最多包含 64 个 rollout 文件。
3. 接收方通过可信渠道取得配对 JSON，核对验证码与 TLS 证书指纹，先预览元数据，再明确接受加密传输。
4. 原生会话包通过 SHA-256 校验后暂存在接收方本机。接收方选择自己的项目目录后，AgentBridge 将 rollout 复制到本机 `CODEX_HOME`，调用公开 App Server 的 `thread/fork`、`lastTurnId` 和 `thread/name/set` 创建独立分支。
5. 「导入历史」可查看状态、重试失败导入、打开已创建会话和删除本机记录及暂存会话包。

核心会话操作使用公开 App Server 接口。为了让 Codex 桌面版立即显示通过 `thread/fork` 创建的会话，AgentBridge 会在本机 Codex 状态索引中同步标题、预览和用户会话可见性字段；该兼容步骤不会上传索引内容。原生会话包保留原始会话记录，可能包含工具输出、文件路径、系统提示或其他敏感内容，分享前必须检查会话，接收方应只导入可信来源。

原生导入不会重新执行历史工具调用。会话会使用接收方选择的项目目录作为后续工作目录，原会话和分享方电脑不会被修改。

## 验收与已知限制

- 设计：[局域网会话分享设计](docs/superpowers/specs/2026-11-11-lan-codex-session-sharing-design.md)。
- 原生包与导入机制：[Codex 原生会话包与导入机制](docs/native-session-import.md)。
- 接口兼容性：[Codex App Server 公开接口与本机兼容性记录](docs/testing/codex-app-server-compatibility.md)。
- 打包与双平台真机矩阵：[跨平台打包与验收记录](docs/testing/cross-platform-acceptance.md)。
- 人工步骤：[分享与导入验收清单](e2e/share-import-checklist.md)。

macOS 未签名的 `.app` 和 `.dmg` 已在本机成功构建；Windows NSIS 安装包、GitHub Actions 双平台运行、双向真机分享和 Codex 桌面版历史可见性仍需按上述文档执行发布前验收。当前 macOS 已使用 Codex CLI 0.167.2 在临时 `CODEX_HOME` 验证原生包安装和 `thread/fork`，单机结果不能替代双平台真机验收。

`.github/workflows/build.yml` 在 macOS 和 Windows runner 上运行配置检查、前端测试、Rust 测试、clippy 和未签名发布构建；正式签名与公证不在第一版开发构建范围内。
