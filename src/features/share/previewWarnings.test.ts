import { describe, expect, it } from "vitest";
import { formatPreviewWarnings } from "./previewWarnings";

describe("预览显示说明兼容", () => {
  it("把旧版省略文案解释为仅影响预览", () => {
    expect(formatPreviewWarnings([
      "已省略不支持的 文件变更 内容（共 2 项）",
      "已省略图片附件",
      "已省略二进制附件（image/png）",
    ])).toEqual([
      "预览中 文件变更 已折叠（共 2 项）；原生分享包仍完整保留",
      "预览中 图片附件 已折叠；原生分享包仍完整保留",
      "预览中未展开二进制附件（image/png）；原生分享包仍完整保留",
    ]);
  });

  it("明确敏感内容只影响预览并保留原文", () => {
    expect(formatPreviewWarnings(["检测到疑似凭据，已在快照中隐藏"]))
      .toEqual(["预览中检测到疑似凭据，默认隐藏；原生分享包仍完整保留原文"]);
  });

  it("保留范围明确的新版提示并去重", () => {
    const warning = "预览中 文件变更 已折叠；原生分享包仍完整保留";
    expect(formatPreviewWarnings([`摘要预览：${warning}`, warning]))
      .toEqual([warning]);
  });
});
