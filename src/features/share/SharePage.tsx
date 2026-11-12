import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { ConversationPreview } from "./ConversationPreview";
import { PairingPanel } from "./PairingPanel";
import { ThreadPicker } from "./ThreadPicker";
import { describeError } from "./shareErrors";
import { shareApi } from "./shareApi";
import type {
  CodexStatus,
  SharePageState,
  SnapshotV1,
  ThreadSummary,
  TurnSummary,
} from "./shareTypes";

interface PreparedPreview {
  id: string;
  threadId: string;
  cutoffTurnId: string | null;
  snapshot: SnapshotV1;
}

function toPageState(status: CodexStatus, threads: ThreadSummary[]): SharePageState {
  if (status.state === "unsupported") {
    return { kind: "unsupported", status };
  }
  if (status.state === "error") {
    return { kind: "error", message: status.detail || "无法连接本机 Codex" };
  }
  if (status.state === "empty" || threads.length === 0) {
    return {
      kind: "empty",
      status: { ...status, state: "empty", detail: "" },
    };
  }
  return { kind: "available", status, threads, turns: [] };
}

export function SharePage() {
  const [pageState, setPageState] = useState<SharePageState>({ kind: "loading" });
  const [selectedThreadId, setSelectedThreadId] = useState<string | null>(null);
  const [selectedTurnId, setSelectedTurnId] = useState<string | null>(null);
  const [turns, setTurns] = useState<TurnSummary[]>([]);
  const [preview, setPreview] = useState<PreparedPreview | null>(null);
  const [previewLoading, setPreviewLoading] = useState(false);
  const [previewError, setPreviewError] = useState<string | null>(null);
  const [checkCount, setCheckCount] = useState(0);
  const activePreviewId = useRef<string | null>(null);
  const previewSequence = useRef(0);

  const checkCodex = useCallback(() => {
    let active = true;
    async function load() {
      try {
        const status = await shareApi.codexStatus();
        const threads = status.state === "available" ? await shareApi.listThreads() : [];
        if (active) {
          setPageState(toPageState(status, threads));
        }
      } catch (error) {
        if (active) {
          setPageState({ kind: "error", message: describeError(error) });
        }
      }
    }
    void load();
    return () => {
      active = false;
    };
  }, []);

  useEffect(() => checkCodex(), [checkCodex, checkCount]);

  const threads = useMemo(
    () => (pageState.kind === "available" ? pageState.threads : []),
    [pageState],
  );

  useEffect(() => {
    if (threads.length > 0 && !selectedThreadId) {
      setSelectedThreadId(threads[0].id);
    }
  }, [threads, selectedThreadId]);

  useEffect(() => {
    let active = true;
    async function loadTurns() {
      if (!selectedThreadId) {
        setTurns([]);
        return;
      }
      try {
        const result = await shareApi.listTurns(selectedThreadId);
        if (active) {
          setTurns(result);
        }
      } catch {
        if (active) {
          setTurns([]);
        }
      }
    }
    void loadTurns();
    return () => {
      active = false;
    };
  }, [selectedThreadId]);

  const invalidatePreview = useCallback((cancelBackend: boolean) => {
    const previewId = activePreviewId.current;
    activePreviewId.current = null;
    if (cancelBackend && previewId) {
      void shareApi.cancelPreview(previewId).catch(() => undefined);
    }
    setPreview(null);
    setPreviewLoading(false);
    setPreviewError(null);
  }, []);

  useEffect(() => () => {
    const previewId = activePreviewId.current;
    if (previewId) {
      void shareApi.cancelPreview(previewId).catch(() => undefined);
    }
  }, []);

  const handleSelectThread = useCallback((threadId: string) => {
    invalidatePreview(true);
    setSelectedThreadId(threadId);
    setSelectedTurnId(null);
    setTurns([]);
  }, [invalidatePreview]);

  const handleSelectTurn = useCallback((turnId: string | null) => {
    invalidatePreview(true);
    setSelectedTurnId(turnId);
  }, [invalidatePreview]);

  const previewReady = preview !== null
    && preview.threadId === selectedThreadId
    && preview.cutoffTurnId === selectedTurnId;

  async function generatePreview() {
    if (!selectedThreadId || previewLoading) {
      return;
    }
    invalidatePreview(true);
    const threadId = selectedThreadId;
    const cutoffTurnId = selectedTurnId;
    const previewId = `preview-${previewSequence.current += 1}`;
    activePreviewId.current = previewId;
    setPreviewLoading(true);
    try {
      const result = await shareApi.previewSnapshot(threadId, cutoffTurnId, previewId);
      if (activePreviewId.current !== previewId) {
        void shareApi.cancelPreview(previewId).catch(() => undefined);
        return;
      }
      setPreview({ id: previewId, threadId, cutoffTurnId, snapshot: result });
    } catch (error) {
      if (activePreviewId.current === previewId) {
        setPreviewError(describeError(error));
      }
    } finally {
      if (activePreviewId.current === previewId) {
        setPreviewLoading(false);
      }
    }
  }

  if (pageState.kind === "loading") {
    return <p className="notice" role="status">正在检查本机 Codex…</p>;
  }
  if (pageState.kind === "error") {
    return (
      <div className="notice error" role="alert">
        <p>Codex 不可用：{pageState.message}。请检查本机 Codex 安装后重试。</p>
        <div className="action-row">
          <button
            type="button"
            className="secondary-action"
            onClick={() => {
              setPageState({ kind: "loading" });
              setCheckCount((count) => count + 1);
            }}
          >
            重新检查 Codex
          </button>
        </div>
      </div>
    );
  }
  if (pageState.kind === "unsupported") {
    return (
      <div className="notice error" role="alert">
        <strong>当前 Codex 版本不兼容</strong>
        <p>{pageState.status.detail}</p>
        <p>请升级到 Codex 桌面版与 CLI 相互匹配的受支持版本。AgentBridge 不会读取桌面版内部数据库。</p>
        <div className="action-row">
          <button
            type="button"
            className="secondary-action"
            onClick={() => {
              setPageState({ kind: "loading" });
              setCheckCount((count) => count + 1);
            }}
          >
            重新检查 Codex
          </button>
        </div>
      </div>
    );
  }
  if (pageState.kind === "empty") {
    return (
      <div className="notice" role="status">
        <p>{pageState.status.detail || "本机没有可分享的 Codex 会话。"}</p>
        <div className="action-row">
          <button
            type="button"
            className="secondary-action"
            onClick={() => {
              setPageState({ kind: "loading" });
              setCheckCount((count) => count + 1);
            }}
          >
            重新检查 Codex
          </button>
        </div>
      </div>
    );
  }

  return (
    <div className="share-layout">
      <ThreadPicker
        threads={pageState.threads}
        turns={turns}
        selectedThreadId={selectedThreadId}
        selectedTurnId={selectedTurnId}
        onSelectThread={handleSelectThread}
        onSelectTurn={handleSelectTurn}
      />
      <div className="share-detail-column">
        <section className="panel preview-action-panel" aria-labelledby="preview-action-title">
          <div className="panel-heading">
            <h2 id="preview-action-title">分享预览</h2>
            <span>{previewReady ? "已生成" : "尚未生成"}</span>
          </div>
          <p className="hint">
            选择会话和截止轮次后生成预览；只有预览成功后才能创建局域网邀请。
          </p>
          <button
            type="button"
            className="primary-action"
            disabled={previewLoading || !selectedThreadId}
            onClick={() => void generatePreview()}
          >
            {previewLoading ? "正在生成预览…" : previewReady ? "重新生成预览" : "生成分享预览"}
          </button>
        </section>
        <ConversationPreview
          snapshot={previewReady ? preview.snapshot : null}
          loading={false}
          error={previewError}
        />
        <PairingPanel
          threadId={selectedThreadId}
          previewId={previewReady ? preview.id : null}
          previewReady={previewReady}
        />
      </div>
    </div>
  );
}
