import { useLayoutEffect, useRef, useState } from "react";
import type { SnapshotV1 } from "./shareTypes";

interface Props {
  snapshot: SnapshotV1 | null;
  loading: boolean;
  error: string | null;
}

const INITIAL_MESSAGE_COUNT = 20;
const MESSAGE_PAGE_SIZE = 50;

export function ConversationPreview({ snapshot, loading, error }: Props) {
  const [visibleMessageCount, setVisibleMessageCount] = useState(INITIAL_MESSAGE_COUNT);
  const messageListRef = useRef<HTMLOListElement>(null);
  const distanceFromBottomRef = useRef<number | null>(null);
  const latestTurnIndex = snapshot && snapshot.messages.length > 0
    ? snapshot.messages[snapshot.messages.length - 1].turnIndex
    : null;
  const latestTurnStartIndex = latestTurnIndex === null
    ? -1
    : snapshot?.messages.findIndex((message) => message.turnIndex === latestTurnIndex) ?? -1;

  useLayoutEffect(() => {
    const messageList = messageListRef.current;
    if (!snapshot || !messageList) {
      return;
    }

    if (distanceFromBottomRef.current === null) {
      const latestTurnStart = messageList.querySelector<HTMLElement>('[data-latest-turn-start="true"]');
      if (latestTurnStart) {
        const listTop = messageList.getBoundingClientRect().top;
        const messageTop = latestTurnStart.getBoundingClientRect().top;
        messageList.scrollTop += messageTop - listTop;
      } else {
        messageList.scrollTop = messageList.scrollHeight;
      }
      return;
    }

    messageList.scrollTop = Math.max(0, messageList.scrollHeight - distanceFromBottomRef.current);
    distanceFromBottomRef.current = null;
  }, [snapshot, visibleMessageCount]);

  if (loading) {
    return <p className="notice">正在读取会话快照…</p>;
  }
  if (error) {
    return <p className="notice error" role="alert">{error}</p>;
  }
  if (!snapshot) {
    return <p className="notice">选择左侧会话后可预览将分享的内容。</p>;
  }

  const latestTurnMessageCount = latestTurnStartIndex >= 0
    ? snapshot.messages.length - latestTurnStartIndex
    : 0;
  const renderedMessageCount = Math.max(visibleMessageCount, latestTurnMessageCount);
  const visibleStartIndex = Math.max(snapshot.messages.length - renderedMessageCount, 0);
  const visibleMessages = snapshot.messages.slice(visibleStartIndex);
  const remainingMessageCount = visibleStartIndex;
  const nextMessageCount = Math.min(MESSAGE_PAGE_SIZE, remainingMessageCount);

  function showEarlierMessages() {
    const messageList = messageListRef.current;
    if (messageList) {
      distanceFromBottomRef.current = messageList.scrollHeight - messageList.scrollTop;
    }
    setVisibleMessageCount((count) => count + MESSAGE_PAGE_SIZE);
  }

  return (
    <section className="panel conversation-preview" aria-labelledby="conversation-preview-title">
      <div className="panel-heading">
        <h2 id="conversation-preview-title">快照预览</h2>
        <span>{snapshot.messages.length} 条消息 · 默认显示最新</span>
      </div>
      <dl className="metadata-grid">
        <dt>源会话标题</dt>
        <dd>{snapshot.metadata.title || "未命名会话"}</dd>
        <dt>项目 / 仓库</dt>
        <dd>{snapshot.metadata.projectName || snapshot.metadata.repositoryId || "未提供"}</dd>
        <dt>分享方工作目录</dt>
        <dd>{snapshot.metadata.sourceWorkingDirectory || "未提供"} <small>（仅作说明，接收方需选择自己的项目目录）</small></dd>
        <dt>截止轮次</dt>
        <dd>{snapshot.metadata.cutoffTurnId || "最近一个已完成轮次"}</dd>
      </dl>
      {snapshot.warnings.length > 0 && (
        <div className="warnings" role="status">
          <strong>内容限制</strong>
          <ul>{snapshot.warnings.map((warning) => <li key={warning}>{warning}</li>)}</ul>
        </div>
      )}
      {remainingMessageCount > 0 && (
        <button
          type="button"
          className="secondary-action message-load-earlier"
          onClick={showEarlierMessages}
        >
          显示更早的 {nextMessageCount} 条（还有 {remainingMessageCount} 条）
        </button>
      )}
      <ol ref={messageListRef} className="message-list" aria-label="快照消息列表">
        {visibleMessages.map((message, index) => (
          <li
            key={`${message.turnIndex}-${index}`}
            className={`message ${message.role}`}
            data-latest-turn-start={visibleStartIndex + index === latestTurnStartIndex ? "true" : undefined}
          >
            <span className="message-role">{message.role === "user" ? "用户" : "助手"} · 第 {message.turnIndex + 1} 轮</span>
            <p>{message.text}</p>
            {message.historicalToolSummary && (
              <p className="tool-summary">历史工具结果：{message.historicalToolSummary}</p>
            )}
          </li>
        ))}
      </ol>
    </section>
  );
}
