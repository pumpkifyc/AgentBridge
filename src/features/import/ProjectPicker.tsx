import { useEffect, useMemo, useState } from "react";
import { describeError } from "../share/shareErrors";
import { importApi } from "./importApi";
import type { CodexProject } from "./importTypes";

interface Props {
  projectPath: string;
  sourcePath: string | null;
  busy: boolean;
  onChange: (value: string) => void;
  onError: (message: string | null) => void;
}

export function ProjectPicker({
  projectPath,
  sourcePath,
  busy,
  onChange,
  onError,
}: Props) {
  const [mode, setMode] = useState<"existing" | "new" | "other">("existing");
  const [projects, setProjects] = useState<CodexProject[]>([]);
  const [projectsLoading, setProjectsLoading] = useState(true);
  const [search, setSearch] = useState("");
  const [parentPath, setParentPath] = useState("");
  const [projectName, setProjectName] = useState("");
  const [localBusy, setLocalBusy] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);

  useEffect(() => {
    let active = true;
    importApi
      .listCodexProjects()
      .then((result) => {
        if (active) {
          setProjects(result);
          setProjectsLoading(false);
        }
      })
      .catch((error) => {
        if (active) {
          setProjectsLoading(false);
          onError(`无法读取 Codex 已有项目：${describeError(error)}`);
        }
      });
    return () => {
      active = false;
    };
  }, [onError]);

  const filteredProjects = useMemo(() => {
    const needle = search.trim().toLocaleLowerCase();
    if (!needle) {
      return projects;
    }
    return projects.filter((project) =>
      `${project.name}\n${project.path}`.toLocaleLowerCase().includes(needle),
    );
  }, [projects, search]);

  const disabled = busy || localBusy;

  function switchMode(nextMode: "existing" | "new" | "other") {
    setMode(nextMode);
    setNotice(null);
    onError(null);
  }

  async function chooseOtherDirectory() {
    setLocalBusy(true);
    try {
      const selected = await importApi.selectProjectDirectory();
      if (selected) {
        onChange(selected);
        setNotice("已选择本机项目目录");
      }
    } catch (error) {
      onError(`无法打开本机目录选择器：${describeError(error)}`);
    } finally {
      setLocalBusy(false);
    }
  }

  async function chooseParentDirectory() {
    setLocalBusy(true);
    try {
      const selected = await importApi.selectProjectParent();
      if (selected) {
        setParentPath(selected);
      }
    } catch (error) {
      onError(`无法打开父目录选择器：${describeError(error)}`);
    } finally {
      setLocalBusy(false);
    }
  }

  async function createProject() {
    const parent = parentPath.trim();
    const name = projectName.trim();
    if (!parent) {
      onError("请先选择新项目的父目录。");
      return;
    }
    if (!name) {
      onError("请输入新项目名称。");
      return;
    }
    setLocalBusy(true);
    onError(null);
    try {
      const created = await importApi.createProjectDirectory(parent, name);
      onChange(created);
      setNotice(`已创建并选择项目：${name}`);
    } catch (error) {
      onError(`无法创建项目目录：${describeError(error)}`);
    } finally {
      setLocalBusy(false);
    }
  }

  return (
    <section className="project-picker" aria-labelledby="project-picker-title">
      <div className="panel-heading">
        <h3 id="project-picker-title">导入到本机项目</h3>
        <span>项目来自 Codex 历史目录</span>
      </div>
      <div className="segmented-control" role="tablist" aria-label="项目目录来源">
        <button
          type="button"
          role="tab"
          aria-selected={mode === "existing"}
          className={mode === "existing" ? "active" : ""}
          disabled={disabled}
          onClick={() => switchMode("existing")}
        >
          Codex 已有项目
        </button>
        <button
          type="button"
          role="tab"
          aria-selected={mode === "new"}
          className={mode === "new" ? "active" : ""}
          disabled={disabled}
          onClick={() => switchMode("new")}
        >
          新建项目
        </button>
        <button
          type="button"
          role="tab"
          aria-selected={mode === "other"}
          className={mode === "other" ? "active" : ""}
          disabled={disabled}
          onClick={() => switchMode("other")}
        >
          其他本机目录
        </button>
      </div>

      {mode === "existing" && (
        <div className="project-mode-panel">
          <label className="field-label" htmlFor="project-search">搜索项目</label>
          <input
            id="project-search"
            value={search}
            onChange={(event) => setSearch(event.target.value)}
            placeholder="按项目名称或路径筛选"
            disabled={disabled}
          />
          {projectsLoading ? (
            <p className="hint">正在读取 Codex 已有项目…</p>
          ) : filteredProjects.length === 0 ? (
            <p className="hint">没有匹配的 Codex 项目，可以新建项目或选择其他本机目录。</p>
          ) : (
            <div className="project-choice-list">
              {filteredProjects.map((project) => (
                <button
                  type="button"
                  key={project.path}
                  className={`project-choice ${projectPath === project.path ? "selected" : ""}`}
                  aria-pressed={projectPath === project.path}
                  disabled={disabled}
                  onClick={() => {
                    onChange(project.path);
                    setNotice(`已选择项目：${project.name}`);
                    onError(null);
                  }}
                >
                  <strong>{project.name}</strong>
                  <small>{project.path}</small>
                  <small>{project.threadCount} 个 Codex 会话</small>
                </button>
              ))}
            </div>
          )}
        </div>
      )}

      {mode === "new" && (
        <div className="project-mode-panel project-create-grid">
          <div>
            <label className="field-label" htmlFor="project-parent">父目录</label>
            <div className="action-row">
              <input
                id="project-parent"
                value={parentPath}
                onChange={(event) => setParentPath(event.target.value)}
                placeholder="选择父目录"
                disabled={disabled}
              />
              <button
                type="button"
                className="secondary-action"
                disabled={disabled}
                onClick={() => void chooseParentDirectory()}
              >
                选择父目录
              </button>
            </div>
          </div>
          <div>
            <label className="field-label" htmlFor="project-name">项目名称</label>
            <div className="action-row">
              <input
                id="project-name"
                value={projectName}
                onChange={(event) => setProjectName(event.target.value)}
                placeholder="例如 shared-agentbridge"
                maxLength={80}
                disabled={disabled}
              />
              <button
                type="button"
                className="primary-action"
                disabled={disabled}
                onClick={() => void createProject()}
              >
                创建项目
              </button>
            </div>
          </div>
        </div>
      )}

      {mode === "other" && (
        <div className="project-mode-panel">
          <p className="hint">选择没有出现在 Codex 历史中的其他本机项目目录。</p>
          <button
            type="button"
            className="secondary-action"
            disabled={disabled}
            onClick={() => void chooseOtherDirectory()}
          >
            选择本机目录
          </button>
        </div>
      )}

      <label className="field-label" htmlFor="project-directory">最终项目目录</label>
      <div className="action-row">
        <input
          id="project-directory"
          aria-label="本机项目目录"
          value={projectPath}
          onChange={(event) => onChange(event.target.value)}
          disabled={disabled}
          placeholder="从 Codex 项目中选择，或切换到其他目录"
        />
      </div>
      {notice && <p className="hint success-text" role="status">{notice}</p>}
      {sourcePath && (
        <p className="hint">
          分享方路径 {sourcePath} <strong>仅作说明，不能作为本机项目目录</strong>。
        </p>
      )}
    </section>
  );
}
