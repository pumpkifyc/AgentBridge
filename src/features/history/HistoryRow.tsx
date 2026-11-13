import type { ImportRecord } from "../import/importTypes";

interface Props {
  record: ImportRecord;
  onOpen: (record: ImportRecord) => void;
  onRetry: (record: ImportRecord) => void;
  onDelete: (record: ImportRecord) => void;
}

const STATUS_LABELS: Record<string, string> = {
  pending: "处理中",
  completed: "已完成",
  failed: "失败",
};

export function HistoryRow({
  record,
  onOpen,
  onRetry,
  onDelete,
}: Props) {
  return (
    <li className="history-row">
      <div className="panel-heading">
        <strong>{record.sourceTitle || "未命名会话"}</strong>
        <span>{STATUS_LABELS[record.status] ?? record.status}</span>
      </div>
      <small>本机项目：{record.projectPath}</small>
      <small>本机会话：{record.localThreadId || "尚未创建"}</small>
      {record.error && <small className="hint">{record.error}</small>}
      {record.retryCount > 0 && <small>已重试 {record.retryCount} 次</small>}
      <div className="action-row">
        {record.status === "completed" && (
          <button type="button" className="secondary-action" onClick={() => onOpen(record)}>
            打开会话
          </button>
        )}
        {record.status === "failed" && (
          <button type="button" className="secondary-action" onClick={() => onRetry(record)}>
            重试导入
          </button>
        )}
        <button type="button" className="secondary-action" onClick={() => onDelete(record)}>
          删除记录
        </button>
      </div>
    </li>
  );
}
