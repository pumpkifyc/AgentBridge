import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const config = JSON.parse(readFileSync(new URL("../src-tauri/tauri.conf.json", import.meta.url), "utf8"));
const cargo = readFileSync(new URL("../src-tauri/Cargo.toml", import.meta.url), "utf8");
const workflow = readFileSync(new URL("../.github/workflows/build.yml", import.meta.url), "utf8");
const acceptance = readFileSync(new URL("../docs/testing/cross-platform-acceptance.md", import.meta.url), "utf8");
const checklist = readFileSync(new URL("./share-import-checklist.md", import.meta.url), "utf8");

test("macOS 与 Windows 安装包目标都已配置", () => {
  assert.deepEqual(config.bundle.targets, ["app", "dmg", "nsis"]);
  assert.equal(config.bundle.createUpdaterArtifacts, false);
  assert.equal(config.bundle.macOS.minimumSystemVersion, "11.0");
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
  for (const item of ["选择本机目录", "确认导入", "复制续接提示", "删除记录"]) {
    assert.ok(checklist.includes(item), `验收清单缺少步骤：${item}`);
  }
  assert.match(checklist, /C:\\Users/);
  assert.match(checklist, /\/Users\//);
});
