import type { ReceivedPreview } from "./receiveTypes";

interface Props {
  preview: ReceivedPreview;
  busy: boolean;
  onAccept: () => void;
  onDecline: () => void;
}

export function ShareApproval({ preview, busy, onAccept, onDecline }: Props) {
  const { metadata } = preview;
  return (
    <section className="panel" aria-labelledby="share-approval-title">
      <div className="panel-heading">
        <h2 id="share-approval-title">确认接收</h2>
        <span>仅显示元数据</span>
      </div>
      <dl className="metadata-grid">
        <dt>源会话标题</dt>
        <dd>{metadata.sourceTitle || "未命名会话"}</dd>
        <dt>项目 / 仓库</dt>
        <dd>{metadata.projectName || metadata.repositoryId || "未提供"}</dd>
        <dt>分享方工作目录</dt>
        <dd>
          {metadata.sourceWorkingDirectory || "未提供"}{" "}
          <small>仅作说明，接收方需选择自己的项目目录</small>
        </dd>
        <dt>消息数量</dt>
        <dd>{metadata.messageCount} 条消息</dd>
        <dt>截止轮次</dt>
        <dd>{metadata.cutoffTurnId || "最近一个已完成轮次"}</dd>
        <dt>快照大小</dt>
        <dd>{preview.totalBytes.toLocaleString()} 字节</dd>
      </dl>
      {metadata.warnings.length > 0 && (
        <div className="warnings" role="status">
          <strong>内容限制</strong>
          <ul>
            {metadata.warnings.map((warning) => <li key={warning}>{warning}</li>)}
          </ul>
        </div>
      )}
      <p className="hint">接受后只会下载并暂存快照；选择本地项目并按下“导入”后才会创建 Codex 会话。</p>
      <div className="action-row">
        <button type="button" className="primary-action" disabled={busy} onClick={onAccept}>
          接受并下载
        </button>
        <button type="button" className="secondary-action" disabled={busy} onClick={onDecline}>
          拒绝分享
        </button>
      </div>
    </section>
  );
}
