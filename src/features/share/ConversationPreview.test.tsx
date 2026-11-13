import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { ConversationPreview } from "./ConversationPreview";
import type {
  PreviewTimelineV1,
  SnapshotV1,
} from "./shareTypes";

function metadataSnapshot(): SnapshotV1 {
  return {
    schemaVersion: 1,
    metadata: {
      sourceThreadId: "thread-preview",
      title: "完整预览",
      createdAt: 100,
      updatedAt: 300,
      cutoffTurnId: "turn-102",
      projectName: "agentbridge",
      repositoryId: null,
      sourceWorkingDirectory: "/Users/alice/agentbridge",
    },
    messages: [],
    warnings: ["已省略不支持的旧警告"],
  };
}

function largeTimeline(): PreviewTimelineV1 {
  return {
    schemaVersion: 1,
    sourceThreadId: "thread-preview",
    cutoffTurnId: "turn-102",
    notices: [],
    turns: Array.from({ length: 102 }, (_, index) => {
      const turnNumber = index + 1;
      return {
        id: `turn-${turnNumber}`,
        index,
        status: "completed",
        startedAt: 100 + index,
        completedAt: 200 + index,
        items: [
          {
            id: `user-${turnNumber}`,
            kind: "user_message" as const,
            title: "用户消息",
            status: null,
            timestamp: 100 + index,
            sensitive: false,
            blocks: [
              {
                kind: "text" as const,
                label: "消息",
                content: `第 ${turnNumber} 轮用户消息`,
                sensitive: false,
                defaultExpanded: true,
              },
            ],
          },
        ],
      };
    }),
  };
}

function detailedTimeline(sourceThreadId = "thread-preview"): PreviewTimelineV1 {
  return {
    schemaVersion: 1,
    sourceThreadId,
    cutoffTurnId: "turn-1",
    notices: [
      {
        code: "sensitive_blocks",
        count: 1,
        message: "预览检测到疑似敏感块，默认隐藏；原生分享包仍完整保留原文。",
        scope: "display_only",
      },
    ],
    turns: [
      {
        id: "turn-1",
        index: 0,
        status: "completed",
        startedAt: 100,
        completedAt: 300,
        items: [
          {
            id: "user-visible",
            kind: "user_message",
            title: "用户消息",
            status: null,
            timestamp: 100,
            sensitive: false,
            blocks: [
              {
                kind: "text",
                label: "消息",
                content: "普通消息",
                sensitive: false,
                defaultExpanded: true,
              },
            ],
          },
          {
            id: "command-1",
            kind: "command",
            title: "命令执行 · failed",
            status: "failed",
            timestamp: 110,
            sensitive: false,
            blocks: [
              {
                kind: "code",
                label: "命令",
                content: "npm test",
                sensitive: false,
                defaultExpanded: true,
              },
              {
                kind: "code",
                label: "输出",
                content: "测试失败",
                sensitive: false,
                defaultExpanded: false,
              },
            ],
          },
          {
            id: "assistant-1",
            kind: "assistant_message",
            title: "助手回复",
            status: null,
            timestamp: 120,
            sensitive: false,
            blocks: [
              {
                kind: "text",
                label: "回复",
                content: "我会检查失败原因。",
                sensitive: false,
                defaultExpanded: true,
              },
            ],
          },
          {
            id: "sensitive-1",
            kind: "tool_call",
            title: "本地工具",
            status: "completed",
            timestamp: 130,
            sensitive: true,
            blocks: [
              {
                kind: "text",
                label: "敏感原文",
                content: "password=hunter2",
                sensitive: true,
                defaultExpanded: false,
              },
            ],
          },
          {
            id: "diff-1",
            kind: "file_change",
            title: "文件修改",
            status: "completed",
            timestamp: 140,
            sensitive: false,
            blocks: [
              {
                kind: "diff",
                label: "src/second.ts",
                content: "diff --git a/second.ts b/second.ts\n+new line",
                sensitive: false,
                defaultExpanded: false,
              },
            ],
          },
          {
            id: "unknown-1",
            kind: "unknown",
            title: "Codex 条目：futureItem",
            status: null,
            timestamp: 150,
            sensitive: false,
            blocks: [
              {
                kind: "json",
                label: "原始 JSON",
                content: '{"payloadValue": 42}',
                sensitive: false,
                defaultExpanded: false,
              },
            ],
          },
        ],
      },
    ],
  };
}

describe("会话预览时间线", () => {
  it("默认只显示最新轮次并按三轮回看更早内容", () => {
    render(
      <ConversationPreview
        timeline={largeTimeline()}
        snapshot={metadataSnapshot()}
        loading={false}
        error={null}
      />,
    );

    expect(screen.getByText("第 102 轮用户消息")).toBeInTheDocument();
    expect(screen.queryByText("第 1 轮用户消息")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "显示更早的 3 轮（还有 101 轮）" })).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "显示更早的 3 轮（还有 101 轮）" }));

    expect(screen.getByText("第 99 轮用户消息")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "显示更早的 3 轮（还有 98 轮）" })).toBeInTheDocument();
  });

  it("按轮次中的原始顺序渲染事件并显示仅影响预览的说明", () => {
    render(
      <ConversationPreview
        timeline={detailedTimeline()}
        snapshot={metadataSnapshot()}
        loading={false}
        error={null}
      />,
    );

    const items = screen.getAllByTestId("preview-item");
    expect(items.map((item) => item.getAttribute("data-item-kind"))).toEqual([
      "user_message",
      "command",
      "assistant_message",
      "tool_call",
      "file_change",
      "unknown",
    ]);
    expect(screen.getByText("当前为完整分享预览", { exact: false })).toBeInTheDocument();
    expect(screen.getByText("原生包始终保留原始会话", { exact: false })).toBeInTheDocument();
    expect(screen.queryByText("已省略不支持的旧警告")).not.toBeInTheDocument();
  });

  it("命令输出、diff、JSON 和敏感块默认折叠，敏感原文逐块解锁", () => {
    render(
      <ConversationPreview
        timeline={detailedTimeline()}
        snapshot={metadataSnapshot()}
        loading={false}
        error={null}
      />,
    );

    expect(screen.queryByText("测试失败")).not.toBeInTheDocument();
    expect(screen.queryByText("diff --git a/second.ts b/second.ts", { exact: false })).not.toBeInTheDocument();
    expect(screen.queryByText('{"payloadValue": 42}')).not.toBeInTheDocument();
    expect(screen.queryByText("password=hunter2")).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "显示原文" }));

    expect(screen.getByText("password=hunter2")).toBeInTheDocument();
    expect(screen.queryByText("测试失败")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "收起 敏感原文" })).toHaveAttribute("aria-expanded", "true");
  });

  it("切换会话后敏感块恢复默认隐藏", () => {
    const { rerender } = render(
      <ConversationPreview
        timeline={detailedTimeline("thread-1")}
        snapshot={metadataSnapshot()}
        loading={false}
        error={null}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: "显示原文" }));
    expect(screen.getByText("password=hunter2")).toBeInTheDocument();

    rerender(
      <ConversationPreview
        timeline={detailedTimeline("thread-2")}
        snapshot={metadataSnapshot()}
        loading={false}
        error={null}
      />,
    );

    expect(screen.queryByText("password=hunter2")).not.toBeInTheDocument();
  });
});
