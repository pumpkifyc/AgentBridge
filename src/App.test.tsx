import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import App from "./App";

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
});
