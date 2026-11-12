export type CodexState = "checking" | "available" | "empty" | "unsupported" | "error";

export interface CodexStatus {
  state: CodexState;
  detail: string;
  version?: string;
}

export interface ThreadSummary {
  id: string;
  title: string | null;
  preview: string;
  cwd: string;
  createdAt: number;
  updatedAt: number;
  repositoryId: string | null;
}

export interface ThreadPage {
  data: ThreadSummary[];
  nextCursor: string | null;
}

export interface TurnSummary {
  id: string;
  completed: boolean;
  startedAt: number | null;
  completedAt: number | null;
}

export interface SnapshotMetadata {
  sourceThreadId: string;
  title: string;
  createdAt: number;
  updatedAt: number;
  cutoffTurnId: string | null;
  projectName: string | null;
  repositoryId: string | null;
  sourceWorkingDirectory: string | null;
}

export interface SnapshotMessage {
  role: "user" | "assistant";
  text: string;
  turnIndex: number;
  timestamp: number | null;
  historicalToolSummary: string | null;
}

export interface SnapshotV1 {
  schemaVersion: number;
  metadata: SnapshotMetadata;
  messages: SnapshotMessage[];
  warnings: string[];
}

export type SharePageState =
  | { kind: "loading" }
  | { kind: "available"; status: CodexStatus; threads: ThreadSummary[]; turns: TurnSummary[] }
  | { kind: "empty"; status: CodexStatus }
  | { kind: "unsupported"; status: CodexStatus }
  | { kind: "error"; message: string };
