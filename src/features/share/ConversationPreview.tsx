import { useLayoutEffect, useRef, useState } from "react";
import { PreviewTurn } from "./PreviewTurn";
import type { PreviewTimelineV1, SnapshotV1 } from "./shareTypes";

interface Props {
  timeline: PreviewTimelineV1 | null;
  snapshot: SnapshotV1 | null;
  loading: boolean;
  error: string | null;
}

const INITIAL_TURN_COUNT = 1;
const TURN_PAGE_SIZE = 3;

export function ConversationPreview({ timeline, snapshot, loading, error }: Props) {
  const [visibleTurnCount, setVisibleTurnCount] = useState(INITIAL_TURN_COUNT);
  const timelineRef = useRef<HTMLOListElement>(null);
  const distanceFromBottomRef = useRef<number | null>(null);

  useLayoutEffect(() => {
    const list = timelineRef.current;
    if (!timeline || !list) {
      return;
    }
    if (distanceFromBottomRef.current === null) {
      const latestTurn = list.querySelector<HTMLElement>('[data-latest-turn-start="true"]');
      if (latestTurn) {
        list.scrollTop += latestTurn.getBoundingClientRect().top - list.getBoundingClientRect().top;
      }
      return;
    }
    list.scrollTop = Math.max(0, list.scrollHeight - distanceFromBottomRef.current);
    distanceFromBottomRef.current = null;
  }, [timeline, visibleTurnCount]);

  if (loading) {
    return <p className="notice">正在读取完整会话历史…</p>;
  }
  if (error) {
    return <p className="notice error" role="alert">{error}</p>;
  }
  if (!timeline || !snapshot) {
    return <p className="notice">选择左侧会话后可预览将分享的完整内容。</p>;
  }

  const visibleStart = Math.max(0, timeline.turns.length - visibleTurnCount);
  const visibleTurns = timeline.turns.slice(visibleStart);
  const remainingTurns = visibleStart;
  const nextTurnCount = Math.min(TURN_PAGE_SIZE, remainingTurns);
  const itemCount = timeline.turns.reduce((count, turn) => count + turn.items.length, 0);

  function showEarlierTurns() {
    const list = timelineRef.current;
    if (list) {
      distanceFromBottomRef.current = list.scrollHeight - list.scrollTop;
    }
    setVisibleTurnCount((count) => count + TURN_PAGE_SIZE);
  }

  return (
    <section className="panel conversation-preview" aria-labelledby="conversation-preview-title">
      <div className="panel-heading">
        <h2 id="conversation-preview-title">会话预览</h2>
        <span>{timeline.turns.length} 轮 · {itemCount} 个事件 · 默认显示最新</span>
      </div>
      <div className="fidelity-notice" role="note">
        <strong>当前为完整分享预览</strong>
        <p>
          敏感内容和大型内容默认折叠；“展开原文”后可与 Codex 中的内容核对。
          <code>.agbridge</code> 原生包始终保留原始会话，不受预览折叠影响。
        </p>
      </div>
      <dl className="metadata-grid">
        <dt>源会话标题</dt>
        <dd>{snapshot.metadata.title || "未命名会话"}</dd>
        <dt>项目 / 仓库</dt>
        <dd>{snapshot.metadata.projectName || snapshot.metadata.repositoryId || "未提供"}</dd>
        <dt>分享方工作目录</dt>
        <dd>
          {snapshot.metadata.sourceWorkingDirectory || "未提供"}{" "}
          <small>（仅作说明，接收方需选择自己的项目目录）</small>
        </dd>
        <dt>截止轮次</dt>
        <dd>{timeline.cutoffTurnId || "完整会话（未截断）"}</dd>
      </dl>
      {timeline.notices.length > 0 && (
        <div className="preview-notices" role="status">
          <strong>预览显示说明</strong>
          <ul>
            {timeline.notices.map((notice) => (
              <li key={notice.code}>
                {notice.message}（{notice.count.toLocaleString("zh-CN")} 项）
              </li>
            ))}
          </ul>
        </div>
      )}
      {remainingTurns > 0 && (
        <button
          type="button"
          className="secondary-action turn-load-earlier"
          onClick={showEarlierTurns}
        >
          显示更早的 {nextTurnCount} 轮（还有 {remainingTurns} 轮）
        </button>
      )}
      <ol ref={timelineRef} className="preview-timeline" aria-label="完整会话时间线">
        {visibleTurns.map((turn, index) => (
          <PreviewTurn
            key={turn.id}
            turn={turn}
            latest={visibleStart + index === timeline.turns.length - 1}
          />
        ))}
      </ol>
    </section>
  );
}
