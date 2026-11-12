# AgentBridge

AgentBridge 是一款用于在局域网内分享 Codex 会话上下文的 macOS 和 Windows 桌面应用。

## 开发环境

- Node.js、npm、Rust stable 工具链，以及当前平台所需的 Tauri 2 桌面开发依赖。

## 常用命令

- `npm install`：安装前端依赖。
- `npm run tauri:dev`：启动桌面开发版。
- `npm test -- --run`：运行前端测试。
- `npm run test:e2e-config`：检查双平台打包配置、CI 步骤和中文验收文档。
- `npm run build`：构建前端资源。
- `npm run tauri:build`：构建当前平台桌面安装包。

Windows 安装包需在 Windows 主机或 CI runner 上构建。

## 第一版使用流程

1. 分享方在「分享会话」搜索并选择本机 Codex 会话和截止轮次，明确生成预览并核对文本与警告后创建一次性邀请。应用会优先自动选择局域网 IPv4 地址，`127.0.0.1` 仅用于本机测试。
2. 通过可信渠道把配对 JSON 交给同事；接收方核对验证码与证书指纹，在「接收会话」先预览元数据，再明确接受下载。
3. 快照校验后先暂存在接收方本机。接收方在导入确认页选择自己的项目目录，再创建独立的本机 Codex 续接会话。
4. 「导入历史」可查看状态、重试失败导入、复制续接提示和删除本机记录及暂存快照。

第一版只使用公开 Codex App Server 接口，不写入桌面版内部存储，也不传输凭据、配置、源码或可执行工具请求。若桌面版无法通过公开接口列出或打开新会话，请复制续接提示在 Codex 中手动新建会话。

## 验收与已知限制

- 设计：[局域网会话分享设计](docs/superpowers/specs/2026-11-11-lan-codex-session-sharing-design.md)。
- 接口兼容性：[Codex App Server 公开接口与本机兼容性记录](docs/testing/codex-app-server-compatibility.md)。
- 打包与双平台真机矩阵：[跨平台打包与验收记录](docs/testing/cross-platform-acceptance.md)。
- 人工步骤：[分享与导入验收清单](e2e/share-import-checklist.md)。

macOS 未签名的 `.app` 和 `.dmg` 已在本机成功构建；Windows NSIS 安装包、GitHub Actions 双平台运行、双向真机分享和 Codex 桌面版历史可见性仍需按上述文档执行发布前验收。当前 macOS 已使用 Codex CLI 0.167.2 验证公开历史接口，单机结果不能替代双平台真机验收。

`.github/workflows/build.yml` 在 macOS 和 Windows runner 上运行配置检查、前端测试、Rust 测试、clippy 和未签名发布构建；正式签名与公证不在第一版开发构建范围内。
