import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { mockTauri, resetTauriMocks } from "./test-setup";
import App from "./App";

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

beforeEach(() => {
  resetTauriMocks();
});

afterEach(() => {
  restoreInvoke?.();
  restoreInvoke = null;
});

describe("AgentBridge shell", () => {
  it("shows the empty state for a new workspace", () => {
    render(<App />);

    expect(screen.getByRole("heading", { name: "AgentBridge" })).toBeInTheDocument();
    expect(screen.getByText("还没有共享或导入的会话")).toBeInTheDocument();
  });
});

describe("AgentBridge 主导航", () => {
  it("从首页可以打开分享会话页面", () => {
    render(<App />);
    fireEvent.click(screen.getAllByRole("button", { name: "分享会话" })[0]);
    expect(screen.getByText("正在检查本机 Codex…")).toBeInTheDocument();
  });

  it("切换回分享会话时复用已经加载的页面", async () => {
    const calls = captureInvocations();
    mockTauri("codex_status", {}, { state: "available", detail: "本机 Codex 可用" });
    mockTauri("list_threads", { cursor: null }, {
      data: [{
        id: "thread-1",
        title: "修复登录流程",
        preview: "修复登录流程",
        cwd: "/Users/alice/project",
        createdAt: 100,
        updatedAt: 200,
        repositoryId: "repo-1",
      }],
      nextCursor: null,
    });
    mockTauri("list_turns", "*", []);
    mockTauri("list_local_addresses", {}, [{ ip: "192.168.1.20", name: "en0" }]);
    render(<App />);

    fireEvent.click(screen.getAllByRole("button", { name: "分享会话" })[0]);
    expect(await screen.findByRole("button", { name: /修复登录流程/ })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "首页" }));
    fireEvent.click(screen.getAllByRole("button", { name: "分享会话" })[0]);

    expect(screen.getByRole("button", { name: /修复登录流程/ })).toBeInTheDocument();
    await waitFor(() => {
      expect(calls.filter((call) => call.command === "codex_status")).toHaveLength(1);
      expect(calls.filter((call) => call.command === "list_threads")).toHaveLength(1);
    });
  });
});
