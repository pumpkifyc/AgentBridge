import { useCallback, useState } from "react";
import type { StagedSnapshot } from "../receive/receiveTypes";
import { formatPreviewWarnings } from "../share/previewWarnings";
import { describeError } from "../share/shareErrors";
import { ProjectPicker } from "./ProjectPicker";
import { importApi } from "./importApi";
import type { ImportConflictPolicy, ImportRecord } from "./importTypes";

interface Props {
  staged: StagedSnapshot;
  onImported?: (record: ImportRecord) => void;
}

const IMPORT_FAILURE = "无法创建本机原生会话。会话包仍保留在本机，请检查 Codex 安装后重试。";

export function ImportReview({ staged, onImported }: Props) {
  const [projectPath, setProjectPath] = useState("");
  const [sessionName, setSessionName] = useState(
    staged.metadata.sourceTitle || "AgentBridge 导入会话",
  );
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [record, setRecord] = useState<ImportRecord | null>(null);
  const [conflictFiles, setConflictFiles] = useState<string[] | null>(null);
  const sourcePath = staged.metadata.sourceWorkingDirectory;
  const warnings = formatPreviewWarnings(staged.metadata.warnings);
  const setPickerError = useCallback((message: string | null) => setError(message), []);

  function validateInputs() {
    const chosen = projectPath.trim();
    const name = sessionName.trim();
    if (!chosen) {
      setError("请选择或输入接收方本机项目目录。");
      return false;
    }
    if (!name) {
      setError("请输入导入后的会话名称。");
      return false;
    }
    if (name.length > 120) {
      setError("会话名称最多允许 120 个字符。");
      return false;
    }
    if (sourcePath && chosen === sourcePath) {
      setError("分享方路径只作说明，请选择接收方自己的项目目录。");
      return false;
    }
    if (!sourcePath && chosen === staged.snapshotPath) {
      setError("请选择接收方自己的项目目录，不能使用本机暂存会话包路径。");
      return false;
    }
    return true;
  }

  async function performImport(conflictPolicy: ImportConflictPolicy) {
    if (!validateInputs()) {
      return;
    }
    const chosen = projectPath.trim();
    const name = sessionName.trim();
    setBusy(true);
    setError(null);
    setConflictFiles(null);
    try {
      const imported = await importApi.importSnapshot({
        shareId: staged.shareId,
        projectPath: chosen,
        sessionName: name,
        conflictPolicy,
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

  async function checkConflictsAndImport() {
    if (!validateInputs()) {
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const report = await importApi.inspectImportConflicts(staged.shareId);
      if (report.files.length > 0) {
        setConflictFiles(report.files);
        return;
      }
      await performImport("preserveLocal");
    } catch (inspectionError) {
      setError(`无法检查本机会话冲突。详细原因：${describeError(inspectionError)}`);
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
        <dt>导入后名称</dt>
        <dd>
          <input
            className="session-name-input"
            aria-label="导入会话名称"
            value={sessionName}
            maxLength={120}
            disabled={busy || record !== null}
            onChange={(event) => {
              setSessionName(event.target.value);
              setError(null);
            }}
          />
        </dd>
        <dt>会话包大小</dt>
        <dd>{staged.totalBytes.toLocaleString()} 字节</dd>
      </dl>
      <ProjectPicker
        projectPath={projectPath}
        sourcePath={sourcePath}
        busy={busy}
        onChange={(value) => {
          setProjectPath(value);
          setError(null);
        }}
        onError={setPickerError}
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
        <button
          type="button"
          className="primary-action"
          disabled={busy || record !== null}
          onClick={() => void checkConflictsAndImport()}
        >
          确认导入
        </button>
      </div>
      {error && <p className="notice error" role="alert">{error}</p>}
      {conflictFiles && (
        <div className="notice conflict-dialog" role="dialog" aria-label="本机已有冲突会话">
          <h3>本机已有相同的 Codex 会话文件</h3>
          <p>
            分享包中有 {conflictFiles.length} 个会话文件与本机内容不同。请选择如何处理本机已有文件：
          </p>
          <ul className="conflict-file-list">
            {conflictFiles.slice(0, 5).map((file) => <li key={file}>{file}</li>)}
            {conflictFiles.length > 5 && <li>另有 {conflictFiles.length - 5} 个文件</li>}
          </ul>
          <div className="action-row conflict-actions">
            <button
              type="button"
              className="primary-action"
              disabled={busy}
              onClick={() => void performImport("preserveLocal")}
            >
              保留本机会话并新建
            </button>
            <button
              type="button"
              className="danger-action"
              disabled={busy}
              onClick={() => void performImport("replaceLocal")}
            >
              覆盖冲突会话
            </button>
            <button
              type="button"
              className="secondary-action"
              disabled={busy}
              onClick={() => setConflictFiles(null)}
            >
              取消
            </button>
          </div>
          <p className="hint">
            “保留本机会话并新建”会在创建独立会话后恢复本机原文件；“覆盖冲突会话”只会在独立会话创建成功后，永久用分享包文件替换本机冲突文件。
          </p>
        </div>
      )}
      {record && (
        <div className="notice" role="status">
          <p>已创建本机原生会话：{record.sessionName || record.sourceTitle}</p>
          <p>会话已写入本机 Codex 历史。若桌面版尚未显示，请重启 Codex 后再次查看。</p>
        </div>
      )}
    </section>
  );
}
