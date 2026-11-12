import { useCallback, useEffect, useMemo, useState } from "react";
import { ConversationPreview } from "./ConversationPreview";
import { PairingPanel } from "./PairingPanel";
import { ThreadPicker } from "./ThreadPicker";
import { shareApi } from "./shareApi";
import type {
  CodexStatus,
  SharePageState,
  SnapshotV1,
  ThreadSummary,
  TurnSummary,
} from "./shareTypes";

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

function describeError(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

export function SharePage() {
  const [pageState, setPageState] = useState<SharePageState>({ kind: "loading" });
  const [selectedThreadId, setSelectedThreadId] = useState<string | null>(null);
  const [selectedTurnId, setSelectedTurnId] = useState<string | null>(null);
  const [turns, setTurns] = useState<TurnSummary[]>([]);
  const [snapshot, setSnapshot] = useState<SnapshotV1 | null>(null);
  const [previewLoading, setPreviewLoading] = useState(false);
  const [previewError, setPreviewError] = useState<string | null>(null);
  const [checkCount, setCheckCount] = useState(0);

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

  useEffect(() => {
    let active = true;
    async function loadPreview() {
      if (!selectedThreadId) {
        setSnapshot(null);
        return;
      }
      setPreviewLoading(true);
      setPreviewError(null);
      try {
        const result = await shareApi.previewSnapshot(selectedThreadId, selectedTurnId);
        if (active) {
          setSnapshot(result);
        }
      } catch (error) {
        if (active) {
          setSnapshot(null);
          setPreviewError(describeError(error));
        }
      } finally {
        if (active) {
          setPreviewLoading(false);
        }
      }
    }
    void loadPreview();
    return () => {
      active = false;
    };
  }, [selectedThreadId, selectedTurnId]);

  const handleSelectThread = useCallback((threadId: string) => {
    setSelectedThreadId(threadId);
    setSelectedTurnId(null);
    setTurns([]);
    setSnapshot(null);
  }, []);

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
        onSelectTurn={setSelectedTurnId}
      />
      <div className="share-detail-column">
        <ConversationPreview snapshot={snapshot} loading={previewLoading} error={previewError} />
        <PairingPanel
          threadId={selectedThreadId}
          previewReady={snapshot !== null && !previewLoading && !previewError}
        />
      </div>
    </div>
  );
}
