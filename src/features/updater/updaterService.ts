import { getVersion } from "@tauri-apps/api/app";
import {
  check,
  type Update,
} from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import {
  UpdaterError,
  type AvailableUpdate,
  type ManualUpdater,
  type UpdateErrorCode,
  type UpdaterDownloadEvent,
  type UpdateProgress,
} from "./updateTypes";

function errorText(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function isSignatureError(error: unknown): boolean {
  const message = errorText(error).toLowerCase();
  return (
    message.includes("signature") ||
    message.includes("invalid signature") ||
    message.includes("签名")
  );
}

function isNetworkError(error: unknown): boolean {
  const message = errorText(error).toLowerCase();
  return [
    "network",
    "connect",
    "connection",
    "timeout",
    "timed out",
    "dns",
    "request failed",
    "status code",
  ].some((part) => message.includes(part));
}

function wrapError(
  code: UpdateErrorCode,
  error: unknown,
): UpdaterError {
  return error instanceof UpdaterError
    ? error
    : new UpdaterError(code, errorText(error));
}

function availableUpdate(update: Update): AvailableUpdate {
  return {
    currentVersion: update.currentVersion,
    version: update.version,
    date: update.date,
    notes: update.body ?? "",
    handle: update,
  };
}

export function createTauriUpdater(): ManualUpdater {
  return {
    async currentVersion() {
      try {
        return await getVersion();
      } catch (error) {
        throw wrapError("unknown", error);
      }
    },

    async check() {
      try {
        const update = await check();
        return update ? availableUpdate(update) : null;
      } catch (error) {
        throw wrapError(
          isNetworkError(error) ? "network" : "manifest",
          error,
        );
      }
    },

    async download(
      update: AvailableUpdate,
      onProgress: (progress: UpdateProgress) => void,
    ) {
      let downloadedBytes = 0;
      let totalBytes: number | undefined;

      function emit() {
        onProgress({
          downloadedBytes,
          ...(totalBytes === undefined ? {} : { totalBytes }),
          ...(totalBytes === undefined || totalBytes === 0
            ? {}
            : {
                percent: Math.min(
                  100,
                  Math.round((downloadedBytes / totalBytes) * 100),
                ),
              }),
        });
      }

      try {
        await update.handle.download((event: UpdaterDownloadEvent) => {
          if (event.event === "Started") {
            totalBytes = event.data.contentLength;
            downloadedBytes = 0;
            emit();
            return;
          }
          if (event.event === "Progress") {
            downloadedBytes += event.data.chunkLength;
            emit();
          }
        });
      } catch (error) {
        throw wrapError(isSignatureError(error) ? "signature" : "download", error);
      }
    },

    async installAndRestart(update: AvailableUpdate) {
      try {
        await update.handle.install({ restartAfterInstall: true });
      } catch (error) {
        throw wrapError(isSignatureError(error) ? "signature" : "install", error);
      }

      try {
        await relaunch();
      } catch (error) {
        throw wrapError("restart", error);
      }
    },

    async discard(update: AvailableUpdate) {
      await update.handle.close().catch(() => {});
    },
  };
}
