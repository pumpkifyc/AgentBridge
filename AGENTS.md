# AgentBridge 项目约定

- 每次桌面版更新完成并通过验证后，必须把当前平台的最新构建安装到本机。
- macOS 使用 `npm run tauri:install`；该命令会构建应用、退出正在运行的旧版、覆盖安装到 `/Applications/AgentBridge.app`，然后重新打开。
- Windows 使用同一命令；安装脚本会静默运行最新的 NSIS 安装包。
- `npm run tauri:build` 仅用于 CI 或明确只需要构建制品的场景，避免 CI 尝试修改 runner 的应用安装目录。
