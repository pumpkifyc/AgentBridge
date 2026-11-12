export interface ImportRecord {
  id: string;
  sourceTitle: string;
  localThreadId: string;
  projectPath: string;
  importedAt: number;
  snapshotPath: string;
  status: "pending" | "completed" | "failed";
  error?: string | null;
  retryCount: number;
  lastRetriedAt?: number | null;
}

export interface ImportRequest {
  shareId: string;
  projectPath: string;
}
