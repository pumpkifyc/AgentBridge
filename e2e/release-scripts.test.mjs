import assert from "node:assert/strict";
import test from "node:test";
import {
  createLatestManifest,
  validateRelease,
} from "../scripts/release-metadata.mjs";

const FIXTURE_CHANGELOG = `# 更新日志

## [0.1.1] - 2026-11-16

### 新增

- 支持在应用内检查、确认并安装新版本。

## [0.1.0] - 2026-11-10

- 初始版本。
`;

const validRelease = {
  tag: "v0.1.1",
  packageVersion: "0.1.1",
  cargoVersion: "0.1.1",
  tauriVersion: "0.1.1",
  changelog: FIXTURE_CHANGELOG,
};

const validManifestInput = {
  version: "0.1.1",
  tag: "v0.1.1",
  notes: "### 新增\n\n- 支持应用内更新。",
  pubDate: "2026-11-16T08:00:00.000Z",
  artifacts: [
    {
      platform: "darwin-aarch64",
      fileName: "AgentBridge.app.tar.gz",
      signature: "mac-signature",
    },
    {
      platform: "darwin-x86_64",
      fileName: "AgentBridge.app.tar.gz",
      signature: "mac-signature",
    },
    {
      platform: "windows-x86_64",
      fileName: "AgentBridge_0.1.1_x64-setup.exe",
      signature: "windows-signature",
    },
  ],
};

test("从中文 CHANGELOG 精确提取标签版本", () => {
  assert.deepEqual(validateRelease(validRelease), {
    version: "0.1.1",
    releaseDate: "2026-11-16",
    notes: "### 新增\n\n- 支持在应用内检查、确认并安装新版本。",
  });
});

test("标签与版本不一致时拒绝发布", () => {
  assert.throws(
    () => validateRelease({ ...validRelease, tag: "v0.1.2" }),
    /发布标签 v0\.1\.2 与 package\.json 版本 0\.1\.1 不一致/,
  );
});

test("其他版本文件不一致时拒绝发布", () => {
  assert.throws(
    () => validateRelease({ ...validRelease, cargoVersion: "0.1.0" }),
    /Cargo\.toml 版本 0\.1\.0 与 package\.json 版本 0\.1\.1 不一致/,
  );
  assert.throws(
    () => validateRelease({ ...validRelease, tauriVersion: "0.2.0" }),
    /tauri\.conf\.json 版本 0\.2\.0 与 package\.json 版本 0\.1\.1 不一致/,
  );
});

test("缺少版本段落时拒绝发布", () => {
  assert.throws(
    () =>
      validateRelease({
        ...validRelease,
        changelog: "# 更新日志\n\n## [0.1.0] - 2026-11-10\n",
      }),
    /CHANGELOG\.md 缺少 0\.1\.1 版本段落/,
  );
});

test("macOS 两种架构共享 universal 制品，Windows 使用 NSIS", () => {
  const manifest = createLatestManifest(validManifestInput);

  assert.equal(
    manifest.platforms["darwin-aarch64"].url,
    manifest.platforms["darwin-x86_64"].url,
  );
  assert.equal(
    manifest.platforms["darwin-aarch64"].url,
    "https://github.com/pumpkifyc/AgentBridge/releases/download/v0.1.1/AgentBridge.app.tar.gz",
  );
  assert.equal(
    manifest.platforms["windows-x86_64"].url,
    "https://github.com/pumpkifyc/AgentBridge/releases/download/v0.1.1/AgentBridge_0.1.1_x64-setup.exe",
  );
  assert.equal(manifest.pub_date, "2026-11-16T08:00:00.000Z");
  assert.equal(manifest.notes, validManifestInput.notes);
  assert.equal(manifest.version, "0.1.1");
});

test("缺失平台、重复平台或空签名时拒绝生成清单", () => {
  assert.throws(
    () =>
      createLatestManifest({
        ...validManifestInput,
        artifacts: validManifestInput.artifacts.slice(0, 2),
      }),
    /更新清单缺少平台 windows-x86_64/,
  );
  assert.throws(
    () =>
      createLatestManifest({
        ...validManifestInput,
        artifacts: [
          ...validManifestInput.artifacts.slice(0, 2),
          validManifestInput.artifacts[0],
          validManifestInput.artifacts[2],
        ],
      }),
    /更新清单包含重复平台 darwin-aarch64/,
  );
  assert.throws(
    () =>
      createLatestManifest({
        ...validManifestInput,
        artifacts: validManifestInput.artifacts.map((artifact) =>
          artifact.platform === "windows-x86_64"
            ? { ...artifact, signature: "" }
            : artifact,
        ),
      }),
    /平台 windows-x86_64 缺少更新签名/,
  );
});

test("非法版本、日期或制品文件名时拒绝生成清单", () => {
  assert.throws(
    () => createLatestManifest({ ...validManifestInput, version: "0.1" }),
    /不是有效的稳定版本号/,
  );
  assert.throws(
    () => createLatestManifest({ ...validManifestInput, pubDate: "not-a-date" }),
    /发布日期不是有效的 RFC 3339 时间/,
  );
  assert.throws(
    () =>
      createLatestManifest({
        ...validManifestInput,
        artifacts: validManifestInput.artifacts.map((artifact) =>
          artifact.platform === "windows-x86_64"
            ? { ...artifact, fileName: "../setup.exe" }
            : artifact,
        ),
      }),
    /制品文件名不能包含路径/,
  );
});
