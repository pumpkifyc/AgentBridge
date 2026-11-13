import { invoke } from "@tauri-apps/api/core";
import type {
  CodexStatus,
  GeneratedPreview,
  ThreadPage,
  ThreadSummary,
  TurnSummary,
} from "./shareTypes";

export interface LocalAddress {
  ip: string;
  name: string;
}

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

  async generatePreview(
    threadId: string,
    cutoffTurnId: string | null,
    previewId: string,
  ): Promise<GeneratedPreview> {
    return invoke<GeneratedPreview>("generate_preview", {
      request: { threadId, cutoffTurnId, previewId },
    });
  },

  async cancelPreview(previewId: string): Promise<void> {
    return invoke<void>("cancel_preview", { previewId });
  },

  async listLocalAddresses(): Promise<LocalAddress[]> {
    return invoke<LocalAddress[]>("list_local_addresses");
  },
};
