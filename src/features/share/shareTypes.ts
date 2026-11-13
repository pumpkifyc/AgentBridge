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

export type PreviewItemKind =
  | "user_message"
  | "assistant_message"
  | "plan"
  | "reasoning"
  | "command"
  | "file_change"
  | "tool_call"
  | "web_search"
  | "image"
  | "unknown"
  | "error";

export type PreviewBlockKind =
  | "text"
  | "code"
  | "diff"
  | "json"
  | "image"
  | "error";

export interface PreviewBlock {
  kind: PreviewBlockKind;
  label: string;
  content: string;
  sensitive: boolean;
  defaultExpanded: boolean;
}

export interface PreviewItem {
  id: string;
  kind: PreviewItemKind;
  title: string;
  status: string | null;
  timestamp: number | null;
  sensitive: boolean;
  blocks: PreviewBlock[];
}

export interface PreviewTurn {
  id: string;
  index: number;
  status: string;
  startedAt: number | null;
  completedAt: number | null;
  items: PreviewItem[];
}

export interface PreviewNotice {
  code: string;
  count: number;
  message: string;
  scope: "display_only";
}

export interface PreviewTimelineV1 {
  schemaVersion: number;
  sourceThreadId: string;
  cutoffTurnId: string | null;
  turns: PreviewTurn[];
  notices: PreviewNotice[];
}

export interface GeneratedPreview {
  snapshot: SnapshotV1;
  timeline: PreviewTimelineV1;
}

export type SharePageState =
  | { kind: "loading" }
  | { kind: "available"; status: CodexStatus; threads: ThreadSummary[]; turns: TurnSummary[] }
  | { kind: "empty"; status: CodexStatus }
  | { kind: "unsupported"; status: CodexStatus }
  | { kind: "error"; message: string };
