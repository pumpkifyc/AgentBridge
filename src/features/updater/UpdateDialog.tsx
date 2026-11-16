import type { UpdateUiState } from "./updateTypes";

interface Props {
  state: Exclude<UpdateUiState, { kind: "idle" }>;
  onClose: () => void;
  onDownloadRequest: () => void;
  onConfirmDownload: () => void;
  onRetryCheck: () => void;
  onRetryDownload: () => void;
}

const DATE_FORMATTER = new Intl.DateTimeFormat("zh-CN", {
  year: "numeric",
  month: "2-digit",
  day: "2-digit",
});

const TIME_FORMATTER = new Intl.DateTimeFormat("zh-CN", {
  hour: "2-digit",
  minute: "2-digit",
  second: "2-digit",
});

function formatDate(value?: string): string {
  if (!value) {
    return "发布日期未知";
  }
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? value : DATE_FORMATTER.format(date);
}

function formatBytes(value: number): string {
  if (!Number.isFinite(value) || value <= 0) {
    return "0 B";
  }
  const units = ["B", "KB", "MB", "GB"];
  const index = Math.min(
    units.length - 1,
    Math.floor(Math.log(value) / Math.log(1024)),
  );
  const amount = value / 1024 ** index;
  return `${amount >= 10 || index === 0 ? amount.toFixed(0) : amount.toFixed(1)} ${units[index]}`;
}

function dialogTitle(state: Exclude<UpdateUiState, { kind: "idle" }>): string {
  switch (state.kind) {
    case "checking":
      return "正在检查更新";
    case "current":
      return "当前已是最新版本";
    case "available":
      return "发现新版本";
    case "confirm":
      return "确认更新";
    case "downloading":
      return "正在下载";
    case "installing":
      return "正在安装更新";
    case "error":
      return "更新失败";
  }
}

export function UpdateDialog({
  state,
  onClose,
  onDownloadRequest,
  onConfirmDownload,
  onRetryCheck,
  onRetryDownload,
}: Props) {
  return (
    <div className="update-dialog-backdrop">
      <section
        className="update-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="update-dialog-title"
      >
        <div className="update-dialog-heading">
          <p className="eyebrow">AgentBridge 更新</p>
          <h2 id="update-dialog-title">{dialogTitle(state)}</h2>
        </div>

        {state.kind === "checking" && (
          <div className="update-loading" role="status">
            <span className="update-spinner" aria-hidden="true" />
            <p>正在读取 GitHub Releases 中的最新版本…</p>
          </div>
        )}

        {state.kind === "current" && (
          <>
            <p>当前版本 v{state.currentVersion} 已是最新版。</p>
            <p className="update-meta">
              最后检查：{TIME_FORMATTER.format(state.checkedAt)}
            </p>
            <div className="update-dialog-actions">
              <button type="button" className="primary-action" onClick={onClose}>
                关闭
              </button>
            </div>
          </>
        )}

        {state.kind === "available" && (
          <>
            <div className="update-version-grid">
              <span>当前版本</span>
              <strong>v{state.update.currentVersion}</strong>
              <span>新版本</span>
              <strong>v{state.update.version}</strong>
              <span>发布日期</span>
              <strong>{formatDate(state.update.date)}</strong>
            </div>
            <div className="update-notes" aria-label="更新内容">
              {state.update.notes || "本次更新暂未提供详细说明。"}
            </div>
            <div className="update-dialog-actions">
              <button
                type="button"
                className="secondary-action"
                onClick={onClose}
              >
                稍后再说
              </button>
              <button
                type="button"
                className="primary-action"
                onClick={onDownloadRequest}
              >
                下载并更新
              </button>
            </div>
          </>
        )}

        {state.kind === "confirm" && (
          <>
            <p>
              将下载并安装 AgentBridge v{state.update.version}
              。更新过程中应用将退出并重启；当前版本在下载或签名校验失败时保持不变。
            </p>
            <div className="update-dialog-actions">
              <button
                type="button"
                className="secondary-action"
                onClick={onClose}
              >
                取消
              </button>
              <button
                type="button"
                className="primary-action"
                onClick={onConfirmDownload}
              >
                确认更新
              </button>
            </div>
          </>
        )}

        {state.kind === "downloading" && (
          <>
            <p>
              {state.progress.percent === undefined
                ? `已下载 ${formatBytes(state.progress.downloadedBytes)}`
                : `正在下载 ${state.progress.percent}%`}
            </p>
            <progress
              className="update-progress"
              aria-label="更新下载进度"
              max={100}
              value={state.progress.percent}
            />
            <p className="update-meta">
              {formatBytes(state.progress.downloadedBytes)}
              {state.progress.totalBytes === undefined
                ? ""
                : ` / ${formatBytes(state.progress.totalBytes)}`}
            </p>
          </>
        )}

        {state.kind === "installing" && (
          <div className="update-loading" role="status">
            <span className="update-spinner" aria-hidden="true" />
            <p>下载和签名校验已完成，正在安装并重启应用…</p>
          </div>
        )}

        {state.kind === "error" && (
          <>
            <p className="notice error update-error" role="alert">
              {state.message}
            </p>
            <p>当前版本仍可正常使用。你可以关闭弹窗后继续工作。</p>
            <div className="update-dialog-actions">
              <button
                type="button"
                className="secondary-action"
                onClick={onClose}
              >
                关闭
              </button>
              {state.retry === "check" && (
                <button
                  type="button"
                  className="primary-action"
                  onClick={onRetryCheck}
                >
                  重试检查
                </button>
              )}
              {state.retry === "download" && (
                <button
                  type="button"
                  className="primary-action"
                  onClick={onRetryDownload}
                >
                  重试下载
                </button>
              )}
            </div>
          </>
        )}
      </section>
    </div>
  );
}
