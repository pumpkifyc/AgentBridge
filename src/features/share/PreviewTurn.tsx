import { PreviewBlock } from "./PreviewBlock";
import type { PreviewItem, PreviewTurn as PreviewTurnType } from "./shareTypes";

interface Props {
  turn: PreviewTurnType;
  latest: boolean;
}

function PreviewItemView({ item }: { item: PreviewItem }) {
  return (
    <li
      className={`preview-item preview-item-${item.kind}`}
      data-testid="preview-item"
      data-item-kind={item.kind}
    >
      <div className="preview-item-heading">
        <strong>{item.title}</strong>
        {item.status && <span className={`preview-status status-${item.status}`}>{item.status}</span>}
        {item.sensitive && <span className="sensitive-badge">敏感内容已隐藏</span>}
      </div>
      <div className="preview-block-list">
        {item.blocks.map((block, index) => (
          <PreviewBlock key={`${item.id}-${index}-${block.kind}`} block={block} />
        ))}
      </div>
    </li>
  );
}

export function PreviewTurn({ turn, latest }: Props) {
  return (
    <li
      className="preview-turn"
      data-latest-turn-start={latest ? "true" : undefined}
    >
      <div className="preview-turn-heading">
        <span>第 {turn.index + 1} 轮</span>
        <span>{turn.status}</span>
      </div>
      <ol className="preview-item-list" aria-label={`第 ${turn.index + 1} 轮事件`}>
        {turn.items.map((item) => (
          <PreviewItemView key={item.id} item={item} />
        ))}
      </ol>
    </li>
  );
}
