import { useCallback, useEffect, useState } from "react";
import type { ImportRecord } from "../import/importTypes";
import { historyApi } from "./historyApi";
import { HistoryRow } from "./HistoryRow";

export function HistoryPage() {
  const [records, setRecords] = useState<ImportRecord[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [pendingDelete, setPendingDelete] = useState<ImportRecord | null>(null);

  const load = useCallback(() => {
    historyApi
      .list()
      .then((result) => {
        setRecords(result);
        setError(null);
      })
      .catch(() => setError("无法读取本机导入记录。"));
  }, []);

  useEffect(load, [load]);

  async function retry(record: ImportRecord) {
    try {
      const updated = await historyApi.retry(record.id);
      await load();
      setNotice(updated.status === "failed" ? "重试失败，请查看记录中的错误说明" : "重试已完成");
      setError(null);
    } catch {
      setError("重试失败，请确认 Codex 可用且本机项目目录仍存在。");
    }
  }

  async function open(record: ImportRecord) {
    try {
      await historyApi.open(record.id);
      setError(null);
    } catch {
      setError("当前 Codex 桌面版无法通过公开接口打开该会话。请复制续接提示，在 Codex 中新建会话后粘贴。");
    }
  }

  async function copyPrompt(record: ImportRecord) {
    try {
      const prompt = await historyApi.copyPrompt(record.id);
      await navigator.clipboard.writeText(prompt);
      setNotice("续接提示已复制到剪贴板");
      setError(null);
    } catch {
      setError("无法复制续接提示，请检查系统剪贴板权限，或先重试失败的导入。");
    }
  }

  async function remove() {
    if (!pendingDelete) {
      return;
    }
    try {
      await historyApi.remove(pendingDelete.id);
      await load();
      setNotice("已删除导入记录");
      setError(null);
    } catch {
      setError("删除导入记录失败，请稍后重试。");
    } finally {
      setPendingDelete(null);
    }
  }

  return (
    <section className="panel" aria-labelledby="history-title">
      <div className="panel-heading">
        <h2 id="history-title">导入历史</h2>
        <span>仅显示本机元数据和状态</span>
      </div>
      {error && <p className="notice error" role="alert">{error}</p>}
      {notice && <p className="notice" role="status">{notice}</p>}
      {records.length === 0 && <p className="notice">暂无导入记录。</p>}
      <ul className="history-list">
        {records.map((record) => (
          <HistoryRow
            key={record.id}
            record={record}
            onOpen={(item) => void open(item)}
            onRetry={(item) => void retry(item)}
            onCopyPrompt={(item) => void copyPrompt(item)}
            onDelete={setPendingDelete}
          />
        ))}
      </ul>
      {pendingDelete && (
        <div className="notice" role="dialog" aria-label="确认删除导入记录">
          <p>删除「{pendingDelete.sourceTitle}」的本机导入记录和本机暂存快照？Codex 会话不会被删除。</p>
          <div className="action-row">
            <button type="button" className="primary-action" onClick={() => void remove()}>确认删除</button>
            <button type="button" className="secondary-action" onClick={() => setPendingDelete(null)}>取消</button>
          </div>
        </div>
      )}
    </section>
  );
}
