interface Props {
  projectPath: string;
  sourcePath: string | null;
  busy: boolean;
  onChoose: () => void;
  onChange: (value: string) => void;
}

export function ProjectPicker({
  projectPath,
  sourcePath,
  busy,
  onChoose,
  onChange,
}: Props) {
  return (
    <div className="project-picker">
      <label className="field-label" htmlFor="project-directory">本机项目目录</label>
      <div className="action-row">
        <input
          id="project-directory"
          aria-label="本机项目目录"
          value={projectPath}
          onChange={(event) => onChange(event.target.value)}
          placeholder="选择或输入接收方电脑上的项目目录"
        />
        <button type="button" className="secondary-action" disabled={busy} onClick={onChoose}>
          选择本机目录
        </button>
      </div>
      {sourcePath && (
        <p className="hint">
          分享方路径 {sourcePath} <strong>仅作说明，不能作为本机项目目录</strong>。
        </p>
      )}
    </div>
  );
}
