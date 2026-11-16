import { describe, expect, it } from "vitest";
import { formatUpdateError } from "./updateErrors";
import { UpdaterError, type UpdateErrorCode } from "./updateTypes";

describe("更新错误文案", () => {
  it.each([
    ["network", "无法连接更新服务器，请检查网络后重试。"],
    ["manifest", "更新清单无效或暂无适用于当前平台的安装包。"],
    ["download", "更新下载失败，请检查网络和磁盘空间后重试。"],
    ["signature", "更新包签名校验失败，当前版本未更改。"],
    ["install", "安装更新失败，当前版本未更改。"],
    [
      "restart",
      "更新已安装，但应用未能自动重启，请手动重新打开 AgentBridge。",
    ],
    ["unknown", "更新失败，请稍后重试。"],
  ] satisfies Array<[UpdateErrorCode, string]>)(
    "将 %s 映射为稳定的中文提示",
    (code, message) => {
      expect(formatUpdateError(new UpdaterError(code, "internal detail"))).toBe(
        message,
      );
    },
  );

  it("不向界面泄露未知底层错误", () => {
    expect(formatUpdateError(new Error("ECONNRESET"))).toBe(
      "更新失败，请稍后重试。",
    );
  });
});
