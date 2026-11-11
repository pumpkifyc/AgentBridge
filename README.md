# AgentBridge

AgentBridge 是一款用于在局域网内分享 Codex 会话上下文的 macOS 和 Windows 桌面应用。

## 开发环境

- Node.js、npm、Rust stable 工具链，以及当前平台所需的 Tauri 2 桌面开发依赖。

## 常用命令

- `npm install`：安装前端依赖。
- `npm run tauri:dev`：启动桌面开发版。
- `npm test -- --run`：运行前端测试。
- `npm run build`：构建前端资源。
- `npm run tauri:build`：构建当前平台桌面安装包。

Windows 安装包需在 Windows 主机或 CI runner 上构建。
