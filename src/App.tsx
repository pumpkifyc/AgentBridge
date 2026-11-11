import "./App.css";

function App() {
  return (
    <main className="app-shell">
      <header className="topbar">
        <div>
          <p className="eyebrow">局域网会话工作台</p>
          <h1>AgentBridge</h1>
        </div>
        <span className="connection-status">未连接 Codex</span>
      </header>

      <section className="empty-state" aria-labelledby="empty-state-title">
        <div className="empty-icon" aria-hidden="true">↔</div>
        <p className="eyebrow">开始分享或接收</p>
        <h2 id="empty-state-title">还没有共享或导入的会话</h2>
        <p className="empty-copy">
          连接本机 Codex 后，你可以把一个会话安全地分享给局域网内的同事。
        </p>
        <div className="action-row">
          <button type="button" className="primary-action">分享会话</button>
          <button type="button" className="secondary-action">接收会话</button>
        </div>
      </section>
    </main>
  );
}

export default App;
