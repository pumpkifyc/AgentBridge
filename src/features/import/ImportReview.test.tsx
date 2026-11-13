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
  snapshotPath: "/Users/receiver/Application Support/staged.agbridge",
  totalBytes: 2048,
  chunkCount: 1,
};

const localProject = {
  path: "/Users/receiver/agentbridge",
  name: "agentbridge",
  threadCount: 3,
  updatedAt: 1_800_000_000,
};

function importedRecord(sessionName = "修复登录流程") {
  return {
    id: "import-1",
    sourceTitle: "修复登录流程",
    sessionName,
    localThreadId: "local-thread-1",
    projectPath: localProject.path,
    importedAt: 1_800_000_100,
    snapshotPath: staged.snapshotPath,
    status: "completed",
    retryCount: 0,
  };
}

beforeEach(() => {
  resetTauriMocks();
  mockTauri("list_codex_projects", "*", [localProject]);
});

describe("导入确认", () => {
  it("只把旧提示解释为预览范围，不暗示原生包被删减", async () => {
    render(<ImportReview staged={staged} />);

    expect(screen.getByText("预览显示说明")).toBeInTheDocument();
    expect(screen.getByText("预览中 图片附件 已折叠；原生分享包仍完整保留")).toBeInTheDocument();
    expect(screen.queryByText("已省略图片附件")).not.toBeInTheDocument();
    expect(await screen.findByText(localProject.path)).toBeInTheDocument();
  });

  it("拒绝使用分享方路径，并在检查冲突前提示", async () => {
    render(<ImportReview staged={staged} />);

    fireEvent.change(screen.getByLabelText("本机项目目录"), {
      target: { value: "/Users/alice/project" },
    });
    fireEvent.click(screen.getByRole("button", { name: "确认导入" }));

    expect(await screen.findByRole("alert")).toHaveTextContent("分享方路径只作说明");
    expect(screen.getByText("仅作说明，不能作为本机项目目录")).toBeInTheDocument();
  });

  it("选择 Codex 已有项目并发送自定义会话名", async () => {
    mockTauri("inspect_import_conflicts", "*", { files: [] });
    mockTauri("import_snapshot", {
      request: {
        shareId: "share-1",
        projectPath: localProject.path,
        sessionName: "团队修复副本",
        conflictPolicy: "preserveLocal",
      },
    }, importedRecord("团队修复副本"));
    render(<ImportReview staged={staged} />);

    const projectPath = await screen.findByText(localProject.path);
    fireEvent.click(projectPath.closest("button")!);
    fireEvent.change(screen.getByLabelText("导入会话名称"), {
      target: { value: "团队修复副本" },
    });
    fireEvent.click(screen.getByRole("button", { name: "确认导入" }));

    expect(await screen.findByText(/已创建本机原生会话：团队修复副本/)).toBeInTheDocument();
    expect(screen.queryByText("请执行历史工具调用")).not.toBeInTheDocument();
  });

  it("搜索项目并按名称筛选 Codex 已有项目", async () => {
    mockTauri("list_codex_projects", "*", [
      localProject,
      {
        path: "/Users/receiver/another-project",
        name: "另一个项目",
        threadCount: 1,
        updatedAt: 1_800_000_100,
      },
    ]);
    render(<ImportReview staged={staged} />);

    fireEvent.change(screen.getByLabelText("搜索项目"), { target: { value: "another" } });

    expect(await screen.findByText("另一个项目")).toBeInTheDocument();
    expect(screen.queryByText("agentbridge")).not.toBeInTheDocument();
  });

  it("可以创建新项目并使用新目录导入", async () => {
    mockTauri("select_project_parent", "*", "/Users/receiver/projects");
    mockTauri("create_project_directory", {
      request: {
        parentPath: "/Users/receiver/projects",
        projectName: "shared-code",
      },
    }, "/Users/receiver/projects/shared-code");
    render(<ImportReview staged={staged} />);

    fireEvent.click(screen.getByRole("tab", { name: "新建项目" }));
    fireEvent.click(screen.getByRole("button", { name: "选择父目录" }));
    expect(await screen.findByDisplayValue("/Users/receiver/projects")).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText("项目名称"), {
      target: { value: "shared-code" },
    });
    fireEvent.click(screen.getByRole("button", { name: "创建项目" }));
    expect(await screen.findByDisplayValue("/Users/receiver/projects/shared-code")).toBeInTheDocument();
  });

  it("检测到本机冲突时要求选择保留本机会话并新建", async () => {
    mockTauri("inspect_import_conflicts", "*", {
      files: ["sessions/2026/11/13/rollout-conflict.jsonl"],
    });
    mockTauri("import_snapshot", {
      request: {
        shareId: "share-1",
        projectPath: localProject.path,
        sessionName: "修复登录流程",
        conflictPolicy: "preserveLocal",
      },
    }, importedRecord());
    render(<ImportReview staged={staged} />);

    fireEvent.click((await screen.findByText(localProject.path)).closest("button")!);
    fireEvent.click(screen.getByRole("button", { name: "确认导入" }));

    expect(await screen.findByRole("dialog", { name: "本机已有冲突会话" })).toHaveTextContent(
      "本机已有相同的 Codex 会话文件",
    );
    fireEvent.click(screen.getByRole("button", { name: "保留本机会话并新建" }));
    expect(await screen.findByText(/已创建本机原生会话/)).toBeInTheDocument();
  });

  it("明确选择覆盖冲突会话时发送覆盖策略", async () => {
    mockTauri("inspect_import_conflicts", "*", {
      files: ["sessions/2026/11/13/rollout-conflict.jsonl"],
    });
    mockTauri("import_snapshot", {
      request: {
        shareId: "share-1",
        projectPath: localProject.path,
        sessionName: "修复登录流程",
        conflictPolicy: "replaceLocal",
      },
    }, importedRecord());
    render(<ImportReview staged={staged} />);

    fireEvent.click((await screen.findByText(localProject.path)).closest("button")!);
    fireEvent.click(screen.getByRole("button", { name: "确认导入" }));
    fireEvent.click(await screen.findByRole("button", { name: "覆盖冲突会话" }));

    expect(await screen.findByText(/已创建本机原生会话/)).toBeInTheDocument();
  });

  it("创建会话失败时保留原生会话包并展示错误原因", async () => {
    mockTauri("inspect_import_conflicts", "*", { files: [] });
    mockTauri(
      "import_snapshot",
      "*",
      new Error("无法启动 Codex App Server：系统找不到指定文件"),
      true,
    );
    render(<ImportReview staged={staged} />);
    fireEvent.click((await screen.findByText(localProject.path)).closest("button")!);

    fireEvent.click(screen.getByRole("button", { name: "确认导入" }));

    expect(await screen.findByRole("alert")).toHaveTextContent("无法创建本机原生会话");
    expect(screen.getByText(/会话包仍保留在本机/)).toBeInTheDocument();
    expect(screen.getByRole("alert")).toHaveTextContent("系统找不到指定文件");
    expect(screen.queryByText(/已创建本机原生会话/)).not.toBeInTheDocument();
  });
});
