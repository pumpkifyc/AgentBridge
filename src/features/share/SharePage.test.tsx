import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { mockTauri, resetTauriMocks } from "../../test-setup";
import { SharePage } from "./SharePage";
import type { SnapshotV1, ThreadSummary, TurnSummary } from "./shareTypes";

const threads: ThreadSummary[] = [
  {
    id: "thread-1",
    title: "修复登录流程",
    preview: "修复登录流程",
    cwd: "/Users/alice/project",
    createdAt: 100,
    updatedAt: 200,
    repositoryId: "repo-1",
  },
  {
    id: "thread-2",
    title: "补充测试",
    preview: "补充测试",
    cwd: "/Users/alice/project",
    createdAt: 101,
    updatedAt: 201,
    repositoryId: "repo-1",
  },
];

const turns: TurnSummary[] = [
  { id: "turn-1", completed: true, startedAt: 110, completedAt: 190 },
  { id: "turn-2", completed: true, startedAt: 210, completedAt: 290 },
  { id: "turn-3", completed: false, startedAt: 310, completedAt: null },
];

function snapshot(overrides: Partial<SnapshotV1> = {}): SnapshotV1 {
  return {
    schemaVersion: 1,
    metadata: {
      sourceThreadId: "thread-1",
      title: "修复登录流程",
      createdAt: 100,
      updatedAt: 300,
      cutoffTurnId: "turn-2",
      projectName: "agentbridge",
      repositoryId: "repo-1",
      sourceWorkingDirectory: "/Users/alice/project",
    },
    messages: [
      {
        role: "user",
        text: "修复登录",
        turnIndex: 0,
        timestamp: 110,
        historicalToolSummary: null,
      },
      {
        role: "assistant",
        text: "我会检查登录流程。",
        turnIndex: 0,
        timestamp: 120,
        historicalToolSummary: "命令执行：找到登录处理器",
      },
    ],
    warnings: ["已省略不支持的 图片附件 内容"],
    ...overrides,
  };
}

function mockAvailable() {
  mockTauri("codex_status", {}, { state: "available", detail: "本机 Codex 可用" });
  mockTauri("list_threads", { cursor: null }, { data: threads, nextCursor: null });
  mockTauri("list_turns", "*", turns);
  mockTauri("preview_snapshot", "*", snapshot());
  mockTauri("cancel_preview", "*", undefined);
  mockTauri("list_local_addresses", {}, [{ ip: "192.168.1.20", name: "en0" }]);
}

let restoreInvoke: (() => void) | null = null;

function captureInvocations() {
  const internals = (
    globalThis as typeof globalThis & {
      __TAURI_INTERNALS__: {
        invoke: (command: string, args?: Record<string, unknown>) => Promise<unknown>;
      };
    }
  ).__TAURI_INTERNALS__;
  const originalInvoke = internals.invoke;
  const calls: Array<{ command: string; args?: Record<string, unknown> }> = [];
  internals.invoke = async (command, args) => {
    calls.push({ command, args });
    return originalInvoke(command, args);
  };
  restoreInvoke = () => {
    internals.invoke = originalInvoke;
  };
  return calls;
}

async function clickPreview() {
  const button = await screen.findByRole("button", { name: "生成分享预览" });
  await waitFor(() => expect(button).toBeEnabled());
  fireEvent.click(button);
}

beforeEach(() => {
  resetTauriMocks();
  vi.restoreAllMocks();
});

afterEach(() => {
  restoreInvoke?.();
  restoreInvoke = null;
});

describe("分享会话页面", () => {
  it("先加载轻量会话列表，只有点击后才读取完整快照", async () => {
    const calls = captureInvocations();
    mockAvailable();
    render(<SharePage />);

    expect(screen.getByText("正在检查本机 Codex…")).toBeInTheDocument();
    expect(await screen.findByText("修复登录流程")).toBeInTheDocument();
    expect(screen.queryByText("快照预览")).not.toBeInTheDocument();
    expect(calls.some((call) => call.command === "preview_snapshot")).toBe(false);

    await clickPreview();

    await waitFor(() => {
      expect(calls).toContainEqual({
        command: "preview_snapshot",
        args: {
          request: {
            threadId: "thread-1",
            cutoffTurnId: null,
            previewId: expect.any(String),
          },
        },
      });
    });
    expect(await screen.findByText("快照预览")).toBeInTheDocument();
    expect(screen.getByText("仅作说明，接收方需选择自己的项目目录", { exact: false })).toBeInTheDocument();
    expect(screen.getByText("已省略不支持的 图片附件 内容")).toBeInTheDocument();
    expect(screen.queryByText("rm -rf /")).not.toBeInTheDocument();
  });

  it("空列表显示明确提示", async () => {
    mockTauri("codex_status", {}, { state: "available", detail: "本机 Codex 可用" });
    mockTauri("list_threads", { cursor: null }, { data: [], nextCursor: null });
    render(<SharePage />);

    expect(await screen.findByText("本机没有可分享的 Codex 会话。")).toBeInTheDocument();
  });

  it("不受支持的 Codex 版本显示升级提示，不读取内部数据库", async () => {
    mockTauri("codex_status", {}, {
      state: "unsupported",
      detail: "当前 CLI 不支持读取桌面版会话历史",
    });
    render(<SharePage />);

    expect(await screen.findByText("当前 Codex 版本不兼容")).toBeInTheDocument();
    expect(screen.getByText("当前 CLI 不支持读取桌面版会话历史")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /修复登录流程/ })).not.toBeInTheDocument();
  });

  it("Codex 不可用错误显示可执行的说明", async () => {
    mockTauri("codex_status", {}, new Error("找不到 Codex CLI"), true);
    mockTauri("codex_status", {}, { state: "available", detail: "本机 Codex 可用" });
    mockTauri("list_threads", { cursor: null }, { data: threads, nextCursor: null });
    mockTauri("list_turns", "*", turns);
    mockTauri("preview_snapshot", "*", snapshot());
    mockTauri("cancel_preview", "*", undefined);
    mockTauri("list_local_addresses", {}, [{ ip: "192.168.1.20", name: "en0" }]);
    render(<SharePage />);

    expect(await screen.findByRole("alert")).toHaveTextContent("找不到 Codex CLI");
    expect(screen.getByText("请检查本机 Codex 安装后重试。", { exact: false })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "重新检查 Codex" }));
    expect(await screen.findByRole("button", { name: "生成分享预览" })).toBeInTheDocument();
    await clickPreview();
    expect(await screen.findByText("快照预览")).toBeInTheDocument();
  });

  it("选择另一个会话后清除旧快照，并要求重新生成预览", async () => {
    const internals = (
      globalThis as typeof globalThis & {
        __TAURI_INTERNALS__: {
          invoke: (command: string, args?: Record<string, unknown>) => Promise<unknown>;
        };
      }
    ).__TAURI_INTERNALS__;
    const originalInvoke = internals.invoke;
    restoreInvoke = () => {
      internals.invoke = originalInvoke;
    };
    internals.invoke = async (command, args) => {
      if (command === "preview_snapshot") {
        const request = (args?.request ?? args) as Record<string, unknown>;
        if (request.threadId === "thread-2") {
          return snapshot({
            metadata: { ...snapshot().metadata, sourceThreadId: "thread-2", title: "补充测试" },
            messages: [{ role: "user", text: "补充回归测试", turnIndex: 0, timestamp: 210, historicalToolSummary: null }],
            warnings: [],
          });
        }
      }
      return originalInvoke(command, args);
    };
    mockAvailable();
    render(<SharePage />);

    await screen.findByRole("button", { name: /修复登录流程/ });
    await clickPreview();
    await screen.findByText("快照预览");

    fireEvent.click(screen.getByRole("button", { name: /补充测试/ }));
    expect(screen.queryByText("快照预览")).not.toBeInTheDocument();

    await clickPreview();
    expect(await screen.findByText("补充回归测试")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /补充测试/ })).toHaveAttribute("aria-pressed", "true");
  });

  it("选择较早的已完成轮次后使用对应截止点重新生成预览", async () => {
    const calls = captureInvocations();
    mockAvailable();
    render(<SharePage />);

    await screen.findByRole("button", { name: "生成分享预览" });
    await clickPreview();
    await screen.findByText("快照预览");

    fireEvent.change(screen.getByLabelText("截止轮次"), { target: { value: "turn-1" } });
    expect(screen.queryByText("快照预览")).not.toBeInTheDocument();
    await clickPreview();

    await waitFor(() => {
      expect(calls).toContainEqual({
        command: "preview_snapshot",
        args: {
          request: {
            threadId: "thread-1",
            cutoffTurnId: "turn-1",
            previewId: expect.any(String),
          },
        },
      });
    });
    expect(screen.getByLabelText("截止轮次")).toHaveValue("turn-1");
    expect(screen.queryByRole("option", { name: /turn-3/ })).not.toBeInTheDocument();
  });

  it("重新生成预览时取消已经完成但仍可分享的旧预览", async () => {
    const calls = captureInvocations();
    mockAvailable();
    render(<SharePage />);

    await screen.findByRole("button", { name: "生成分享预览" });
    await clickPreview();
    await screen.findByText("快照预览");

    fireEvent.click(screen.getByRole("button", { name: "重新生成预览" }));

    await waitFor(() => {
      expect(calls).toContainEqual({
        command: "cancel_preview",
        args: { previewId: expect.any(String) },
      });
    });
    const oldPreviewId = calls.find((call) => call.command === "preview_snapshot")
      ?.args?.request as { previewId: string };
    expect(calls).toContainEqual({
      command: "cancel_preview",
      args: { previewId: oldPreviewId.previewId },
    });
  });

  it("快照命令返回对象错误时显示 message，而不是 [object Object]", async () => {
    mockAvailable();
    mockTauri(
      "preview_snapshot",
      "*",
      new Error("Codex App Server 请求失败（-32600）：历史接口已变更"),
    );
    render(<SharePage />);

    await screen.findByRole("button", { name: "生成分享预览" });
    await clickPreview();

    expect(await screen.findByRole("alert")).toHaveTextContent("历史接口已变更");
    expect(screen.queryByText("[object Object]")).not.toBeInTheDocument();
    expect(screen.queryByText("修复登录")).not.toBeInTheDocument();
  });

  it("切换会话时取消尚未完成的旧预览，并忽略迟到结果", async () => {
    let resolvePreview: (value: SnapshotV1) => void = () => {};
    const pendingPreview = new Promise<SnapshotV1>((resolve) => {
      resolvePreview = resolve;
    });
    const calls = captureInvocations();
    mockAvailable();
    mockTauri("preview_snapshot", "*", pendingPreview);
    render(<SharePage />);

    await screen.findByRole("button", { name: /修复登录流程/ });
    await clickPreview();
    await waitFor(() => {
      expect(calls.some((call) => call.command === "preview_snapshot")).toBe(true);
    });
    fireEvent.click(screen.getByRole("button", { name: /补充测试/ }));

    await waitFor(() => {
      expect(calls.some((call) => call.command === "cancel_preview")).toBe(true);
    });
    await act(async () => {
      resolvePreview(snapshot());
      await pendingPreview;
    });
    await waitFor(() => {
      expect(calls.filter((call) => call.command === "cancel_preview")).toHaveLength(2);
    });
    const previewIds = calls
      .filter((call) => call.command === "cancel_preview")
      .map((call) => call.args?.previewId);
    expect(previewIds[0]).toBe(previewIds[1]);
    expect(screen.queryByText("快照预览")).not.toBeInTheDocument();
  });
});
