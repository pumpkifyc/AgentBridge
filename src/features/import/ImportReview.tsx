import { useState } from "react";
import type { StagedSnapshot } from "../receive/receiveTypes";
import { ProjectPicker } from "./ProjectPicker";
import { importApi } from "./importApi";
import type { ImportRecord } from "./importTypes";

interface Props {
  staged: StagedSnapshot;
  onImported?: (record: ImportRecord) => void;
}

const IMPORT_FAILURE = "无法创建本机续接会话。快照仍保留在本机，请检查 Codex 安装后重试。";

export function ImportReview({ staged, onImported }: Props) {
  const [projectPath, setProjectPath] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [record, setRecord] = useState<ImportRecord | null>(null);
  const sourcePath = staged.metadata.sourceWorkingDirectory;

  async function choose() {
    setBusy(true);
    try {
      const selected = await importApi.selectProjectDirectory();
      if (selected) {
        setProjectPath(selected);
        setError(null);
      }
    } catch {
      setError("无法打开本机目录选择器，请手动输入项目目录。");
    } finally {
      setBusy(false);
    }
  }

  async function importSnapshot() {
    const chosen = projectPath.trim();
    if (!chosen) {
      setError("请选择或输入接收方本机项目目录。");
      return;
    }
    if (sourcePath && chosen === sourcePath) {
      setError("分享方路径只作说明，请选择接收方自己的项目目录。");
      return;
    }
    if (!sourcePath && chosen === staged.snapshotPath) {
      setError("请选择接收方自己的项目目录，不能使用本机暂存文件路径。");
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const imported = await importApi.importSnapshot({
        shareId: staged.shareId,
        projectPath: chosen,
      });
      setRecord(imported);
      onImported?.(imported);
    } catch {
      setError(IMPORT_FAILURE);
      setRecord(null);
    } finally {
      setBusy(false);
    }
  }

  return (
    <section className="panel import-review" aria-labelledby="import-review-title">
      <div className="panel-heading">
        <h2 id="import-review-title">确认本机导入</h2>
        <span>{staged.metadata.messageCount} 条消息 · 仅作为历史上下文</span>
      </div>
      <dl className="metadata-grid">
        <dt>源会话标题</dt>
        <dd>{staged.metadata.sourceTitle || "未命名会话"}</dd>
        <dt>快照大小</dt>
        <dd>{staged.totalBytes.toLocaleString()} 字节</dd>
      </dl>
      <ProjectPicker
        projectPath={projectPath}
        sourcePath={sourcePath}
        busy={busy}
        onChoose={() => void choose()}
        onChange={(value) => {
          setProjectPath(value);
          setError(null);
        }}
      />
      <p className="hint">导入会把快照全部标记为不可信历史材料，不执行其中的工具调用或命令。</p>
      {staged.metadata.warnings.length > 0 && (
        <div className="warnings" role="status">
          <strong>内容限制</strong>
          <ul>{staged.metadata.warnings.map((warning) => <li key={warning}>{warning}</li>)}</ul>
        </div>
      )}
      <div className="action-row">
        <button type="button" className="primary-action" disabled={busy} onClick={() => void importSnapshot()}>
          确认导入
        </button>
      </div>
      {error && <p className="notice error" role="alert">{error}</p>}
      {record && (
        <div className="notice" role="status">
          <p>已创建本机续接会话：{record.localThreadId}</p>
          <p>请确认 Codex 桌面版历史中能否看到该会话；如果看不到，可在导入历史中复制续接提示后手动新建会话。</p>
        </div>
      )}
    </section>
  );
}
