import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";
import { mockTauri, resetTauriMocks } from "../../test-setup";
import { ImportReview } from "./ImportReview";
import type { StagedSnapshot } from "../receive/receiveTypes";

const staged: StagedSnapshot = {
  shareId: "share-1",
  endpoint: "192.168.1.20:49152",
  fingerprint: `sha256:${"a".repeat(64)}`,
  metadata: {
    shareId: "share-1",
    sourceTitle: "修复登录流程",
    projectName: "agentbridge",
    repositoryId: "repo-1",
    sourceWorkingDirectory: "/Users/alice/project",
    messageCount: 2,
    cutoffTurnId: "turn-2",
    warnings: ["已省略图片附件"],
    schemaVersion: 1,
  },
  snapshotPath: "/Users/receiver/Application Support/staged.json",
  totalBytes: 2048,
  chunkCount: 1,
};

beforeEach(() => {
  resetTauriMocks();
});

describe("导入确认", () => {
  it("显示接收方选择的目录并拒绝使用分享方路径", async () => {
    render(<ImportReview staged={staged} />);

    fireEvent.change(screen.getByLabelText("本机项目目录"), {
      target: { value: "/Users/alice/project" },
    });
    fireEvent.click(screen.getByRole("button", { name: "确认导入" }));

    expect(await screen.findByRole("alert")).toHaveTextContent("分享方路径只作说明");
    expect(screen.getByText("仅作说明，不能作为本机项目目录")).toBeInTheDocument();
  });

  it("选择本机目录后才发送导入命令，不向前端回传会话正文", async () => {
    mockTauri("select_project_directory", "*", "/Users/receiver/local-project");
    mockTauri("import_snapshot", {
      request: {
        shareId: "share-1",
        projectPath: "/Users/receiver/local-project",
      },
    }, {
      id: "import-1",
      sourceTitle: "修复登录流程",
      localThreadId: "local-thread-1",
      projectPath: "/Users/receiver/local-project",
      importedAt: 1_800_000_100,
      snapshotPath: staged.snapshotPath,
      status: "completed",
    });
    render(<ImportReview staged={staged} />);

    fireEvent.click(screen.getByRole("button", { name: "选择本机目录" }));
    expect(await screen.findByDisplayValue("/Users/receiver/local-project")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "确认导入" }));

    expect(await screen.findByText(/已创建本机续接会话/)).toBeInTheDocument();
    expect(screen.getByText(/local-thread-1/)).toBeInTheDocument();
    expect(screen.queryByText("修复登录")).not.toBeInTheDocument();
  });

  it("创建会话失败时保留快照并展示后备提示", async () => {
    mockTauri("select_project_directory", "*", "/Users/receiver/local-project");
    mockTauri(
      "import_snapshot",
      "*",
      new Error("无法启动 Codex App Server：系统找不到指定文件"),
      true,
    );
    render(<ImportReview staged={staged} />);
    fireEvent.click(screen.getByRole("button", { name: "选择本机目录" }));
    await screen.findByDisplayValue("/Users/receiver/local-project");

    fireEvent.click(screen.getByRole("button", { name: "确认导入" }));

    expect(await screen.findByRole("alert")).toHaveTextContent("无法创建本机续接会话");
    expect(screen.getByText(/快照仍保留在本机/)).toBeInTheDocument();
    expect(screen.getByRole("alert")).toHaveTextContent("系统找不到指定文件");
    expect(screen.queryByText(/已创建本机续接会话/)).not.toBeInTheDocument();
  });
});
