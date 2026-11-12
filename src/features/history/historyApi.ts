import { invoke } from "@tauri-apps/api/core";
import type { ImportRecord } from "../import/importTypes";

export const historyApi = {
  list(): Promise<ImportRecord[]> {
    return invoke<ImportRecord[]>("list_imports");
  },

  retry(importId: string): Promise<ImportRecord> {
    return invoke<ImportRecord>("retry_import", { importId });
  },

  open(importId: string): Promise<void> {
    return invoke<void>("open_import", { importId });
  },

  copyPrompt(importId: string): Promise<string> {
    return invoke<string>("copy_continuation_prompt", { importId });
  },

  remove(importId: string): Promise<void> {
    return invoke<void>("delete_import", { importId });
  },
};
