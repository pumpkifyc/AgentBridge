# 应用内更新真机验收

文档日期：2026-11-16  
适用范围：AgentBridge `0.1.1` 应用内手动更新

## 验收原则

- 应用不会自动安装更新，只有用户依次完成「检查更新」「下载并更新」「确认更新」后才开始下载。
- 每次正常升级从 `0.1.0 → 0.1.1`，并保留升级前的本机应用数据。
- Tauri 更新签名、Apple 签名与公证、Windows 代码签名分别记录；任一 CI 构建成功都不能替代真机升级。
- 未执行或失败的项目保留原记录，不填写“通过”。

## 运行记录

每次填写一行：

| 日期 | 执行人 | 系统与架构 | 当前版本 | 目标版本 | 安装来源 | 结果 | 记录位置 |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 2026-11-16 | Codex | Apple Silicon macOS | 0.1.0 | 0.1.1 | GitHub Release | 通过 | 本文“Apple Silicon macOS 实测结果” |
|  |  | Intel Mac | 0.1.0 | 0.1.1 | GitHub Release | 未执行 |  |
|  |  | Windows x64 | 0.1.0 | 0.1.1 | GitHub Release | 未执行 |  |

表头之外记录操作系统版本、AgentBridge 初始安装包、网络环境、Codex 桌面版版本和 `codex --version`。

### Apple Silicon macOS 实测结果

测试环境为 macOS 15.6.0、Apple Silicon，Codex 桌面版 `26.1111.21308`，`codex-cli 0.167.2`。执行流程与结果如下：

1. 安装并确认 `0.1.0` 基线后，顶栏显示“当前版本 v0.1.0”。
2. 点击「检查更新」后，应用显示 `v0.1.0 → v0.1.1`、发布日期 `2026/11/16` 和与 `CHANGELOG.md` 一致的中文更新说明。
3. 点击「下载并更新」后只出现二次确认弹窗；再次点击「确认更新」后才开始下载、验证 Tauri 更新签名、退出、安装并重启。
4. 重启后 `/Applications/AgentBridge.app` 的 `CFBundleShortVersionString` 和界面顶栏均为 `0.1.1`。
5. 升级前后应用数据目录的 4 个文件 SHA-256 完全一致，现有数据库、暂存快照和其他应用数据均未改变。
6. 升级后重新加载出 `52 / 52` 个 Codex 会话；成功生成一份 `52` 轮、`6,285` 个事件的完整分享预览；「导入历史」成功读取完成与失败记录；首页、分享会话和导入历史之间切换正常。

## 正常升级流程

以下步骤在 Apple Silicon macOS、Intel Mac 和 Windows x64 分别执行：

1. 确认当前安装版本为 `0.1.0`，打开应用并确认启动后不会自动请求更新。
2. 点击「检查更新」，确认弹窗显示 `0.1.0 → 0.1.1`、发布日期和中文更新说明。
3. 点击「下载并更新」，确认此时尚未开始下载，并显示退出、重启和失败保留原版本的说明。
4. 点击「确认更新」，确认出现真实下载进度、总大小和完成状态。
5. 等待 Tauri 更新签名验证、安装和应用重启；确认重启后的版本显示 `0.1.1`。
6. 检查现有导入历史、暂存会话、项目目录、分享设置和其他应用数据，要求**应用数据保持不变**。
7. 执行一次分享预览、一次导入历史读取和一次页面切换，确认前端、Rust 后端和原生依赖均可运行。
8. 记录结果、截图或日志位置。CI 构建或单独打开安装包不算应用内升级通过。

## 失败与恢复

每个平台至少执行网络中断和签名校验失败；磁盘空间不足、应用文件被占用、重启失败在具备可控测试环境时执行。

| 场景 | 操作 | 预期 | macOS | Windows x64 |
| --- | --- | --- | --- | --- |
| 网络中断 | 下载中切断网络 | 显示下载失败；旧版本继续可启动；有重试入口 | 未执行 | 未执行 |
| 签名校验失败 | 使用测试清单指向签名不匹配的制品 | 显示签名校验失败，不运行安装器，旧版本不变 | 未执行 | 未执行 |
| 磁盘空间不足 | 在剩余空间不足的卷或测试环境下载 | 干净失败并提示检查网络和磁盘空间 | 未执行 | 未执行 |
| 应用文件被占用 | 模拟安装目标不可替换或被占用 | 安装失败，不把状态标记为成功；旧版本可启动 | 未执行 | 未执行 |
| 重启失败 | 安装后阻止应用自动重开 | 提示用户手动重新打开 AgentBridge | 未执行 | 未执行 |

失败后都需再次启动当前已安装版本并核对应用数据，避免只验证进程退出而遗漏数据损坏。

## Release 核对

1. 不登录 GitHub，访问：

```text
https://github.com/pumpkifyc/AgentBridge/releases/latest/download/latest.json
```

2. 确认 `version` 为 `0.1.1`，`notes` 与 `CHANGELOG.md` 的 `0.1.1` 段落一致。
3. 确认清单包含 `darwin-aarch64`、`darwin-x86_64` 和 `windows-x86_64`，每项均有非空签名和可匿名下载的 URL。
4. 确认 macOS 两种架构指向同一 universal 更新制品，Windows 指向 x64 NSIS 安装包。
5. 下载 DMG、应用归档和 NSIS 安装包，确认 Release 正文、应用更新弹窗和 `latest.json` 使用同一份中文说明。
6. 记录 Tauri 更新签名结果、Apple 签名与公证状态、Windows 代码签名状态和相应系统警告。

### 本次 Release 核对结果

- 发布运行：[`43809834082`](https://github.com/pumpkifyc/AgentBridge/actions/runs/43809834082)，全部 job 成功。
- Release：[`v0.1.1`](https://github.com/pumpkifyc/AgentBridge/releases/tag/v0.1.1)，已于 `2026-11-16T05:39:49Z` 公开。
- 未登录 GitHub 访问 `releases/latest/download/latest.json` 返回 HTTP 200，版本为 `0.1.1`，说明与 `CHANGELOG.md` 的 `0.1.1` 段落一致。
- `darwin-aarch64`、`darwin-x86_64` 和 `windows-x86_64` 均存在非空 Tauri 更新签名；两种 macOS 架构指向同一个 universal `.app.tar.gz`。
- macOS 更新包、universal DMG 和 Windows x64 NSIS 安装包的公开下载地址均返回 HTTP 200。
- Tauri 更新签名已通过；Apple Developer ID 签名与公证、Windows Authenticode 代码签名尚未配置，系统可能分别显示 Gatekeeper 或 SmartScreen 提示。

## 完成标准

- Apple Silicon macOS、Intel Mac 和 Windows x64 的正常升级行均有真实结果。
- 网络中断与签名校验失败在每个实际发布平台通过，其他失败场景有结果或明确未执行原因。
- Tauri 更新签名通过，且发布说明、版本和清单一致。
- 升级后的应用数据保持不变，核心分享和导入功能可运行。
- 尚未完成的操作系统签名、公证或真机项写入已知限制，不能以 CI 绿灯替代。

本次 Apple Silicon macOS 正常升级、发布产物和公开更新清单已完成验收；Intel Mac、Windows x64 真机升级，以及网络中断、签名篡改等异常矩阵仍保留为“未执行”。
