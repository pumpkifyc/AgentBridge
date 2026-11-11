import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import App from "./App";

describe("AgentBridge shell", () => {
  it("shows the empty state for a new workspace", () => {
    render(<App />);

    expect(screen.getByRole("heading", { name: "AgentBridge" })).toBeInTheDocument();
    expect(screen.getByText("还没有共享或导入的会话")).toBeInTheDocument();
  });
});
