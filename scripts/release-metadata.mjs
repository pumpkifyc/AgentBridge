import {
  readFileSync,
  readdirSync,
  statSync,
  writeFileSync,
} from "node:fs";
import { basename, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const REPOSITORY = "pumpkifyc/AgentBridge";
const REQUIRED_PLATFORMS = [
  "darwin-aarch64",
  "darwin-x86_64",
  "windows-x86_64",
];
const STABLE_VERSION = /^\d+\.\d+\.\d+$/;
const ROOT = fileURLToPath(new URL("../", import.meta.url));

function fail(message) {
  throw new Error(message);
}

function requireStableVersion(version) {
  if (typeof version !== "string" || !STABLE_VERSION.test(version)) {
    fail(`版本 ${String(version)} 不是有效的稳定版本号`);
  }
}

function escapeRegExp(value) {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

function extractVersionSection(changelog, version) {
  const heading = new RegExp(
    `^## \\[${escapeRegExp(version)}\\] - (\\d{4}-\\d{2}-\\d{2})\\s*$`,
    "m",
  );
  const match = heading.exec(changelog);
  if (!match) {
    fail(`CHANGELOG.md 缺少 ${version} 版本段落`);
  }

  const remainder = changelog.slice(match.index + match[0].length);
  const nextHeading = remainder.search(/^## \[/m);
  const notes = (nextHeading === -1 ? remainder : remainder.slice(0, nextHeading))
    .trim();
  if (!notes) {
    fail(`CHANGELOG.md 的 ${version} 版本段落没有更新说明`);
  }
  return { releaseDate: match[1], notes };
}

export function validateRelease({
  tag,
  packageVersion,
  cargoVersion,
  tauriVersion,
  changelog,
}) {
  requireStableVersion(packageVersion);
  if (tag !== `v${packageVersion}`) {
    fail(`发布标签 ${tag} 与 package.json 版本 ${packageVersion} 不一致`);
  }
  if (cargoVersion !== packageVersion) {
    fail(
      `Cargo.toml 版本 ${cargoVersion} 与 package.json 版本 ${packageVersion} 不一致`,
    );
  }
  if (tauriVersion !== packageVersion) {
    fail(
      `tauri.conf.json 版本 ${tauriVersion} 与 package.json 版本 ${packageVersion} 不一致`,
    );
  }
  const { releaseDate, notes } = extractVersionSection(
    changelog,
    packageVersion,
  );
  return { version: packageVersion, releaseDate, notes };
}

export function createLatestManifest({
  version,
  tag,
  notes,
  pubDate,
  artifacts,
}) {
  requireStableVersion(version);
  if (tag !== `v${version}`) {
    fail(`更新清单标签 ${tag} 与版本 ${version} 不一致`);
  }
  if (typeof notes !== "string" || !notes.trim()) {
    fail("更新清单缺少版本说明");
  }
  if (
    typeof pubDate !== "string" ||
    Number.isNaN(Date.parse(pubDate)) ||
    !pubDate.includes("T")
  ) {
    fail(`发布日期不是有效的 RFC 3339 时间：${String(pubDate)}`);
  }

  const platforms = {};
  const seen = new Set();
  for (const artifact of artifacts) {
    const { platform, fileName, signature } = artifact;
    if (!REQUIRED_PLATFORMS.includes(platform)) {
      fail(`更新清单包含未知平台 ${platform}`);
    }
    if (seen.has(platform)) {
      fail(`更新清单包含重复平台 ${platform}`);
    }
    seen.add(platform);
    if (typeof signature !== "string" || !signature.trim()) {
      fail(`平台 ${platform} 缺少更新签名`);
    }
    if (
      typeof fileName !== "string" ||
      !fileName ||
      basename(fileName) !== fileName ||
      fileName.includes("\\")
    ) {
      fail(`制品文件名不能包含路径：${String(fileName)}`);
    }
    platforms[platform] = {
      signature: signature.trim(),
      url: `https://github.com/${REPOSITORY}/releases/download/${tag}/${encodeURIComponent(fileName)}`,
    };
  }

  for (const platform of REQUIRED_PLATFORMS) {
    if (!platforms[platform]) {
      fail(`更新清单缺少平台 ${platform}`);
    }
  }

  return {
    version,
    notes,
    pub_date: new Date(pubDate).toISOString(),
    platforms,
  };
}

function option(args, name) {
  const index = args.indexOf(name);
  if (index === -1 || !args[index + 1]) {
    fail(`缺少参数 ${name}`);
  }
  return args[index + 1];
}

function readJson(path) {
  return JSON.parse(readFileSync(path, "utf8"));
}

function repositoryVersions(repoRoot) {
  const packageVersion = readJson(resolve(repoRoot, "package.json")).version;
  const cargoText = readFileSync(resolve(repoRoot, "src-tauri/Cargo.toml"), "utf8");
  const cargoVersion = /^\[package\][\s\S]*?^version\s*=\s*"([^"]+)"/m.exec(
    cargoText,
  )?.[1];
  const tauriVersion = readJson(
    resolve(repoRoot, "src-tauri/tauri.conf.json"),
  ).version;
  const changelog = readFileSync(resolve(repoRoot, "CHANGELOG.md"), "utf8");
  return { packageVersion, cargoVersion, tauriVersion, changelog };
}

function collectUpdaterTargets(directory) {
  const targets = [];
  for (const name of readdirSync(directory)) {
    const path = resolve(directory, name);
    if (statSync(path).isDirectory()) {
      targets.push(...collectUpdaterTargets(path));
    } else if (name === "updater-target.json") {
      targets.push(readJson(path));
    }
  }
  return targets;
}

function writeOutput(path, content) {
  if (path) {
    writeFileSync(path, content.endsWith("\n") ? content : `${content}\n`);
    return;
  }
  process.stdout.write(content.endsWith("\n") ? content : `${content}\n`);
}

function main() {
  const [command, ...args] = process.argv.slice(2);
  const repoRoot =
    args.find((value) => value === "--root") === undefined
      ? ROOT
      : args[args.indexOf("--root") + 1];

  if (command === "validate") {
    const release = validateRelease({
      tag: option(args, "--tag"),
      ...repositoryVersions(repoRoot),
    });
    process.stdout.write(
      `发布校验通过：v${release.version}（${release.releaseDate}）\n`,
    );
    return;
  }

  if (command === "notes") {
    const release = validateRelease({
      tag: option(args, "--tag"),
      ...repositoryVersions(repoRoot),
    });
    writeOutput(args.includes("--output") ? option(args, "--output") : null, release.notes);
    return;
  }

  if (command === "manifest") {
    const tag = option(args, "--tag");
    const release = validateRelease({
      tag,
      ...repositoryVersions(repoRoot),
    });
    const manifest = createLatestManifest({
      version: release.version,
      tag,
      notes: release.notes,
      pubDate: args.includes("--pub-date")
        ? option(args, "--pub-date")
        : `${release.releaseDate}T00:00:00.000Z`,
      artifacts: collectUpdaterTargets(resolve(option(args, "--artifacts"))),
    });
    writeOutput(option(args, "--output"), JSON.stringify(manifest, null, 2));
    return;
  }

  fail("用法：release-metadata.mjs <validate|notes|manifest> [参数]");
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  try {
    main();
  } catch (error) {
    process.stderr.write(`${error.message}\n`);
    process.exitCode = 1;
  }
}
