export interface ShareMetadata {
  shareId: string;
  sourceTitle: string;
  projectName: string | null;
  repositoryId: string | null;
  sourceWorkingDirectory: string | null;
  messageCount: number;
  cutoffTurnId: string | null;
  warnings: string[];
  schemaVersion: number;
}

export interface ReceivedPreview {
  shareId: string;
  metadata: ShareMetadata;
  totalBytes: number;
  chunkCount: number;
}

export interface StagedSnapshot {
  shareId: string;
  endpoint: string;
  fingerprint: string;
  metadata: ShareMetadata;
  snapshotPath: string;
  totalBytes: number;
  chunkCount: number;
}

export interface TransferProgress {
  kind: string;
  transferredBytes: number;
  totalBytes: number;
}
