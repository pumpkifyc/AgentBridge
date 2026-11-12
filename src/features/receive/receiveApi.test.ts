import { beforeEach, describe, expect, it } from "vitest";
import { mockTauri, resetTauriMocks } from "../../test-setup";
import { receiveApi } from "./receiveApi";

beforeEach(() => resetTauriMocks());

describe("接收命令契约", () => {
  it("预览命令把完整配对 JSON 放在 request 中", async () => {
    mockTauri("preview_share", "*", { shareId: "share-1" });

    const result = await receiveApi.preview("配对 JSON");

    expect(result).toEqual({ shareId: "share-1" });
  });

  it("接受命令发送 shareId 与进度通道，不把凭据或会话正文发回前端", async () => {
    mockTauri("accept_share", "*", { shareId: "share-1" });

    const result = await receiveApi.accept("share-1", () => {});

    expect(result).toEqual({ shareId: "share-1" });
  });

  it("重试预览和删除暂存分别使用对应命令", async () => {
    mockTauri("retry_preview", "*", { shareId: "share-1" });
    mockTauri("delete_staged_snapshot", "*", undefined);

    expect(await receiveApi.retryPreview("配对 JSON", "share-1")).toEqual({ shareId: "share-1" });
    expect(await receiveApi.deleteStaged("share-1")).toBeUndefined();
  });
});
