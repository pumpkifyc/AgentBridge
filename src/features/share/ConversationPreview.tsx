import type { SnapshotV1 } from "./shareTypes";

interface Props {
  snapshot: SnapshotV1 | null;
  loading: boolean;
  error: string | null;
}

export function ConversationPreview({ snapshot, loading, error }: Props) {
  if (loading) {
    return <p className="notice">正在读取会话快照…</p>;
  }
  if (error) {
    return <p className="notice error" role="alert">{error}</p>;
  }
  if (!snapshot) {
    return <p className="notice">选择左侧会话后可预览将分享的内容。</p>;
  }

  return (
    <section className="panel conversation-preview" aria-labelledby="conversation-preview-title">
      <div className="panel-heading">
        <h2 id="conversation-preview-title">快照预览</h2>
        <span>{snapshot.messages.length} 条消息</span>
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
      <ol className="message-list">
        {snapshot.messages.map((message, index) => (
          <li key={`${message.turnIndex}-${index}`} className={`message ${message.role}`}>
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
