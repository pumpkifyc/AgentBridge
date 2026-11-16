import { beforeEach, describe, expect, it, vi } from "vitest";
import { getVersion } from "@tauri-apps/api/app";
import { check } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { createTauriUpdater } from "./updaterService";
import type { UpdaterDownloadEvent, UpdateProgress } from "./updateTypes";

vi.mock("@tauri-apps/api/app", () => ({
  getVersion: vi.fn(),
}));

vi.mock("@tauri-apps/plugin-updater", () => ({
  check: vi.fn(),
}));

vi.mock("@tauri-apps/plugin-process", () => ({
  relaunch: vi.fn(),
}));

const getVersionMock = vi.mocked(getVersion);
const checkMock = vi.mocked(check);
const relaunchMock = vi.mocked(relaunch);

interface FakeUpdateOptions {
  version?: string;
  body?: string;
  date?: string;
  contentLength?: number;
  chunkLength?: number;
  downloadError?: Error;
}

function fakeUpdate(options: FakeUpdateOptions = {}) {
  const download = vi.fn(
    async (onEvent: (event: UpdaterDownloadEvent) => void) => {
      onEvent({
        event: "Started",
        data: { contentLength: options.contentLength },
      });
      onEvent({
        event: "Progress",
        data: { chunkLength: options.chunkLength ?? 512 },
      });
      if (options.downloadError) {
        throw options.downloadError;
      }
      onEvent({ event: "Finished" });
    },
  );
  return {
    currentVersion: "0.1.0",
    version: options.version ?? "0.1.1",
    body:
      "body" in options
        ? options.body
        : "## 0.1.1\n\n- 支持应用内更新。",
    date: options.date ?? "2026-11-16T08:00:00.000Z",
    download,
    install: vi.fn().mockResolvedValue(undefined),
    close: vi.fn().mockResolvedValue(undefined),
  };
}

describe("Tauri 更新服务", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    getVersionMock.mockResolvedValue("0.1.0");
    relaunchMock.mockResolvedValue(undefined);
    checkMock.mockResolvedValue(fakeUpdate() as never);
  });

  it("读取当前版本但不检查更新", async () => {
    const updater = createTauriUpdater();

    expect(await updater.currentVersion()).toBe("0.1.0");
    expect(checkMock).not.toHaveBeenCalled();
  });

  it("将 Tauri Update 转换为带默认说明的可展示对象", async () => {
    checkMock.mockResolvedValue(fakeUpdate({ body: undefined }) as never);
    const updater = createTauriUpdater();

    expect(await updater.check()).toMatchObject({
      currentVersion: "0.1.0",
      version: "0.1.1",
      notes: "",
    });
  });

  it("累计下载字节并在已知总大小时计算百分比", async () => {
    checkMock.mockResolvedValue(fakeUpdate({ contentLength: 1024 }) as never);
    const updater = createTauriUpdater();
    const update = await updater.check();
    const events: UpdateProgress[] = [];

    await updater.download(update!, (progress) => events.push(progress));

    expect(events[events.length - 1]).toEqual({
      downloadedBytes: 512,
      totalBytes: 1024,
      percent: 50,
    });
  });

  it("未知总大小时不伪造下载百分比", async () => {
    checkMock.mockResolvedValue(
      fakeUpdate({ contentLength: undefined }) as never,
    );
    const updater = createTauriUpdater();
    const update = await updater.check();
    const events: UpdateProgress[] = [];

    await updater.download(update!, (progress) => events.push(progress));

    expect(events[events.length - 1]).toEqual({ downloadedBytes: 512 });
  });

  it("下载阶段识别签名错误并包装为 signature", async () => {
    checkMock.mockResolvedValue(
      fakeUpdate({ downloadError: new Error("invalid signature") }) as never,
    );
    const updater = createTauriUpdater();
    const update = await updater.check();

    await expect(
      updater.download(update!, () => {}),
    ).rejects.toMatchObject({
      code: "signature",
    });
  });

  it("安装时要求 Windows 重启并随后调用 relaunch", async () => {
    const updater = createTauriUpdater();
    const update = await updater.check();

    await updater.installAndRestart(update!);

    expect(update!.handle.install).toHaveBeenCalledWith({
      restartAfterInstall: true,
    });
    expect(relaunchMock).toHaveBeenCalledOnce();
  });

  it("放弃更新时关闭 Tauri 资源", async () => {
    const updater = createTauriUpdater();
    const update = await updater.check();

    await updater.discard(update!);

    expect(update!.handle.close).toHaveBeenCalledOnce();
  });
});
