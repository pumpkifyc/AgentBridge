const NATIVE_PRESERVED = "原生分享包仍完整保留";
const NATIVE_PRESERVED_LITERAL = `${NATIVE_PRESERVED}原文`;

function withoutLegacyPrefix(warning: string): string {
  return warning.replace(/^摘要预览：/, "").trim();
}

function formatOne(warning: string): string {
  const normalized = withoutLegacyPrefix(warning);
  if (
    normalized.startsWith("预览中")
    || normalized.includes(NATIVE_PRESERVED)
    || normalized.includes("Codex 当前可提供的原始记录")
  ) {
    return normalized;
  }

  const unsupported = normalized.match(/^已省略不支持的 (.+?) 内容(（共 (\d+) 项）)?$/);
  if (unsupported) {
    const count = unsupported[3] ? `（共 ${unsupported[3]} 项）` : "";
    return `预览中 ${unsupported[1]} 已折叠${count}；${NATIVE_PRESERVED}`;
  }

  const binary = normalized.match(/^已省略二进制附件（(.+)）$/);
  if (binary) {
    return `预览中未展开二进制附件（${binary[1]}）；${NATIVE_PRESERVED}`;
  }

  const image = normalized.match(/^已省略(图片附件)(（共 (\d+) 项）)?$/);
  if (image) {
    const count = image[3] ? `（共 ${image[3]} 项）` : "";
    return `预览中 ${image[1]} 已折叠${count}；${NATIVE_PRESERVED}`;
  }

  if (normalized === "检测到疑似凭据，已在快照中隐藏") {
    return `预览中检测到疑似凭据，默认隐藏；${NATIVE_PRESERVED_LITERAL}`;
  }

  const truncated = normalized.match(/^第 (\d+) 轮历史内容已截断$/);
  if (truncated) {
    return `预览中第 ${truncated[1]} 轮历史内容不完整；原生分享包保留 Codex 当前可提供的原始记录`;
  }

  return normalized;
}

export function formatPreviewWarnings(warnings: string[]): string[] {
  return [...new Set(warnings.map(formatOne).filter(Boolean))];
}
