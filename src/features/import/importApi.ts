import { invoke } from "@tauri-apps/api/core";
import type {
  CodexProject,
  ImportConflictReport,
  ImportRecord,
  ImportRequest,
} from "./importTypes";

export const importApi = {
  listCodexProjects(): Promise<CodexProject[]> {
    return invoke<CodexProject[]>("list_codex_projects");
  },

  selectProjectDirectory(): Promise<string | null> {
    return invoke<string | null>("select_project_directory");
  },

  selectProjectParent(): Promise<string | null> {
    return invoke<string | null>("select_project_parent");
  },

  createProjectDirectory(parentPath: string, projectName: string): Promise<string> {
    return invoke<string>("create_project_directory", {
      request: { parentPath, projectName },
    });
  },

  inspectImportConflicts(shareId: string): Promise<ImportConflictReport> {
    return invoke<ImportConflictReport>("inspect_import_conflicts", { shareId });
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
