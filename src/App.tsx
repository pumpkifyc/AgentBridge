import { useState } from "react";
import "./App.css";
import { ReceivePage } from "./features/receive/ReceivePage";
import { HistoryPage } from "./features/history/HistoryPage";
import { SharePage } from "./features/share/SharePage";

type View = "home" | "share" | "receive" | "history";

function App() {
  const [view, setView] = useState<View>("home");

  return (
    <main className="app-shell">
      <header className="topbar">
        <div>
          <p className="eyebrow">局域网会话工作台</p>
          <h1>AgentBridge</h1>
        </div>
        <span className="connection-status">点对点 · 无云端中继</span>
      </header>

      <nav className="main-nav" aria-label="主导航">
        <button type="button" className={view === "home" ? "active" : ""} onClick={() => setView("home")}>首页</button>
        <button type="button" className={view === "share" ? "active" : ""} onClick={() => setView("share")}>分享会话</button>
        <button type="button" className={view === "receive" ? "active" : ""} onClick={() => setView("receive")}>接收会话</button>
        <button type="button" className={view === "history" ? "active" : ""} onClick={() => setView("history")}>导入历史</button>
      </nav>

      <div className="page-content">
        {view === "home" && (
          <section className="empty-state" aria-labelledby="empty-state-title">
            <div className="empty-icon" aria-hidden="true">↔</div>
            <p className="eyebrow">开始分享或接收</p>
            <h2 id="empty-state-title">还没有共享或导入的会话</h2>
            <p className="empty-copy">
              连接本机 Codex 后，你可以把一个会话安全地分享给局域网内的同事。
            </p>
            <div className="action-row">
              <button type="button" className="primary-action" onClick={() => setView("share")}>分享会话</button>
              <button type="button" className="secondary-action" onClick={() => setView("receive")}>接收会话</button>
            </div>
          </section>
        )}
        {view === "share" && <SharePage />}
        {view === "receive" && <ReceivePage />}
        {view === "history" && <HistoryPage />}
      </div>
    </main>
  );
}

export default App;
