import type { ThreadSummary, TurnSummary } from "./shareTypes";

interface Props {
  threads: ThreadSummary[];
  turns: TurnSummary[];
  selectedThreadId: string | null;
  selectedTurnId: string | null;
  onSelectThread: (threadId: string) => void;
  onSelectTurn: (turnId: string | null) => void;
}

function formatTime(timestamp: number): string {
  return new Date(timestamp * 1000).toLocaleString("zh-CN", {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  });
}

export function ThreadPicker({
  threads,
  turns,
  selectedThreadId,
  selectedTurnId,
  onSelectThread,
  onSelectTurn,
}: Props) {
  const completedTurns = turns.filter((turn) => turn.completed);

  return (
    <section className="panel thread-picker" aria-labelledby="thread-picker-title">
      <div className="panel-heading">
        <h2 id="thread-picker-title">选择会话</h2>
        <span>{threads.length} 个会话</span>
      </div>
      <ul className="thread-list">
        {threads.map((thread) => (
          <li key={thread.id}>
            <button
              type="button"
              className={`thread-row ${selectedThreadId === thread.id ? "selected" : ""}`}
              aria-pressed={selectedThreadId === thread.id}
              onClick={() => onSelectThread(thread.id)}
            >
              <strong>{thread.title || thread.preview || "未命名会话"}</strong>
              <span className="thread-meta">{formatTime(thread.updatedAt)} · {thread.cwd}</span>
            </button>
          </li>
        ))}
      </ul>
      <label className="field-label" htmlFor="cutoff-turn">
        截止轮次
      </label>
      <select
        id="cutoff-turn"
        value={selectedTurnId ?? ""}
        disabled={completedTurns.length === 0}
        onChange={(event) => onSelectTurn(event.target.value || null)}
      >
        <option value="">最近一个已完成轮次</option>
        {completedTurns.map((turn, index) => (
          <option key={turn.id} value={turn.id}>
            第 {index + 1} 轮 · {turn.id.slice(0, 8)}
          </option>
        ))}
      </select>
    </section>
  );
}
