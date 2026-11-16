import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const config = JSON.parse(readFileSync(new URL("../src-tauri/tauri.conf.json", import.meta.url), "utf8"));
const releaseConfig = JSON.parse(readFileSync(new URL("../src-tauri/tauri.release.conf.json", import.meta.url), "utf8"));
const packageJson = JSON.parse(readFileSync(new URL("../package.json", import.meta.url), "utf8"));
const cargo = readFileSync(new URL("../src-tauri/Cargo.toml", import.meta.url), "utf8");
const lib = readFileSync(new URL("../src-tauri/src/lib.rs", import.meta.url), "utf8");
const capability = JSON.parse(readFileSync(new URL("../src-tauri/capabilities/default.json", import.meta.url), "utf8"));
const workflow = readFileSync(new URL("../.github/workflows/build.yml", import.meta.url), "utf8");
const releaseWorkflow = readFileSync(new URL("../.github/workflows/release.yml", import.meta.url), "utf8");
const acceptance = readFileSync(new URL("../docs/testing/cross-platform-acceptance.md", import.meta.url), "utf8");
const checklist = readFileSync(new URL("./share-import-checklist.md", import.meta.url), "utf8");
const readme = readFileSync(new URL("../README.md", import.meta.url), "utf8");
const updaterAcceptance = readFileSync(new URL("../docs/testing/in-app-updater-acceptance.md", import.meta.url), "utf8");

test("macOS 与 Windows 安装包目标都已配置", () => {
  assert.deepEqual(config.bundle.targets, ["app", "dmg", "nsis"]);
  assert.equal(config.bundle.createUpdaterArtifacts, false);
  assert.equal(config.bundle.macOS.minimumSystemVersion, "11.0");
});

test("桌面窗口以工作台尺寸打开并保留可用的最小边界", () => {
  assert.deepEqual(config.app.windows[0], {
    title: "AgentBridge",
    width: 1450,
    height: 900,
    minWidth: 1040,
    minHeight: 720,
    center: true,
  });
});

test("应用配置公开更新来源并保留最小权限", () => {
  assert.deepEqual(config.plugins.updater.endpoints, [
    "https://github.com/pumpkifyc/AgentBridge/releases/latest/download/latest.json",
  ]);
  assert.ok(config.plugins.updater.pubkey.length > 20);
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

test("正式发布版本在三处配置中保持一致", () => {
  assert.equal(packageJson.version, "0.1.1");
  assert.equal(config.version, "0.1.1");
  assert.match(cargo, /^version = "0\.1\.1"$/m);
});

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
  assert.doesNotMatch(releaseWorkflow, /npm run tauri:build/);
});

test("Windows NSIS 使用当前用户安装且配置可被 Tauri 解析", () => {
  assert.equal(config.bundle.windows.nsis.installMode, "currentUser");
  assert.match(workflow, /macos-15/);
  assert.match(workflow, /windows-2022/);
});

test("存在测试辅助二进制时仍明确指定应用主二进制", () => {
  assert.match(cargo, /^default-run = "agentbridge"$/m);
  assert.match(cargo, /^\[\[bin\]\][\s\S]*?^name = "agentbridge"$/m);
});

test("CI 不写入签名密钥，并在双平台执行统一验证与构建", () => {
  assert.doesNotMatch(workflow, /APPLE_CERTIFICATE|WINDOWS_CERTIFICATE|SIGNING_SECRET/);
  for (const step of [
    "npm ci",
    "npm run test:e2e-config",
    "npm test -- --run",
    "cargo test --manifest-path src-tauri/Cargo.toml",
    "cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings",
    "npm run tauri:build",
  ]) {
    assert.ok(workflow.includes(step), `CI 缺少步骤：${step}`);
  }
  assert.doesNotMatch(workflow, /gh release create|tauri-apps\/tauri-action/);
});

test("中文验收文档覆盖双向真机与发布前未完成项", () => {
  assert.match(acceptance, /macOS \| Windows/);
  assert.match(acceptance, /Windows \| macOS/);
  assert.match(acceptance, /经双平台验证的最低 Codex 版本：待填写/);
  assert.match(acceptance, /发布门槛/);
  for (const item of [
    "过期配对",
    "拒绝分享",
    "传输中断",
    "重试",
    "Codex CLI 缺失",
    "桌面版可见",
    "后备",
    "清理",
  ]) {
    assert.ok(acceptance.includes(item), `验收文档缺少：${item}`);
  }
  for (const item of ["选择本机目录", "确认导入", "重启 Codex", "删除记录"]) {
    assert.ok(checklist.includes(item), `验收清单缺少步骤：${item}`);
  }
  assert.match(checklist, /C:\\Users/);
  assert.match(checklist, /\/Users\//);
});

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
    assert.ok(`${readme}\n${updaterAcceptance}`.includes(text), `文档缺少：${text}`);
  }
  assert.match(updaterAcceptance, /Apple Silicon/);
  assert.match(updaterAcceptance, /Intel Mac/);
  assert.match(updaterAcceptance, /Windows x64/);
  assert.match(updaterAcceptance, /网络中断/);
  assert.match(updaterAcceptance, /签名校验失败/);
  assert.match(updaterAcceptance, /磁盘空间不足/);
  assert.match(updaterAcceptance, /应用文件被占用/);
  assert.match(updaterAcceptance, /应用数据保持不变/);
});
