export interface ImportRecord {
  id: string;
  sourceTitle: string;
  sessionName: string;
  localThreadId: string;
  projectPath: string;
  importedAt: number;
  snapshotPath: string;
  status: "pending" | "completed" | "failed";
  error?: string | null;
  retryCount: number;
  lastRetriedAt?: number | null;
}

export type ImportConflictPolicy = "preserveLocal" | "replaceLocal";

export interface ImportRequest {
  shareId: string;
  projectPath: string;
  sessionName: string;
  conflictPolicy: ImportConflictPolicy;
}

export interface CodexProject {
  path: string;
  name: string;
  threadCount: number;
  updatedAt: number;
}

export interface ImportConflictReport {
  files: string[];
}
