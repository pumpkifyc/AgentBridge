import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

describe("Tauri app identity", () => {
  it("uses AgentBridge for the product and window title", () => {
    const config = JSON.parse(readFileSync("src-tauri/tauri.conf.json", "utf8"));

    expect(config.productName).toBe("AgentBridge");
    expect(config.app.windows[0].title).toBe("AgentBridge");
  });
});
