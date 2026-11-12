import { fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ConversationPreview } from "./ConversationPreview";
import type { SnapshotV1 } from "./shareTypes";

function largeSnapshot(): SnapshotV1 {
  return {
    schemaVersion: 1,
    metadata: {
      sourceThreadId: "thread-large",
      title: "大型会话",
      createdAt: 100,
      updatedAt: 300,
      cutoffTurnId: "turn-large",
      projectName: "agentbridge",
      repositoryId: null,
      sourceWorkingDirectory: "/Users/alice/agentbridge",
    },
    messages: Array.from({ length: 203 }, (_, index) => ({
      role: index % 2 === 0 ? "user" as const : "assistant" as const,
      text: `消息 ${index + 1}`,
      turnIndex: index >= 201 ? 101 : Math.floor(index / 2),
      timestamp: 100 + index,
      historicalToolSummary: null,
    })),
    warnings: [],
  };
}

describe("会话快照预览", () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("大型快照默认从最新消息开始，并按需加载更早内容", () => {
    render(<ConversationPreview snapshot={largeSnapshot()} loading={false} error={null} />);

    expect(screen.getAllByRole("listitem")).toHaveLength(20);
    expect(screen.queryByText("消息 1")).not.toBeInTheDocument();
    expect(screen.getByText("消息 203")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "显示更早的 50 条（还有 183 条）" })).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "显示更早的 50 条（还有 183 条）" }));

    expect(screen.getAllByRole("listitem")).toHaveLength(70);
    expect(screen.getByText("消息 134")).toBeInTheDocument();
    expect(screen.getByText("消息 203")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "显示更早的 50 条（还有 133 条）" })).toBeInTheDocument();
  });

  it("快照生成后定位到最新一轮的第一条消息", () => {
    vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
      const top = this.classList.contains("message-list")
        ? 100
        : this.getAttribute("data-latest-turn-start") === "true"
          ? 340
          : 0;
      return {
        x: 0,
        y: top,
        top,
        right: 0,
        bottom: top,
        left: 0,
        width: 0,
        height: 0,
        toJSON: () => ({}),
      };
    });

    render(<ConversationPreview snapshot={largeSnapshot()} loading={false} error={null} />);

    expect(document.querySelector('[data-latest-turn-start="true"]')).toHaveTextContent("消息 202");
    expect(screen.getByRole("list", { name: "快照消息列表" })).toHaveProperty("scrollTop", 240);
  });
});
