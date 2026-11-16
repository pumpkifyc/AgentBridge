import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { UpdateControl } from "./UpdateControl";
import {
  UpdaterError,
  type AvailableUpdate,
  type ManualUpdater,
  type UpdateProgress,
} from "./updateTypes";

function deferred<T>() {
  let resolve!: (value: T | PromiseLike<T>) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });
  return { promise, resolve, reject };
}

function fakeOffer(notes = "### 新增\n\n- 支持应用内更新。"): AvailableUpdate {
  return {
    currentVersion: "0.1.0",
    version: "0.1.1",
    date: "2026-11-16T08:00:00.000Z",
    notes,
    handle: {
      download: vi.fn(),
      install: vi.fn(),
      close: vi.fn(),
    },
  };
}

interface FakeOptions {
  checkResult?: AvailableUpdate | null;
  checkError?: Error;
  checkGate?: Promise<AvailableUpdate | null>;
  downloadError?: Error;
  downloadGate?: Promise<void>;
}

function createFakeUpdater(options: FakeOptions = {}) {
  let progressCallback: ((progress: UpdateProgress) => void) | null = null;
  const check = vi.fn(async () => {
    if (options.checkError) {
      throw options.checkError;
    }
    if (options.checkGate) {
      return options.checkGate;
    }
    return options.checkResult === undefined
      ? fakeOffer()
      : options.checkResult;
  });
  const download = vi.fn(
    async (
      _update: AvailableUpdate,
      onProgress: (progress: UpdateProgress) => void,
    ) => {
      progressCallback = onProgress;
      onProgress({
        downloadedBytes: 0,
        totalBytes: 1024,
        percent: 0,
      });
      if (options.downloadGate) {
        await options.downloadGate;
      }
      if (options.downloadError) {
        throw options.downloadError;
      }
      onProgress({
        downloadedBytes: 1024,
        totalBytes: 1024,
        percent: 100,
      });
    },
  );
  const installAndRestart = vi.fn().mockResolvedValue(undefined);
  const discard = vi.fn().mockResolvedValue(undefined);
  const currentVersion = vi.fn().mockResolvedValue("0.1.0");
  const updater: ManualUpdater = {
    currentVersion,
    check,
    download,
    installAndRestart,
    discard,
  };
  return {
    updater,
    check,
    download,
    installAndRestart,
    discard,
    currentVersion,
    emitProgress(progress: UpdateProgress) {
      progressCallback?.(progress);
    },
  };
}

describe("手动更新入口", () => {
  it("启动时只读取版本，不检查更新", async () => {
    const fake = createFakeUpdater();
    render(<UpdateControl updater={fake.updater} />);

    expect(
      await screen.findByText("当前版本 v0.1.0"),
    ).toBeInTheDocument();
    expect(fake.currentVersion).toHaveBeenCalledOnce();
    expect(fake.check).not.toHaveBeenCalled();
  });

  it("当前已是最新版时显示检查时间", async () => {
    const fake = createFakeUpdater({ checkResult: null });
    render(<UpdateControl updater={fake.updater} />);

    fireEvent.click(await screen.findByRole("button", { name: "检查更新" }));

    const dialog = await screen.findByRole("dialog", {
      name: "当前已是最新版本",
    });
    expect(dialog).toHaveTextContent("当前版本 v0.1.0");
    expect(dialog).toHaveTextContent(/最后检查：/);
    expect(fake.check).toHaveBeenCalledOnce();
  });

  it("展示更新说明并经过二次确认后才下载", async () => {
    const fake = createFakeUpdater();
    render(<UpdateControl updater={fake.updater} />);

    fireEvent.click(await screen.findByRole("button", { name: "检查更新" }));
    const dialog = await screen.findByRole("dialog", { name: "发现新版本" });
    expect(dialog).toHaveTextContent("v0.1.0");
    expect(dialog).toHaveTextContent("v0.1.1");
    expect(dialog).toHaveTextContent("支持应用内更新");

    fireEvent.click(screen.getByRole("button", { name: "下载并更新" }));

    expect(fake.download).not.toHaveBeenCalled();
    expect(
      screen.getByText(/更新过程中应用将退出并重启/),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("dialog", { name: "确认更新" }),
    ).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "确认更新" }));

    await waitFor(() => {
      expect(fake.download).toHaveBeenCalledOnce();
      expect(fake.installAndRestart).toHaveBeenCalledOnce();
    });
  });

  it("显示下载进度并在更新期间禁止重复操作", async () => {
    const gate = deferred<void>();
    const fake = createFakeUpdater({ downloadGate: gate.promise });
    render(<UpdateControl updater={fake.updater} />);

    fireEvent.click(await screen.findByRole("button", { name: "检查更新" }));
    fireEvent.click(await screen.findByRole("button", { name: "下载并更新" }));
    fireEvent.click(screen.getByRole("button", { name: "确认更新" }));

    await waitFor(() => expect(fake.download).toHaveBeenCalledOnce());
    act(() => {
      fake.emitProgress({
        downloadedBytes: 512,
        totalBytes: 1024,
        percent: 50,
      });
    });

    expect(await screen.findByText("正在下载 50%")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "检查更新" })).toBeDisabled();
    expect(
      screen.queryByRole("button", { name: "关闭" }),
    ).not.toBeInTheDocument();

    await act(async () => gate.resolve());
  });

  it("检查进行中重复点击只发起一次请求", async () => {
    const checkGate = deferred<AvailableUpdate | null>();
    const fake = createFakeUpdater({ checkGate: checkGate.promise });
    render(<UpdateControl updater={fake.updater} />);

    const button = await screen.findByRole("button", { name: "检查更新" });
    fireEvent.click(button);
    fireEvent.click(button);

    expect(await screen.findByRole("dialog", {
      name: "正在检查更新",
    })).toBeInTheDocument();
    expect(button).toBeDisabled();
    expect(fake.check).toHaveBeenCalledOnce();

    await act(async () => checkGate.resolve(null));
  });

  it("稍后再说会关闭弹窗并释放更新资源", async () => {
    const fake = createFakeUpdater();
    render(<UpdateControl updater={fake.updater} />);

    fireEvent.click(await screen.findByRole("button", { name: "检查更新" }));
    fireEvent.click(await screen.findByRole("button", { name: "稍后再说" }));

    await waitFor(() => expect(fake.discard).toHaveBeenCalledOnce());
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("取消第二确认会关闭弹窗并释放更新资源", async () => {
    const fake = createFakeUpdater();
    render(<UpdateControl updater={fake.updater} />);

    fireEvent.click(await screen.findByRole("button", { name: "检查更新" }));
    fireEvent.click(await screen.findByRole("button", { name: "下载并更新" }));
    fireEvent.click(screen.getByRole("button", { name: "取消" }));

    await waitFor(() => expect(fake.discard).toHaveBeenCalledOnce());
    expect(fake.download).not.toHaveBeenCalled();
  });

  it("检查失败显示中文原因并允许重试检查", async () => {
    const fake = createFakeUpdater({
      checkError: new UpdaterError("network", "offline"),
    });
    render(<UpdateControl updater={fake.updater} />);

    fireEvent.click(await screen.findByRole("button", { name: "检查更新" }));

    expect(
      await screen.findByText("无法连接更新服务器，请检查网络后重试。"),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "重试检查" }),
    ).toBeInTheDocument();
  });

  it("下载失败显示签名错误并允许重试下载", async () => {
    const fake = createFakeUpdater({
      downloadError: new UpdaterError("signature", "bad signature"),
    });
    render(<UpdateControl updater={fake.updater} />);

    fireEvent.click(await screen.findByRole("button", { name: "检查更新" }));
    fireEvent.click(await screen.findByRole("button", { name: "下载并更新" }));
    fireEvent.click(screen.getByRole("button", { name: "确认更新" }));

    expect(
      await screen.findByText("更新包签名校验失败，当前版本未更改。"),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "重试下载" }),
    ).toBeInTheDocument();
  });

  it("更新说明中的脚本只作为文本显示", async () => {
    const fake = createFakeUpdater({
      checkResult: fakeOffer("<script>window.evil = true</script>安全说明"),
    });
    render(<UpdateControl updater={fake.updater} />);

    fireEvent.click(await screen.findByRole("button", { name: "检查更新" }));
    const dialog = await screen.findByRole("dialog", { name: "发现新版本" });

    expect(dialog).toHaveTextContent("<script>window.evil = true</script>安全说明");
    expect(dialog.querySelector("script")).toBeNull();
  });

  it("可按 Escape 关闭没有执行任务的弹窗", async () => {
    const fake = createFakeUpdater();
    render(<UpdateControl updater={fake.updater} />);

    fireEvent.click(await screen.findByRole("button", { name: "检查更新" }));
    expect(
      await screen.findByRole("dialog", { name: "发现新版本" }),
    ).toBeInTheDocument();

    fireEvent.keyDown(window, { key: "Escape" });

    await waitFor(() => expect(fake.discard).toHaveBeenCalledOnce());
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("检查进行中按 Escape 不会丢失请求", async () => {
    const checkGate = deferred<AvailableUpdate | null>();
    const fake = createFakeUpdater({ checkGate: checkGate.promise });
    render(<UpdateControl updater={fake.updater} />);

    fireEvent.click(await screen.findByRole("button", { name: "检查更新" }));
    fireEvent.keyDown(window, { key: "Escape" });

    expect(
      screen.getByRole("dialog", { name: "正在检查更新" }),
    ).toBeInTheDocument();

    await act(async () => checkGate.resolve(null));
  });

  it("安装调用返回后提示用户手动重新打开应用", async () => {
    const fake = createFakeUpdater();
    render(<UpdateControl updater={fake.updater} />);

    fireEvent.click(await screen.findByRole("button", { name: "检查更新" }));
    fireEvent.click(await screen.findByRole("button", { name: "下载并更新" }));
    fireEvent.click(screen.getByRole("button", { name: "确认更新" }));

    expect(
      await screen.findByText(
        "更新已安装，但应用未能自动重启，请手动重新打开 AgentBridge。",
      ),
    ).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "重试下载" }),
    ).not.toBeInTheDocument();
  });
});
