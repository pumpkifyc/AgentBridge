import { invoke } from "@tauri-apps/api/core";
import type { CodexStatus, SnapshotV1, ThreadPage, ThreadSummary, TurnSummary } from "./shareTypes";

export const shareApi = {
  async codexStatus(): Promise<CodexStatus> {
    return invoke<CodexStatus>("codex_status");
  },

  async listThreads(): Promise<ThreadSummary[]> {
    const threads: ThreadSummary[] = [];
    let cursor: string | null = null;
    for (let page = 0; page < 50; page += 1) {
      const result: ThreadPage = await invoke<ThreadPage>("list_threads", { cursor });
      threads.push(...result.data);
      cursor = result.nextCursor;
      if (!cursor) {
        return threads;
      }
    }
    throw new Error("本机会话数量过多，请缩小列表范围后重试");
  },

  async listTurns(threadId: string): Promise<TurnSummary[]> {
    return invoke<TurnSummary[]>("list_turns", { threadId });
  },

  async previewSnapshot(threadId: string, cutoffTurnId: string | null): Promise<SnapshotV1> {
    return invoke<SnapshotV1>("preview_snapshot", { threadId, cutoffTurnId });
  },
};
