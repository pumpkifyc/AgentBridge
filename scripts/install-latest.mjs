import { existsSync, readdirSync, statSync } from "node:fs";
import { basename, join } from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("..", import.meta.url));
const bundleRoot = join(root, "src-tauri", "target", "release", "bundle");

function run(command, args, options = {}) {
  const result = spawnSync(command, args, {
    cwd: root,
    stdio: "inherit",
    ...options,
  });
  if (result.error) {
    throw result.error;
  }
  if (result.status !== 0) {
    throw new Error(`${basename(command)} 退出状态：${result.status}`);
  }
}

function installMac() {
  const source = join(bundleRoot, "macos", "AgentBridge.app");
  const target = "/Applications/AgentBridge.app";
  if (!existsSync(source)) {
    throw new Error(`找不到 macOS 应用：${source}`);
  }
  spawnSync("osascript", ["-e", 'tell application "AgentBridge" to quit'], {
    stdio: "ignore",
  });
  spawnSync("sleep", ["1"]);
  run("ditto", [source, target]);
  run("open", [target]);
}

function installWindows() {
  const nsisRoot = join(bundleRoot, "nsis");
  if (!existsSync(nsisRoot)) {
    throw new Error(`找不到 Windows NSIS 目录：${nsisRoot}`);
  }
  const installers = readdirSync(nsisRoot)
    .filter((name) => name.endsWith(".exe"))
    .map((name) => join(nsisRoot, name))
    .sort((left, right) => statSync(right).mtimeMs - statSync(left).mtimeMs);
  if (installers.length === 0) {
    throw new Error(`NSIS 目录中没有安装包：${nsisRoot}`);
  }
  spawnSync("taskkill", ["/IM", "AgentBridge.exe", "/F"], {
    stdio: "ignore",
  });
  run(installers[0], ["/S"]);
}

if (process.platform === "darwin") {
  installMac();
} else if (process.platform === "win32") {
  installWindows();
} else {
  throw new Error(`当前平台不支持自动安装：${process.platform}`);
}
