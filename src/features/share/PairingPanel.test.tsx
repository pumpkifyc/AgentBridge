import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { mockTauri, resetTauriMocks } from "../../test-setup";
import { PairingPanel } from "./PairingPanel";
import type { ShareSession, ShareState } from "./pairingApi";

vi.mock("qrcode", () => ({
  toString: vi.fn(async () => "test"),
}));

const nowSeconds = 1_800_000_000;

function session(overrides: Partial<ShareSession> = {}): ShareSession {
  return {
    id: "invitation-1",
    endpoint: "192.168.1.20:49152",
    secret: "share-secret-value",
    fingerprint: `sha256:${"a".repeat(64)}`,
    verificationCode: "012345",
    expiresAt: nowSeconds + 300,
    state: "等待配对",
    ...overrides,
  };
}

function mockStart(value: unknown = session()) {
  mockTauri("start_share", "*", value);
}

function mockPolling(...states: ShareState[]) {
  for (const state of states) {
    mockTauri("share_status", { invitationId: "invitation-1" }, state, true);
  }
  mockTauri("share_status", { invitationId: "invitation-1" }, "等待配对");
}

beforeEach(() => {
  resetTauriMocks();
  vi.useFakeTimers({ shouldAdvanceTime: true });
  vi.setSystemTime(nowSeconds * 1000);
  vi.spyOn(console, "error").mockImplementation(() => {});
  vi.spyOn(console, "warn").mockImplementation(() => {});
  vi.spyOn(console, "log").mockImplementation(() => {});
});

afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
});

describe("分享方配对面板", () => {
  it("未选择会话时提示先预览，且不创建邀请", () => {
    render(<PairingPanel threadId={null} previewReady={false} />);

    fireEvent.click(screen.getByRole("button", { name: "创建一次性邀请" }));

    expect(screen.getByRole("alert")).toHaveTextContent("请先选择并预览一个会话");
    expect(screen.getByText("尚未开始分享")).toBeInTheDocument();
  });

  it("创建邀请后显示地址、验证码、指纹和含凭据的二维码", async () => {
    mockStart();
    mockPolling("等待配对");
    render(<PairingPanel threadId="thread-1" previewReady />);

    fireEvent.change(screen.getByLabelText("本机局域网地址"), {
      target: { value: "192.168.1.20" },
    });
    fireEvent.click(screen.getByRole("button", { name: "创建一次性邀请" }));

    await waitFor(() => expect(screen.getByText("192.168.1.20:49152")).toBeInTheDocument());
    expect(screen.getByText("012345")).toBeInTheDocument();
    expect(screen.getByText(`sha256:${"a".repeat(64)}`)).toBeInTheDocument();
    const pairingText = screen.getByLabelText("配对信息");
    expect(pairingText).toHaveValue(
      JSON.stringify({
        endpoint: "192.168.1.20:49152",
        secret: "share-secret-value",
        fingerprint: `sha256:${"a".repeat(64)}`,
        verificationCode: "012345",
        expiresAt: nowSeconds + 300,
      }),
    );
    await waitFor(() => expect(screen.getByAltText("配对二维码")).toHaveAttribute("src", "data:image/svg+xml;base64,dGVzdA=="));
    expect(screen.getByRole("button", { name: "复制配对信息" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "立即撤销" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "取消分享" })).toBeInTheDocument();
    expect(screen.getByText("等待配对")).toBeInTheDocument();
  });

  it("每个 1.5 秒轮询状态并显示所有中文阶段", async () => {
    mockStart();
    mockPolling(
      "已配对",
      "预览中",
      "传输中",
      "传输完成",
      "已导入",
      "已拒绝",
      "已撤销",
      "已取消",
      "失败",
    );
    render(<PairingPanel threadId="thread-1" previewReady />);
    fireEvent.click(screen.getByRole("button", { name: "创建一次性邀请" }));
    await waitFor(() => expect(screen.getByText("192.168.1.20:49152")).toBeInTheDocument());

    for (const state of [
      "已配对",
      "预览中",
      "传输中",
      "传输完成",
      "已导入",
      "已拒绝",
      "已撤销",
      "已取消",
      "失败",
    ]) {
      await act(async () => {
        vi.advanceTimersByTime(1500);
      });
      await waitFor(() => expect(screen.getByText(state)).toBeInTheDocument());
    }
  });

  it("到过期时间后显示已过期，并允许重新创建邀请", async () => {
    mockStart();
    mockPolling("等待配对");
    render(<PairingPanel threadId="thread-1" previewReady />);
    fireEvent.click(screen.getByRole("button", { name: "创建一次性邀请" }));
    await waitFor(() => expect(screen.getByText("192.168.1.20:49152")).toBeInTheDocument());

    await act(async () => {
      vi.setSystemTime((nowSeconds + 301) * 1000);
      vi.advanceTimersByTime(1500);
    });

    await waitFor(() => expect(screen.getByText("已过期")).toBeInTheDocument());
    expect(screen.getByRole("button", { name: "重新创建邀请" })).toBeInTheDocument();
    expect(screen.queryByText("share-secret-value")).not.toBeInTheDocument();
    expect(screen.queryByLabelText("配对信息")).not.toBeInTheDocument();
  });

  it("立即撤销会发送对应命令并清理显示的凭据", async () => {
    mockStart();
    mockPolling("等待配对");
    mockTauri("revoke_share", { invitationId: "invitation-1" }, undefined);
    render(<PairingPanel threadId="thread-1" previewReady />);
    fireEvent.click(screen.getByRole("button", { name: "创建一次性邀请" }));
    await waitFor(() => expect(screen.getByText("192.168.1.20:49152")).toBeInTheDocument());

    fireEvent.click(screen.getByRole("button", { name: "立即撤销" }));

    await waitFor(() => expect(screen.getByText("已撤销")).toBeInTheDocument());
    expect(screen.queryByLabelText("配对信息")).not.toBeInTheDocument();
    expect(screen.queryByText("share-secret-value")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "重新创建邀请" })).toBeInTheDocument();
  });

  it("取消分享会发送对应命令并显示已取消", async () => {
    mockStart();
    mockPolling("等待配对");
    mockTauri("cancel_share", { invitationId: "invitation-1" }, undefined);
    render(<PairingPanel threadId="thread-1" previewReady />);
    fireEvent.click(screen.getByRole("button", { name: "创建一次性邀请" }));
    await waitFor(() => expect(screen.getByText("192.168.1.20:49152")).toBeInTheDocument());

    fireEvent.click(screen.getByRole("button", { name: "取消分享" }));

    await waitFor(() => expect(screen.getByText("已取消")).toBeInTheDocument());
    expect(screen.queryByLabelText("配对信息")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "重新创建邀请" })).toBeInTheDocument();
  });

  it("创建失败和轮询失败显示固定中文错误，并允许重新创建", async () => {
    mockStart(new Error("secret=share-secret-value 与会话正文不该出现在错误里"));
    render(<PairingPanel threadId="thread-1" previewReady />);
    fireEvent.click(screen.getByRole("button", { name: "创建一次性邀请" }));

    await waitFor(() => expect(screen.getByRole("alert")).toHaveTextContent("无法创建分享，请确认局域网地址有效后重试"));
    const restart = screen.getByRole("button", { name: "重新创建邀请" });
    mockStart(session({ id: "invitation-2" }));
    mockTauri("share_status", { invitationId: "invitation-2" }, "等待配对");
    fireEvent.click(restart);
    await waitFor(() => expect(screen.getByText("192.168.1.20:49152")).toBeInTheDocument());

    mockTauri("share_status", { invitationId: "invitation-2" }, new Error("secret=share-secret-value"), true);
    await act(async () => {
      vi.advanceTimersByTime(1500);
    });
    await waitFor(() => expect(screen.getByRole("alert")).toHaveTextContent("无法读取分享状态，可重新创建邀请"));
    expect(screen.queryByText("share-secret-value")).not.toBeInTheDocument();
    expect(console.error).not.toHaveBeenCalled();
    expect(console.warn).not.toHaveBeenCalled();
    expect(console.log).not.toHaveBeenCalled();
  });

  it("切换会话或预览失效时停止旧邀请轮询", async () => {
    mockStart();
    mockPolling("等待配对");
    const { rerender } = render(<PairingPanel threadId="thread-1" previewReady />);
    fireEvent.click(screen.getByRole("button", { name: "创建一次性邀请" }));
    await waitFor(() => expect(screen.getByText("192.168.1.20:49152")).toBeInTheDocument());

    rerender(<PairingPanel threadId="thread-2" previewReady />);
    expect(screen.queryByLabelText("配对信息")).not.toBeInTheDocument();
    expect(screen.getByText("尚未开始分享")).toBeInTheDocument();

    rerender(<PairingPanel threadId="thread-2" previewReady={false} />);
    await act(async () => {
      vi.advanceTimersByTime(4500);
    });
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });
});
