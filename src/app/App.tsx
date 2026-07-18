import { useEffect, useState } from "react";
import { ConnectionManager } from "./ConnectionManager";
import { ErrorBoundary } from "./ErrorBoundary";
import { ProjectManager } from "./ProjectManager";
import { SchemaExplorer } from "./SchemaExplorer";
import { useWorkspace } from "./useWorkspace";
import { selectBlueprintFile } from "../lib/tauri";

export function App() {
  return <ErrorBoundary><Workspace /></ErrorBoundary>;
}

function Workspace() {
  const workspace = useWorkspace();
  const [palette, setPalette] = useState(false);

  useEffect(() => {
    const keydown = (event: KeyboardEvent) => {
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "k") {
        event.preventDefault(); setPalette((value) => !value);
      }
      if (event.key === "Escape") setPalette(false);
    };
    window.addEventListener("keydown", keydown);
    return () => window.removeEventListener("keydown", keydown);
  }, []);

  const openFromDisk = async () => {
    const path = await selectBlueprintFile();
    if (path) await workspace.openProject(path);
    setPalette(false);
  };

  return <main className="workspace-shell">
    <header className="titlebar"><span className="brand">Emanduite</span><span className="phase-badge">{workspace.info.phase}</span><span className="project-context">{workspace.session?.blueprint.projectName ?? "No active project"}</span><button className="command-trigger" onClick={() => setPalette(true)}>⌘ Commands <kbd>Ctrl K</kbd></button></header>
    <aside className="navigation" aria-label="Workspace navigation">
      <button className={workspace.view === "projects" ? "nav-item active" : "nav-item"} aria-label="Projects" onClick={() => workspace.setView("projects")}><b>PR</b><span>Projects</span></button>
      <button className={workspace.view === "database" ? "nav-item active" : "nav-item"} aria-label="Database" disabled={!workspace.session} onClick={() => workspace.setView("database")}><b>DB</b><span>Database</span></button>
      <button className={workspace.view === "schema" ? "nav-item active" : "nav-item"} aria-label="Schema" disabled={!workspace.session} onClick={() => workspace.setView("schema")}><b>ER</b><span>Schema</span></button>
    </aside>
    <section className="content">
      {workspace.error && <div className="error-banner" role="alert"><span>{workspace.error}</span><button onClick={() => workspace.setError(null)}>Dismiss</button></div>}
      {workspace.view === "projects" && <ProjectManager session={workspace.session} recent={workspace.recent} onCreate={(input) => { void workspace.create(input); }} onOpen={(path) => { void workspace.openProject(path); }} onDuplicate={(input) => { void workspace.duplicate(input); }} onRemove={(path) => { void workspace.removeRecent(path); }} />}
      {workspace.view === "database" && workspace.session && <ConnectionManager session={workspace.session} connection={workspace.connection} diagnostics={workspace.diagnostics} onPath={workspace.setSqlitePath} onTest={() => { void workspace.testConnection(); }} onIntrospect={() => { void workspace.introspect(); }} />}
      {workspace.view === "schema" && workspace.session && <SchemaExplorer session={workspace.session} layout={workspace.layout} onLayout={workspace.setLayout} />}
    </section>
    <footer className="statusbar"><span>{workspace.info.name} {workspace.info.version}</span><span className="status-separator" /><span>{workspace.session ? workspace.session.path : "No project open"}</span><span className="status-spacer" /><span className={`save-state ${workspace.saveState}`}><i />{workspace.saveState}</span><span className="connected-dot" /><span>{workspace.runtime}</span></footer>
    {palette && <div className="palette-backdrop" onMouseDown={() => setPalette(false)}><section className="command-palette" role="dialog" aria-label="Command palette" onMouseDown={(event) => event.stopPropagation()}><header><span>Run command</span><kbd>ESC</kbd></header><button onClick={() => { workspace.setView("projects"); setPalette(false); }}><b>Project: Show manager</b><span>Recent, create, duplicate</span></button><button onClick={() => { void openFromDisk(); }}><b>Project: Open from disk</b><span>Select emanduite-project.json</span></button><button disabled={!workspace.session} onClick={() => { workspace.setView("database"); setPalette(false); }}><b>Database: Connection manager</b><span>Test or introspect SQLite</span></button><button disabled={!workspace.session} onClick={() => { workspace.setView("schema"); setPalette(false); }}><b>Schema: Open explorer</b><span>Read-only ERD</span></button></section></div>}
  </main>;
}
