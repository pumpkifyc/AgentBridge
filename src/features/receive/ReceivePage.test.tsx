import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { mockTauri, resetTauriMocks } from "../../test-setup";
import { ReceivePage } from "./ReceivePage";
import type { ReceivedPreview, StagedSnapshot } from "./receiveTypes";

const validPairing = {
  endpoint: "192.168.1.20:49152",
  secret: "share-secret-value",
  fingerprint: `sha256:${"a".repeat(64)}`,
  verificationCode: "065307",
  expiresAt: 1_800_000_300,
};
const pairingText = JSON.stringify(validPairing);

const preview: ReceivedPreview = {
  shareId: "share-1",
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
  totalBytes: 2048,
  chunkCount: 1,
};

const staged: StagedSnapshot = {
  shareId: "share-1",
  endpoint: "192.168.1.20:49152",
  fingerprint: `sha256:${"a".repeat(64)}`,
  metadata: preview.metadata,
  snapshotPath: "/Users/receiver/Library/Application Support/AgentBridge/share-1.agbridge",
  totalBytes: 2048,
  chunkCount: 1,
};

beforeEach(() => {
  resetTauriMocks();
  vi.spyOn(console, "error").mockImplementation(() => {});
  vi.spyOn(console, "warn").mockImplementation(() => {});
});

describe("接收会话页面", () => {
  it("粘贴配对信息后先展示元数据预览，下载前不显示会话正文", async () => {
    mockTauri("preview_share", "*", preview);
    render(<ReceivePage />);

    fireEvent.change(screen.getByLabelText("配对信息"), { target: { value: pairingText } });
    fireEvent.click(screen.getByRole("button", { name: "预览分享" }));

    expect(await screen.findByText("修复登录流程")).toBeInTheDocument();
    expect(screen.getByText("2 条消息")).toBeInTheDocument();
    expect(screen.getByText("turn-2")).toBeInTheDocument();
    expect(screen.getByText("已省略图片附件")).toBeInTheDocument();
    expect(screen.getByText("仅作说明，接收方需选择自己的项目目录")).toBeInTheDocument();
    expect(screen.queryByText("修复登录")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "接受并下载" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "拒绝分享" })).toBeInTheDocument();
  });

  it("配对信息格式错误时显示可执行提示，不调用传输命令", async () => {
    render(<ReceivePage />);

    fireEvent.change(screen.getByLabelText("配对信息"), { target: { value: "不是 JSON" } });
    fireEvent.click(screen.getByRole("button", { name: "预览分享" }));

    expect(await screen.findByRole("alert")).toHaveTextContent("配对信息无效");
    expect(screen.queryByText("修复登录流程")).not.toBeInTheDocument();
  });

  it("连接超时时提示检查物理网卡和 Windows 防火墙", async () => {
    mockTauri("preview_share", "*", new Error("连接分享方超时"), true);
    render(<ReceivePage />);

    fireEvent.change(screen.getByLabelText("配对信息"), { target: { value: pairingText } });
    fireEvent.click(screen.getByRole("button", { name: "预览分享" }));

    expect(await screen.findByRole("alert")).toHaveTextContent("Wi-Fi 或 Ethernet");
    expect(screen.getByRole("alert")).toHaveTextContent("Windows 防火墙");
  });

  it("接收方拒绝分享后回到输入状态", async () => {
    mockTauri("preview_share", "*", preview);
    mockTauri("decline_share", { shareId: "share-1" }, undefined);
    render(<ReceivePage />);
    fireEvent.change(screen.getByLabelText("配对信息"), { target: { value: pairingText } });
    fireEvent.click(screen.getByRole("button", { name: "预览分享" }));
    await screen.findByText("修复登录流程");

    fireEvent.click(screen.getByRole("button", { name: "拒绝分享" }));

    expect(await screen.findByText("已拒绝该分享")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "接受并下载" })).not.toBeInTheDocument();
  });

  it("接受后显示下载进度和待导入状态，不立即创建 Codex 会话", async () => {
    mockTauri("preview_share", "*", preview);
    mockTauri("accept_share", { shareId: "share-1" }, staged);
    render(<ReceivePage />);
    fireEvent.change(screen.getByLabelText("配对信息"), { target: { value: pairingText } });
    fireEvent.click(screen.getByRole("button", { name: "预览分享" }));
    await screen.findByText("修复登录流程");

    fireEvent.click(screen.getByRole("button", { name: "接受并下载" }));

    expect(await screen.findByText("原生会话包已暂存，等待选择本地项目后导入")).toBeInTheDocument();
    expect(screen.getByRole("progressbar")).toHaveAttribute("aria-valuenow", "100");
    expect(screen.getByText("2.0 KB / 2.0 KB")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "创建 Codex 会话" })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "删除暂存会话包" })).toBeInTheDocument();
  });

  it("下载中断后保留配对信息，允许明确重试", async () => {
    mockTauri("preview_share", "*", preview);
    mockTauri("retry_preview", "*", preview);
    mockTauri("accept_share", { shareId: "share-1" }, new Error("secret=share-secret-value 与会话内容"), true);
    mockTauri("accept_share", { shareId: "share-1" }, staged);
    render(<ReceivePage />);
    fireEvent.change(screen.getByLabelText("配对信息"), { target: { value: pairingText } });
    fireEvent.click(screen.getByRole("button", { name: "预览分享" }));
    await screen.findByText("修复登录流程");

    fireEvent.click(screen.getByRole("button", { name: "接受并下载" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("传输中断，可检查局域网连接后重试");
    expect(screen.queryByText("share-secret-value")).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "重试下载" }));
    expect(await screen.findByText("原生会话包已暂存，等待选择本地项目后导入")).toBeInTheDocument();
  });

  it("下载中可取消，取消后不把中断快照标记为成功", async () => {
    mockTauri("preview_share", "*", preview);
    let resolveAccept: (value: StagedSnapshot) => void = () => {};
    const pending = new Promise<StagedSnapshot>((resolve) => {
      resolveAccept = resolve;
    });
    mockTauri("accept_share", { shareId: "share-1" }, pending);
    mockTauri("cancel_transfer", { shareId: "share-1" }, undefined);
    render(<ReceivePage />);
    fireEvent.change(screen.getByLabelText("配对信息"), { target: { value: pairingText } });
    fireEvent.click(screen.getByRole("button", { name: "预览分享" }));
    await screen.findByText("修复登录流程");

    fireEvent.click(screen.getByRole("button", { name: "接受并下载" }));
    fireEvent.click(await screen.findByRole("button", { name: "取消下载" }));
    expect(await screen.findByText("下载已取消")).toBeInTheDocument();
    resolveAccept(staged);
    await waitFor(() => {
      expect(screen.queryByText("原生会话包已暂存，等待选择本地项目后导入")).not.toBeInTheDocument();
    });
  });

  it("删除暂存原生会话包会移除本机文件与记录，且错误不含凭据", async () => {
    mockTauri("preview_share", "*", preview);
    mockTauri("accept_share", { shareId: "share-1" }, staged);
    mockTauri("delete_staged_snapshot", { shareId: "share-1" }, undefined);
    render(<ReceivePage />);
    fireEvent.change(screen.getByLabelText("配对信息"), { target: { value: pairingText } });
    fireEvent.click(screen.getByRole("button", { name: "预览分享" }));
    await screen.findByText("修复登录流程");
    fireEvent.click(screen.getByRole("button", { name: "接受并下载" }));
    await screen.findByText("原生会话包已暂存，等待选择本地项目后导入");

    fireEvent.click(screen.getByRole("button", { name: "删除暂存会话包" }));

    expect(await screen.findByText("已删除本机暂存原生会话包")).toBeInTheDocument();
    expect(screen.queryByText("原生会话包已暂存，等待选择本地项目后导入")).not.toBeInTheDocument();
    expect(console.error).not.toHaveBeenCalled();
    expect(console.warn).not.toHaveBeenCalled();
  });
});
