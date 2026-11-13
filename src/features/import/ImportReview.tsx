import { useState } from "react";
import type { StagedSnapshot } from "../receive/receiveTypes";
import { formatPreviewWarnings } from "../share/previewWarnings";
import { describeError } from "../share/shareErrors";
import { ProjectPicker } from "./ProjectPicker";
import { importApi } from "./importApi";
import type { ImportRecord } from "./importTypes";

interface Props {
  staged: StagedSnapshot;
  onImported?: (record: ImportRecord) => void;
}

const IMPORT_FAILURE = "无法创建本机原生会话。会话包仍保留在本机，请检查 Codex 安装后重试。";

export function ImportReview({ staged, onImported }: Props) {
  const [projectPath, setProjectPath] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [record, setRecord] = useState<ImportRecord | null>(null);
  const sourcePath = staged.metadata.sourceWorkingDirectory;
  const warnings = formatPreviewWarnings(staged.metadata.warnings);

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
      setError("请选择接收方自己的项目目录，不能使用本机暂存会话包路径。");
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
    } catch (importError) {
      setError(`${IMPORT_FAILURE} 详细原因：${describeError(importError)}`);
      setRecord(null);
    } finally {
      setBusy(false);
    }
  }

  return (
    <section className="panel import-review" aria-labelledby="import-review-title">
      <div className="panel-heading">
        <h2 id="import-review-title">确认本机导入</h2>
        <span>{staged.metadata.messageCount} 条消息 · 完整原生会话</span>
      </div>
      <dl className="metadata-grid">
        <dt>源会话标题</dt>
        <dd>{staged.metadata.sourceTitle || "未命名会话"}</dd>
        <dt>会话包大小</dt>
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
      <p className="hint">
        导入会把原始会话 rollout 复制到本机 Codex 数据目录，并通过公开接口创建独立会话分支。
        历史工具调用不会重新执行；原始会话可能包含工具输出、文件路径或其他敏感内容，请只导入可信来源。
      </p>
      {warnings.length > 0 && (
        <div className="warnings" role="status">
          <strong>预览显示说明</strong>
          <ul>{warnings.map((warning) => <li key={warning}>{warning}</li>)}</ul>
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
          <p>已创建本机原生会话：{record.localThreadId}</p>
          <p>会话已写入本机 Codex 历史。若桌面版尚未显示，请重启 Codex 后再次查看。</p>
        </div>
      )}
    </section>
  );
}
