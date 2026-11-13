import type { TransferProgress as Progress } from "./receiveTypes";

interface Props {
  progress: Progress;
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) {
    return `${bytes} B`;
  }
  return `${(bytes / 1024).toFixed(1)} KB`;
}

export function TransferProgress({ progress }: Props) {
  const percentage = progress.totalBytes === 0
    ? 100
    : Math.min(100, Math.round((progress.transferredBytes / progress.totalBytes) * 100));
  return (
    <div className="transfer-progress" role="status">
      <strong>正在接收原生会话包…</strong>
      <progress value={percentage} max={100} aria-valuenow={percentage} />
      <span>{formatBytes(progress.transferredBytes)} / {formatBytes(progress.totalBytes)}</span>
    </div>
  );
}
