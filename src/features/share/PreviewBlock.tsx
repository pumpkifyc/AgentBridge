import { useEffect, useState } from "react";
import type { PreviewBlock as PreviewBlockType } from "./shareTypes";

interface Props {
  block: PreviewBlockType;
}

function fileLabel(value: string): string {
  return value.split(/[\\/]/).filter(Boolean).pop() || value;
}

function BlockContent({ block }: Props) {
  if (block.kind === "text") {
    return <p className="preview-block-text">{block.content}</p>;
  }
  if (block.kind === "image") {
    return (
      <div className="preview-image-reference">
        <strong>{fileLabel(block.content)}</strong>
        <code>{block.content}</code>
      </div>
    );
  }
  if (block.kind === "diff") {
    return (
      <pre className="preview-code preview-diff">
        {block.content.split("\n").map((line, index) => {
          const kind = line.startsWith("+") && !line.startsWith("+++")
            ? "add"
            : line.startsWith("-") && !line.startsWith("---")
              ? "remove"
              : "context";
          return (
            <span key={`${index}-${line.slice(0, 12)}`} className={`diff-line diff-${kind}`}>
              {line || " "}
            </span>
          );
        })}
      </pre>
    );
  }
  return (
    <pre className={`preview-code preview-${block.kind}`}>
      {block.content}
    </pre>
  );
}

export function PreviewBlock({ block }: Props) {
  const [revealed, setRevealed] = useState(!block.sensitive);
  const [expanded, setExpanded] = useState(!block.sensitive && block.defaultExpanded);

  useEffect(() => {
    setRevealed(!block.sensitive);
    setExpanded(!block.sensitive && block.defaultExpanded);
  }, [block]);

  if (!revealed) {
    return (
      <div className="preview-block sensitive-block">
        <button
          type="button"
          className="preview-block-toggle sensitive-reveal"
          aria-expanded={false}
          onClick={() => {
            setRevealed(true);
            setExpanded(true);
          }}
        >
          显示原文
        </button>
        <span className="sensitive-mask">疑似敏感内容，默认隐藏；只影响预览。</span>
      </div>
    );
  }

  return (
    <div className={`preview-block preview-block-${block.kind}`}>
      <button
        type="button"
        className="preview-block-toggle"
        aria-label={`${expanded ? "收起" : "展开"} ${block.label}`}
        aria-expanded={expanded}
        onClick={() => setExpanded((value) => !value)}
      >
        <span>{block.label}</span>
        <span>{expanded ? "收起" : "展开"}</span>
      </button>
      {expanded && <BlockContent block={block} />}
    </div>
  );
}
