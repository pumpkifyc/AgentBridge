import type { ShareState } from "./pairingApi";

interface Props {
  state: ShareState | null;
  error: string | null;
  busy?: boolean;
}

export const SHARE_STATES: ShareState[] = [
  "等待配对",
  "已配对",
  "预览中",
  "传输中",
  "传输完成",
  "已导入",
  "已拒绝",
  "已过期",
  "已撤销",
  "已取消",
  "失败",
];

export function ShareStatus({ state, error, busy = false }: Props) {
  return (
    <div className="share-status" aria-live="polite">
      <span className={`status-badge ${state === "失败" || state === "已过期" ? "alert" : ""}`}>
        {state || "尚未开始分享"}
      </span>
      {busy && <span className="status-busy">正在处理…</span>}
      {error && <p className="notice error" role="alert">{error}</p>}
    </div>
  );
}
