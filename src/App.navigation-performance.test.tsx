import { fireEvent, render, screen, within } from "@testing-library/react";
import { useState } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { mockTauri, resetTauriMocks } from "./test-setup";

vi.mock("./features/share/SharePage", () => ({
  SharePage: ({ active }: { active: boolean }) => {
    const [selectionCount, setSelectionCount] = useState(0);
    if (!active) {
      return null;
    }
    return (
      <div>
        <span>分享页测试内容：{selectionCount}</span>
        <button type="button" onClick={() => setSelectionCount((count) => count + 1)}>
          增加选择
        </button>
      </div>
    );
  },
}));

import App from "./App";

describe("AgentBridge 导航性能", () => {
  beforeEach(() => {
    resetTauriMocks();
    mockTauri("list_imports", "*", []);
  });

  it("离开分享页后移除大型页面内容，再次返回时保留页面状态", () => {
    render(<App />);
    const navigation = within(screen.getByRole("navigation"));

    fireEvent.click(navigation.getByRole("button", { name: "分享会话" }));
    expect(screen.getByText("分享页测试内容：0")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "增加选择" }));
    expect(screen.getByText("分享页测试内容：1")).toBeInTheDocument();

    for (let index = 0; index < 3; index += 1) {
      fireEvent.click(navigation.getByRole("button", { name: "首页" }));
      expect(screen.queryByText("分享页测试内容：1")).not.toBeInTheDocument();
      fireEvent.click(navigation.getByRole("button", { name: "接收会话" }));
      fireEvent.click(navigation.getByRole("button", { name: "导入历史" }));
      fireEvent.click(navigation.getByRole("button", { name: "分享会话" }));
      expect(screen.getByText("分享页测试内容：1")).toBeInTheDocument();
    }
  });
});
