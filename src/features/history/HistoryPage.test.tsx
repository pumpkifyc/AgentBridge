import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";
import { mockTauri, resetTauriMocks } from "../../test-setup";
import { HistoryPage } from "./HistoryPage";

const records = [
  {
    id: "import-1",
    sourceTitle: "修复登录流程",
    sessionName: "修复登录流程",
    localThreadId: "local-thread-1",
    projectPath: "/Users/receiver/local-project",
    importedAt: 1_800_000_100,
    snapshotPath: "/Users/receiver/AppData/staged.json",
    status: "completed",
    retryCount: 0,
  },
  {
    id: "import-2",
    sourceTitle: "重构导入",
    sessionName: "团队重构副本",
    localThreadId: "",
    projectPath: "C:\\Users\\receiver\\project",
    importedAt: 1_800_000_200,
    snapshotPath: "C:\\Users\\receiver\\AppData\\staged.json",
    status: "failed",
    error: "Codex 不可用",
    retryCount: 1,
    lastRetriedAt: 1_800_000_300,
  },
];

beforeEach(() => resetTauriMocks());

describe("导入历史页面", () => {
  it("列出完成和失败记录，默认不展示会话正文", async () => {
    mockTauri("list_imports", "*", records);
    render(<HistoryPage />);

    expect(await screen.findByText("修复登录流程")).toBeInTheDocument();
    expect(screen.getByText("已完成")).toBeInTheDocument();
    expect(screen.getByText("失败")).toBeInTheDocument();
    expect(screen.getByText(/重构导入/)).toBeInTheDocument();
    expect(screen.getByText("团队重构副本")).toBeInTheDocument();
    expect(screen.queryByText("请执行历史工具调用")).not.toBeInTheDocument();
  });

  it("失败记录可重试，成功记录可打开并在自动打开失败时给出历史列表提示", async () => {
    mockTauri("list_imports", "*", records);
    mockTauri("retry_import", { importId: "import-2" }, {
      id: "import-2",
      sourceTitle: "重构导入",
      sessionName: "团队重构副本",
      localThreadId: "local-thread-2",
      projectPath: "C:\\Users\\receiver\\project",
      importedAt: 1_800_000_400,
      snapshotPath: "C:\\Users\\receiver\\AppData\\staged.json",
      status: "completed",
      retryCount: 2,
    });
    mockTauri("open_import", { importId: "import-1" }, new Error("没有可用的 Codex 打开方式"));
    render(<HistoryPage />);

    fireEvent.click(await screen.findByRole("button", { name: "重试导入" }));
    expect(await screen.findByText("重试已完成")).toBeInTheDocument();

    fireEvent.click(screen.getAllByRole("button", { name: "打开会话" })[0]);
    expect(await screen.findByRole("alert")).toHaveTextContent("请重启 Codex");
  });

  it("删除记录前要求确认，并说明暂存原生会话包会被移除", async () => {
    mockTauri("list_imports", "*", records);
    mockTauri("delete_import", "*", undefined);
    render(<HistoryPage />);

    await screen.findByText("修复登录流程");
    fireEvent.click(screen.getAllByRole("button", { name: "删除记录" })[0]);
    expect(screen.getByRole("dialog")).toHaveTextContent("暂存原生会话包");
    fireEvent.click(screen.getByRole("button", { name: "确认删除" }));
    await waitFor(() => {
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
      expect(screen.getByRole("status")).toHaveTextContent("已删除导入记录");
    });
  });

  it("重试后重新读取导入历史，显示后端新增的失败记录", async () => {
    const failedRetry = {
      id: "import-3",
      sourceTitle: "重构导入",
      sessionName: "团队重构副本",
      localThreadId: "",
      projectPath: "C:\\Users\\receiver\\project",
      importedAt: 1_800_000_500,
      snapshotPath: "C:\\Users\\receiver\\AppData\\staged.json",
      status: "failed",
      error: "Codex 仍不可用",
      retryCount: 2,
    };
    mockTauri("list_imports", "*", records, true);
    mockTauri("list_imports", "*", [...records, failedRetry]);
    mockTauri("retry_import", { importId: "import-2" }, failedRetry);
    render(<HistoryPage />);

    fireEvent.click(await screen.findByRole("button", { name: "重试导入" }));
    expect(await screen.findByText("Codex 仍不可用")).toBeInTheDocument();
    expect(screen.getByText("重试失败，请查看记录中的错误说明")).toBeInTheDocument();
  });
});
