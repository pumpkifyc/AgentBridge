import { describe, expect, it } from "vitest";
import { buildPairingInfo, parsePairingInfo } from "./pairingApi";
import { SHARE_STATES } from "./ShareStatus";

describe("配对信息契约", () => {
  it("把创建邀请所需的全部字段写入可复制的配对信息", () => {
    const text = buildPairingInfo({
      id: "invitation-1",
      endpoint: "192.168.1.20:49152",
      secret: "share-secret-value",
      fingerprint: "sha256:abc123",
      verificationCode: "012345",
      expiresAt: 1_800_000_000,
      state: "等待配对",
    });

    expect(JSON.parse(text)).toEqual({
      endpoint: "192.168.1.20:49152",
      secret: "share-secret-value",
      fingerprint: "sha256:abc123",
      verificationCode: "012345",
      expiresAt: 1_800_000_000,
    });
  });

  it("拒绝缺少证书指纹或包含正文内容的配对信息", () => {
    expect(() => parsePairingInfo(JSON.stringify({
      endpoint: "192.168.1.20:49152",
      secret: "share-secret-value",
      verificationCode: "012345",
      expiresAt: 1_800_000_000,
    }))).toThrow("配对信息无效");
    expect(() => parsePairingInfo("不是 JSON")).toThrow("配对信息无效");
    expect(() => parsePairingInfo(JSON.stringify({
      endpoint: "192.168.1.20:49152",
      secret: "share-secret-value",
      fingerprint: "sha256:abc123",
      verificationCode: "012345",
      expiresAt: 1_800_000_000,
      messages: ["会话正文"],
    }))).toThrow("配对信息无效");
  });

  it("列出分享方需要显示的每一种中文状态", () => {
    expect(SHARE_STATES).toEqual([
      "等待配对",
      "已配对",
      "预览中",
      "传输中",
      "传输完成",
      "已导入",
      "已拒绝",
      "已过期",
      "已撤销",
      "已取消",
      "失败",
    ]);
  });
});
