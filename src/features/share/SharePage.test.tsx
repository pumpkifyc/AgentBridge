import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
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
}

beforeEach(() => {
  resetTauriMocks();
  vi.restoreAllMocks();
});

describe("分享会话页面", () => {
  it("先显示加载中，再显示会话列表和第一份快照", async () => {
    mockAvailable();
    render(<SharePage />);

    expect(screen.getByText("正在检查本机 Codex…")).toBeInTheDocument();
    expect(await screen.findByText("修复登录流程")).toBeInTheDocument();
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
    mockTauri("codex_status", {}, new Error("找不到 Codex CLI"));
    render(<SharePage />);

    expect(await screen.findByRole("alert")).toHaveTextContent("找不到 Codex CLI");
    expect(screen.getByText("请检查本机 Codex 安装后重试。", { exact: false })).toBeInTheDocument();
  });

  it("选择另一个会话后刷新截止轮次和预览", async () => {
    mockAvailable();
    mockTauri("preview_snapshot", { threadId: "thread-2", cutoffTurnId: null }, snapshot({
      metadata: { ...snapshot().metadata, sourceThreadId: "thread-2", title: "补充测试" },
      messages: [{ role: "user", text: "补充回归测试", turnIndex: 0, timestamp: 210, historicalToolSummary: null }],
      warnings: [],
    }));
    render(<SharePage />);

    const second = await screen.findByRole("button", { name: /补充测试/ });
    fireEvent.click(second);

    await waitFor(() => {
      expect(screen.getByText("补充回归测试")).toBeInTheDocument();
    });
    expect(screen.getByRole("button", { name: /补充测试/ })).toHaveAttribute("aria-pressed", "true");
  });

  it("选择较早的已完成轮次会使用对应截止点重新预览", async () => {
    const calls: Array<Record<string, unknown>> = [];
    const internals = (
      globalThis as typeof globalThis & {
        __TAURI_INTERNALS__: {
          invoke: (command: string, args?: Record<string, unknown>) => Promise<unknown>;
        };
      }
    ).__TAURI_INTERNALS__;
    const originalInvoke = internals.invoke;
    internals.invoke = async (command, args) => {
      if (command === "preview_snapshot") {
        calls.push(args ?? {});
      }
      return originalInvoke(command, args);
    };
    mockAvailable();
    mockTauri(
      "preview_snapshot",
      { threadId: "thread-1", cutoffTurnId: "turn-1" },
      snapshot({ metadata: { ...snapshot().metadata, cutoffTurnId: "turn-1" } }),
    );
    render(<SharePage />);

    const select = await screen.findByLabelText("截止轮次");
    await waitFor(() => expect(calls.length).toBeGreaterThan(0));
    fireEvent.change(select, { target: { value: "turn-1" } });

    await waitFor(() => {
      expect(calls).toContainEqual({ threadId: "thread-1", cutoffTurnId: "turn-1" });
    });
    expect(screen.getByLabelText("截止轮次")).toHaveValue("turn-1");
    expect(screen.queryByRole("option", { name: /turn-3/ })).not.toBeInTheDocument();
    internals.invoke = originalInvoke;
  });

  it("快照读取失败时显示错误且不渲染会话正文", async () => {
    mockAvailable();
    mockTauri("preview_snapshot", "*", new Error("历史分页接口不受支持"));
    render(<SharePage />);

    expect(await screen.findByRole("alert")).toHaveTextContent("历史分页接口不受支持");
    expect(screen.queryByText("修复登录")).not.toBeInTheDocument();
  });
});
