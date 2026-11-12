import { invoke } from "@tauri-apps/api/core";
import type { ImportRecord, ImportRequest } from "./importTypes";

export const importApi = {
  selectProjectDirectory(): Promise<string | null> {
    return invoke<string | null>("select_project_directory");
  },

  importSnapshot(request: ImportRequest): Promise<ImportRecord> {
    return invoke<ImportRecord>("import_snapshot", { request });
  },

  listImports(): Promise<ImportRecord[]> {
    return invoke<ImportRecord[]>("list_imports");
  },

  deleteImport(importId: string): Promise<void> {
    return invoke<void>("delete_import", { importId });
  },
};
