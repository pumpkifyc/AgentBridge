import { useRef, useState } from "react";
import { ShareApproval } from "./ShareApproval";
import { TransferProgress } from "./TransferProgress";
import { receiveApi } from "./receiveApi";
import type {
  ReceivedPreview,
  StagedSnapshot,
  TransferProgress as Progress,
} from "./receiveTypes";
import { parsePairingInfo } from "../share/pairingApi";

type State =
  | { kind: "input"; message?: string }
  | { kind: "previewing" }
  | { kind: "approval"; preview: ReceivedPreview }
  | { kind: "downloading"; preview: ReceivedPreview; progress: Progress }
  | { kind: "retry"; preview: ReceivedPreview; message: string }
  | { kind: "staged"; snapshot: StagedSnapshot }
  | { kind: "error"; message: string };

const PAIRING_ERROR = "配对信息无效，请从分享方重新复制完整的 JSON 文本。";
const TRANSFER_ERROR = "传输中断，可检查局域网连接后重试。";

export function ReceivePage() {
  const [pairingText, setPairingText] = useState("");
  const [state, setState] = useState<State>({ kind: "input" });
  const activeTransfer = useRef(0);

  function startTransfer(preview: ReceivedPreview, retry: boolean) {
    const transferId = activeTransfer.current + 1;
    activeTransfer.current = transferId;
    setState({
      kind: "downloading",
      preview,
      progress: { kind: "start", transferredBytes: 0, totalBytes: preview.totalBytes },
    });
    return (async () => {
      try {
        if (retry) {
          await receiveApi.retryPreview(pairingText, preview.shareId);
        }
        const snapshot = await receiveApi.accept(preview.shareId, (event) => {
          setState((previous) => activeTransfer.current === transferId
            && previous.kind === "downloading"
            ? { ...previous, progress: event }
            : previous);
        });
        if (activeTransfer.current === transferId) {
          setState({ kind: "staged", snapshot });
        }
      } catch {
        if (activeTransfer.current === transferId) {
          setState({ kind: "retry", preview, message: TRANSFER_ERROR });
        }
      }
    })();
  }

  async function preview(retryShareId?: string) {
    try {
      await parsePairingInfo(pairingText);
    } catch {
      setState({ kind: "error", message: PAIRING_ERROR });
      return;
    }
    setState({ kind: "previewing" });
    try {
      const result = retryShareId
        ? await receiveApi.retryPreview(pairingText, retryShareId)
        : await receiveApi.preview(pairingText);
      setState({ kind: "approval", preview: result });
    } catch {
      setState({ kind: "error", message: "无法连接分享方，请检查地址、证书指纹和邀请是否仍然有效。" });
    }
  }

  function accept(current: ReceivedPreview, retry = false) {
    return startTransfer(current, retry);
  }

  async function decline(shareId: string) {
    try {
      await receiveApi.decline(shareId);
      setState({ kind: "input", message: "已拒绝该分享" });
    } catch {
      setState({ kind: "error", message: "拒绝未完成，邀请可能已过期或被撤销。" });
    }
  }

  async function cancel(shareId: string) {
    activeTransfer.current += 1;
    try {
      await receiveApi.cancel(shareId);
    } catch {
      // 本地取消仍应结束界面上的传输状态。
    }
    setState({ kind: "input", message: "下载已取消" });
  }

  async function deleteStaged(shareId: string) {
    try {
      await receiveApi.deleteStaged(shareId);
      setState({ kind: "input", message: "已删除本机暂存快照" });
    } catch {
      setState({ kind: "error", message: "删除暂存快照失败，请稍后重试。" });
    }
  }

  return (
    <section className="receive-page" aria-labelledby="receive-title">
      <div className="panel">
        <div className="panel-heading">
          <h2 id="receive-title">接收会话</h2>
          <span>点对点 · 先预览后下载</span>
        </div>
        <label className="field-label" htmlFor="pairing-info">配对信息</label>
        <textarea
          id="pairing-info"
          aria-label="配对信息"
          rows={4}
          value={pairingText}
          onChange={(event) => setPairingText(event.target.value)}
          placeholder="粘贴分享方提供的配对 JSON"
        />
        <p className="hint">请通过可信渠道核对验证码和证书指纹；凭据不会写入日志。</p>
        <div className="action-row">
          <button
            type="button"
            className="primary-action"
            disabled={state.kind === "previewing" || state.kind === "downloading"}
            onClick={() => void preview()}
          >
            预览分享
          </button>
        </div>
      </div>

      {state.kind === "input" && state.message && <p className="notice">{state.message}</p>}
      {state.kind === "previewing" && <p className="notice">正在读取分享元数据…</p>}
      {state.kind === "error" && <p className="notice error" role="alert">{state.message}</p>}
      {state.kind === "approval" && (
        <ShareApproval
          preview={state.preview}
          busy={false}
          onAccept={() => void accept(state.preview)}
          onDecline={() => void decline(state.preview.shareId)}
        />
      )}
      {state.kind === "downloading" && (
        <div className="panel">
          <TransferProgress progress={state.progress} />
          <div className="action-row">
            <button
              type="button"
              className="secondary-action"
              onClick={() => void cancel(state.preview.shareId)}
            >
              取消下载
            </button>
          </div>
        </div>
      )}
      {state.kind === "retry" && (
        <div className="panel">
          <p className="notice error" role="alert">{state.message}</p>
          <TransferProgress progress={{ kind: "failed", transferredBytes: 0, totalBytes: state.preview.totalBytes }} />
          <div className="action-row">
            <button type="button" className="primary-action" onClick={() => void accept(state.preview, true)}>
              重试下载
            </button>
          </div>
        </div>
      )}
      {state.kind === "staged" && (
        <div className="panel">
          <h2>快照已暂存，等待选择本地项目后导入</h2>
          <TransferProgress progress={{ kind: "complete", transferredBytes: state.snapshot.totalBytes, totalBytes: state.snapshot.totalBytes }} />
          <p className="hint">快照已通过完整性校验并保存在本机。导入页面将在你确认本地项目目录后创建独立 Codex 会话。</p>
          <div className="action-row">
            <button type="button" className="secondary-action" onClick={() => void deleteStaged(state.snapshot.shareId)}>
              删除暂存快照
            </button>
          </div>
        </div>
      )}
    </section>
  );
}
