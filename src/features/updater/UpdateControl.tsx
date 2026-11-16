import { useEffect, useMemo, useRef, useState } from "react";
import { formatUpdateError } from "./updateErrors";
import { createTauriUpdater } from "./updaterService";
import { UpdateDialog } from "./UpdateDialog";
import {
  UpdaterError,
  type AvailableUpdate,
  type ManualUpdater,
  type UpdateProgress,
  type UpdateUiState,
} from "./updateTypes";

interface Props {
  updater?: ManualUpdater;
}

function isClosable(state: UpdateUiState): boolean {
  return (
    state.kind === "current" ||
    state.kind === "available" ||
    state.kind === "confirm" ||
    state.kind === "error"
  );
}

export function UpdateControl({ updater: providedUpdater }: Props) {
  const updater = useMemo(
    () => providedUpdater ?? createTauriUpdater(),
    [providedUpdater],
  );
  const [currentVersion, setCurrentVersion] = useState<string | null>(null);
  const [state, setState] = useState<UpdateUiState>({
    kind: "idle",
    currentVersion: "",
  });
  const activeUpdate = useRef<AvailableUpdate | null>(null);
  const activeOperation = useRef<"check" | "download" | null>(null);
  const mounted = useRef(true);

  useEffect(() => {
    mounted.current = true;
    updater
      .currentVersion()
      .then((version) => {
        if (!mounted.current) {
          return;
        }
        setCurrentVersion(version);
        setState((previous) =>
          previous.kind === "idle"
            ? { kind: "idle", currentVersion: version }
            : previous,
        );
      })
      .catch(() => {
        if (mounted.current) {
          setCurrentVersion("未知");
        }
      });

    return () => {
      mounted.current = false;
      const update = activeUpdate.current;
      activeUpdate.current = null;
      if (update) {
        void updater.discard(update);
      }
    };
  }, [updater]);

  function closeDialog() {
    if (!isClosable(state)) {
      return;
    }
    const update = activeUpdate.current;
    activeUpdate.current = null;
    setState({
      kind: "idle",
      currentVersion: currentVersion ?? "",
    });
    if (update) {
      void updater.discard(update);
    }
  }

  useEffect(() => {
    if (!isClosable(state)) {
      return;
    }
    function handleKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape") {
        closeDialog();
      }
    }
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  });

  async function startCheck() {
    if (activeOperation.current) {
      return;
    }
    activeOperation.current = "check";
    const version = currentVersion ?? "未知";
    setState({ kind: "checking", currentVersion: version });

    try {
      const update = await updater.check();
      if (!mounted.current) {
        if (update) {
          void updater.discard(update);
        }
        return;
      }
      activeUpdate.current = update;
      setState(
        update
          ? { kind: "available", update }
          : {
              kind: "current",
              currentVersion: version,
              checkedAt: new Date(),
            },
      );
    } catch (error) {
      if (mounted.current) {
        setState({
          kind: "error",
          message: formatUpdateError(error),
          retry: "check",
        });
      }
    } finally {
      activeOperation.current = null;
    }
  }

  async function startDownload() {
    if (activeOperation.current || !activeUpdate.current) {
      return;
    }
    const update = activeUpdate.current;
    activeOperation.current = "download";
    setState({
      kind: "downloading",
      update,
      progress: { downloadedBytes: 0 },
    });

    try {
      await updater.download(update, (progress: UpdateProgress) => {
        if (mounted.current) {
          setState((previous) =>
            previous.kind === "downloading"
              ? { ...previous, progress }
              : previous,
          );
        }
      });
      if (!mounted.current) {
        return;
      }
      setState({ kind: "installing", update });
      await updater.installAndRestart(update);
      if (mounted.current) {
        activeUpdate.current = null;
        setState({
          kind: "error",
          message: formatUpdateError(
            new UpdaterError("restart", "application did not restart"),
          ),
          retry: null,
        });
      }
    } catch (error) {
      if (mounted.current) {
        const retry =
          error instanceof UpdaterError && error.code === "restart"
            ? null
            : "download";
        if (retry === null) {
          activeUpdate.current = null;
        }
        setState({
          kind: "error",
          message: formatUpdateError(error),
          retry,
        });
      }
    } finally {
      activeOperation.current = null;
    }
  }

  const isBusy =
    state.kind === "checking" ||
    state.kind === "downloading" ||
    state.kind === "installing";

  return (
    <>
      <div className="update-control">
        <span className="update-version" aria-live="polite">
          {currentVersion === null
            ? "正在读取版本…"
            : currentVersion === "未知"
              ? "版本未知"
              : `当前版本 v${currentVersion}`}
        </span>
        <button
          type="button"
          className="update-check-button"
          disabled={isBusy}
          aria-busy={state.kind === "checking"}
          onClick={() => void startCheck()}
        >
          {state.kind === "checking" ? "正在检查…" : "检查更新"}
        </button>
      </div>

      {state.kind !== "idle" && (
        <UpdateDialog
          state={state}
          onClose={closeDialog}
          onDownloadRequest={() => {
            if (state.kind === "available") {
              setState({ kind: "confirm", update: state.update });
            }
          }}
          onConfirmDownload={() => void startDownload()}
          onRetryCheck={() => void startCheck()}
          onRetryDownload={() => void startDownload()}
        />
      )}
    </>
  );
}
