import * as QRCode from "qrcode";
import { useCallback, useEffect, useMemo, useState } from "react";
import { ShareStatus } from "./ShareStatus";
import { buildPairingInfo, pairingApi } from "./pairingApi";
import type { ShareSession, ShareState } from "./pairingApi";

interface Props {
  threadId: string | null;
  previewReady: boolean;
}

const POLL_INTERVAL_MS = 1500;
const ACTIVE_STATES: ShareState[] = [
  "等待配对",
  "已配对",
  "预览中",
  "传输中",
  "失败",
];

export function PairingPanel({ threadId, previewReady }: Props) {
  const [session, setSession] = useState<ShareSession | null>(null);
  const [state, setState] = useState<ShareState | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [bindIp, setBindIp] = useState("127.0.0.1");
  const [qrCode, setQrCode] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);

  const reset = useCallback(() => {
    setSession(null);
    setState(null);
    setError(null);
    setQrCode(null);
    setCopied(false);
  }, []);

  useEffect(() => {
    reset();
  }, [threadId, previewReady, reset]);

  const pairingInfo = useMemo(
    () => (session ? buildPairingInfo(session) : ""),
    [session],
  );

  useEffect(() => {
    if (!session) {
      return;
    }
    let active = true;
    QRCode.toString(pairingInfo, {
      type: "svg",
      margin: 1,
      errorCorrectionLevel: "M",
    }).then((svg) => {
      if (active) {
        setQrCode(`data:image/svg+xml;base64,${btoa(svg)}`);
      }
    }).catch(() => {
      if (active) {
        setQrCode(null);
      }
    });
    return () => {
      active = false;
    };
  }, [session, pairingInfo]);

  useEffect(() => {
    if (!session) {
      return;
    }
    let active = true;
    const timer = window.setInterval(async () => {
      if (Date.now() / 1000 >= session.expiresAt) {
        if (active) {
          setState("已过期");
          setSession(null);
          setQrCode(null);
        }
        return;
      }
      try {
        const next = await pairingApi.status(session.id);
        if (active) {
          setState(next);
        }
      } catch {
        if (active) {
          setState("失败");
          setError("无法读取分享状态，可重新创建邀请。");
        }
      }
    }, POLL_INTERVAL_MS);
    return () => {
      active = false;
      window.clearInterval(timer);
    };
  }, [session]);

  async function start() {
    if (!threadId || !previewReady) {
      setError("请先选择并预览一个会话。");
      return;
    }
    setBusy(true);
    setError(null);
    setCopied(false);
    try {
      const created = await pairingApi.startShare(threadId, bindIp);
      setSession(created);
      setState(created.state);
    } catch {
      setState("失败");
      setError("无法创建分享，请确认局域网地址有效后重试。");
      setSession(null);
    } finally {
      setBusy(false);
    }
  }

  async function stop(kind: "revoke" | "cancel") {
    if (!session) {
      return;
    }
    setBusy(true);
    try {
      if (kind === "revoke") {
        await pairingApi.revoke(session.id);
      } else {
        await pairingApi.cancel(session.id);
      }
      setState(kind === "revoke" ? "已撤销" : "已取消");
      setError(null);
      setSession(null);
      setQrCode(null);
    } catch {
      setState("失败");
      setError("操作未完成，可以重新创建邀请。");
    } finally {
      setBusy(false);
    }
  }

  async function copy() {
    if (!pairingInfo) {
      return;
    }
    try {
      await navigator.clipboard.writeText(pairingInfo);
      setCopied(true);
    } catch {
      setError("自动复制失败，请手动复制下方配对信息。");
    }
  }

  const activeSession = session !== null && ACTIVE_STATES.includes(state ?? "等待配对");

  return (
    <section className="panel pairing-panel" aria-labelledby="pairing-title">
      <div className="panel-heading">
        <h2 id="pairing-title">局域网配对</h2>
        <ShareStatus state={state} error={error} busy={busy} />
      </div>
      <label className="field-label" htmlFor="bind-ip">本机局域网地址</label>
      <input
        id="bind-ip"
        value={bindIp}
        onChange={(event) => setBindIp(event.target.value)}
        placeholder="例如 192.168.1.20"
        disabled={busy || activeSession}
      />
      <p className="hint">只绑定你选择的地址；接收方需在同一局域网内。</p>
      <div className="action-row">
        <button
          type="button"
          className="primary-action"
          disabled={busy || activeSession}
          onClick={() => void start()}
        >
          {state && state !== "等待配对" ? "重新创建邀请" : "创建一次性邀请"}
        </button>
        {activeSession && (
          <>
            <button type="button" className="secondary-action" disabled={busy} onClick={() => void stop("revoke")}>
              立即撤销
            </button>
            <button type="button" className="secondary-action" disabled={busy} onClick={() => void stop("cancel")}>
              取消分享
            </button>
          </>
        )}
      </div>
      {activeSession && session && (
        <div className="pairing-details">
          <dl className="metadata-grid">
            <dt>连接地址</dt>
            <dd>{session.endpoint}</dd>
            <dt>验证码</dt>
            <dd className="verification-code">{session.verificationCode}</dd>
            <dt>证书指纹</dt>
            <dd className="fingerprint">{session.fingerprint}</dd>
          </dl>
          <p className="hint">请将以下配对信息通过可信渠道发送给同事；有效期为 5 分钟且只能完成一次传输。</p>
          {qrCode && <img className="pairing-qr" src={qrCode} alt="配对二维码" />}
          <textarea readOnly aria-label="配对信息" value={pairingInfo} rows={4} />
          <div className="action-row">
            <button type="button" className="secondary-action" onClick={() => void copy()}>
              复制配对信息
            </button>
            {copied && <span className="hint">已复制到剪贴板</span>}
          </div>
          <p className="hint">配对信息包含一次性凭据，请勿写入日志或截图外传。</p>
        </div>
      )}
    </section>
  );
}
