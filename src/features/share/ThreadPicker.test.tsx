import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { ThreadPicker } from "./ThreadPicker";
import type { ThreadSummary } from "./shareTypes";

const threads: ThreadSummary[] = [
  {
    id: "thread-1",
    title: "设计会话互通方案",
    preview: "讨论分享流程",
    cwd: "/Users/alice/AgentBridge",
    createdAt: 100,
    updatedAt: 200,
    repositoryId: "agentbridge",
  },
  {
    id: "thread-2",
    title: null,
    preview: "修复 Windows 安装包",
    cwd: "C:\\projects\\Bridge",
    createdAt: 101,
    updatedAt: 201,
    repositoryId: "bridge-win",
  },
];

function renderPicker() {
  render(
    <ThreadPicker
      threads={threads}
      turns={[]}
      selectedThreadId="thread-1"
      selectedTurnId={null}
      onSelectThread={vi.fn()}
      onSelectTurn={vi.fn()}
    />,
  );
}

describe("会话选择器搜索", () => {
  it("按标题、预览和工作目录即时过滤，并显示匹配数量", async () => {
    renderPicker();

    const search = screen.getByRole("searchbox", { name: "搜索会话" });
    fireEvent.change(search, { target: { value: "windows" } });

    await waitFor(() => {
      expect(screen.getByRole("button", { name: /修复 Windows 安装包/ })).toBeInTheDocument();
      expect(screen.queryByRole("button", { name: /设计会话互通方案/ })).not.toBeInTheDocument();
      expect(screen.getByText("显示 1 / 2 个会话")).toBeInTheDocument();
    });

    fireEvent.change(search, { target: { value: "agentbridge" } });
    await waitFor(() => {
      expect(screen.getByRole("button", { name: /设计会话互通方案/ })).toBeInTheDocument();
      expect(screen.queryByRole("button", { name: /修复 Windows 安装包/ })).not.toBeInTheDocument();
    });
  });

  it("没有匹配结果时显示明确提示", async () => {
    renderPicker();

    fireEvent.change(screen.getByRole("searchbox", { name: "搜索会话" }), {
      target: { value: "不存在的会话" },
    });

    await waitFor(() => {
      expect(screen.getByText("没有匹配的会话")).toBeInTheDocument();
      expect(screen.getByText("显示 0 / 2 个会话")).toBeInTheDocument();
    });
  });
});
