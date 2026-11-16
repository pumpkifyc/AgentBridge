export type UpdateErrorCode =
  | "network"
  | "manifest"
  | "download"
  | "signature"
  | "install"
  | "restart"
  | "unknown";

export type UpdaterDownloadEvent =
  | { event: "Started"; data: { contentLength?: number } }
  | { event: "Progress"; data: { chunkLength: number } }
  | { event: "Finished" };

export interface UpdateHandle {
  download(onEvent: (event: UpdaterDownloadEvent) => void): Promise<void>;
  install(options: { restartAfterInstall: boolean }): Promise<void>;
  close(): Promise<void>;
}

export interface AvailableUpdate {
  currentVersion: string;
  version: string;
  date?: string;
  notes: string;
  handle: UpdateHandle;
}

export interface UpdateProgress {
  downloadedBytes: number;
  totalBytes?: number;
  percent?: number;
}

export interface ManualUpdater {
  currentVersion(): Promise<string>;
  check(): Promise<AvailableUpdate | null>;
  download(
    update: AvailableUpdate,
    onProgress: (progress: UpdateProgress) => void,
  ): Promise<void>;
  installAndRestart(update: AvailableUpdate): Promise<void>;
  discard(update: AvailableUpdate): Promise<void>;
}

export class UpdaterError extends Error {
  constructor(
    public readonly code: UpdateErrorCode,
    message: string,
  ) {
    super(message);
    this.name = "UpdaterError";
  }
}
