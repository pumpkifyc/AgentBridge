import { describe, expect, it } from "vitest";
import { describeError } from "./shareErrors";

describe("分享页面的错误提示", () => {
  it("显示 Tauri 命令对象中的中文原因，而不是 [object Object]", () => {
    expect(describeError({
      code: "rpcError",
      message: "Codex App Server 请求失败（-32600）：历史接口已变更",
    })).toBe("Codex App Server 请求失败（-32600）：历史接口已变更");
  });

  it("对没有可读原因的错误显示可执行的兜底提示", () => {
    expect(describeError({ code: "unknown" })).toBe("未知错误，请重试");
  });
});
