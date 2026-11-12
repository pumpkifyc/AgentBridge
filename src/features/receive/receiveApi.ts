import { Channel, invoke } from "@tauri-apps/api/core";
import type {
  ReceivedPreview,
  StagedSnapshot,
  TransferProgress,
} from "./receiveTypes";

export const receiveApi = {
  preview(pairingInfo: string): Promise<ReceivedPreview> {
    return invoke<ReceivedPreview>("preview_share", {
      request: { pairingInfo },
    });
  },

  decline(shareId: string): Promise<void> {
    return invoke<void>("decline_share", { shareId });
  },

  accept(
    shareId: string,
    onProgress: (event: TransferProgress) => void,
  ): Promise<StagedSnapshot> {
    const onEvent = new Channel<TransferProgress>();
    onEvent.onmessage = onProgress;
    return invoke<StagedSnapshot>("accept_share", { shareId, onEvent });
  },

  cancel(shareId: string): Promise<void> {
    return invoke<void>("cancel_transfer", { shareId });
  },

  retryPreview(pairingInfo: string, shareId: string): Promise<ReceivedPreview> {
    return invoke<ReceivedPreview>("retry_preview", {
      request: { pairingInfo, shareId },
    });
  },

  deleteStaged(shareId: string): Promise<void> {
    return invoke<void>("delete_staged_snapshot", { shareId });
  },
};
