import { useEffect, useState } from "react";
import type { AppInfo } from "../contracts/commands";
import { getAppInfo } from "../lib/tauri";

const fallbackInfo: AppInfo = { name: "Emanduite", version: "web-preview", phase: "Phase 1 · Desktop Foundation", blueprintSchemaVersion: 1, databaseProviders: ["sqlite"] };

export function App() {
  const [info, setInfo] = useState(fallbackInfo);
  const [runtime, setRuntime] = useState("Browser preview");
  useEffect(() => { getAppInfo().then((r) => { if (r.ok) { setInfo(r.data); setRuntime("Tauri runtime connected"); } }).catch(() => undefined); }, []);
  return <main className="workspace-shell">
    <header className="titlebar"><span className="brand">Emanduite</span><span className="phase-badge">{info.phase}</span></header>
    <aside className="navigation" aria-label="Workspace navigation"><button className="nav-item active" aria-label="Foundation status">EM</button><button className="nav-item" aria-label="Projects" disabled>PR</button><button className="nav-item" aria-label="Database" disabled>DB</button></aside>
    <section className="content"><div className="eyebrow">DESKTOP FOUNDATION</div><h1>Core contracts are ready for implementation.</h1><p className="lede">Phase 1 establishes the versioned blueprint, secure secret boundary, SQLite adapter, and typed Tauri command surface.</p><div className="status-grid"><article className="status-card"><span>Blueprint schema</span><strong>v{info.blueprintSchemaVersion}</strong></article><article className="status-card"><span>First provider</span><strong>{info.databaseProviders.join(", ")}</strong></article><article className="status-card"><span>Runtime</span><strong>{runtime}</strong></article></div></section>
    <footer className="statusbar"><span>{info.name} {info.version}</span><span className="connected-dot" /><span>{runtime}</span></footer>
  </main>;
}
