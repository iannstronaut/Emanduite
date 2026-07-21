import { useEffect, useState } from "react";
import { Code, Data, Diagram, Edit, Folder, Global, LoginCurve, Magicpen, Monitor, Profile2User, SecuritySafe } from "iconsax-reactjs";
import { ConnectionManager } from "./ConnectionManager";
import { ErrorBoundary } from "./ErrorBoundary";
import { ProjectManager } from "./ProjectManager";
import { SchemaExplorer } from "./SchemaExplorer";
import { useWorkspace } from "./useWorkspace";
import { selectBlueprintFile } from "../lib/tauri";
import { SchemaEditor } from "./SchemaEditor";
import { AuthEditor, EntityEditor, GlobalEditor, PermissionEditor } from "./ConfigEditors";
import { ExtensionEditor } from "./ExtensionEditor";
import { WorkflowPanel } from "./WorkflowPanel";
import { GeneratorPanel } from "./GeneratorPanel";
import { AiDesigner } from "./AiDesigner";
import { AiSettings } from "./AiSettings";
import activeLogo from "../../assets/emanduite.svg";
import inactiveLogo from "../../assets/emanduite_inactive.svg";

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
    <img className="app-logo" src={workspace.session ? activeLogo : inactiveLogo} alt="Emanduite" />
    <header className="titlebar"><span className="brand">Emanduite</span><span className="phase-badge">{workspace.info.phase}</span><span className="project-context">{workspace.session?.blueprint.projectName ?? "No active project"}</span><button className="command-trigger" onClick={() => setPalette(true)}>⌘ Commands <kbd>Ctrl K</kbd></button></header>
    <aside className="navigation" aria-label="Workspace navigation">
      <button className={workspace.view === "projects" ? "nav-item active" : "nav-item"} aria-label="Projects" onClick={() => workspace.setView("projects")}><Folder size={18} variant={workspace.view === "projects" ? "Bold" : "Linear"} /><span>Projects</span></button>
      <button className={workspace.view === "database" ? "nav-item active" : "nav-item"} aria-label="Database" disabled={!workspace.session} onClick={() => workspace.setView("database")}><Data size={18} variant={workspace.view === "database" ? "Bold" : "Linear"} /><span>Database</span></button>
      <button className={workspace.view === "schema" ? "nav-item active" : "nav-item"} aria-label="Schema" disabled={!workspace.session} onClick={() => workspace.setView("schema")}><Diagram size={18} variant={workspace.view === "schema" ? "Bold" : "Linear"} /><span>Schema</span></button>
      <button className={workspace.view === "editor" ? "nav-item active" : "nav-item"} aria-label="Schema editor" disabled={!workspace.session} onClick={() => workspace.setView("editor")}><Edit size={18} variant={workspace.view === "editor" ? "Bold" : "Linear"} /><span>Editor</span></button>
      <button className={workspace.view === "ai" ? "nav-item active" : "nav-item"} aria-label="AI database designer" disabled={!workspace.session} onClick={() => workspace.setView("ai")}><Magicpen size={18} variant={workspace.view === "ai" ? "Bold" : "Linear"} /><span>AI Design</span></button>
      <button className={workspace.view === "entities" ? "nav-item active" : "nav-item"} aria-label="Entities" disabled={!workspace.session} onClick={() => workspace.setView("entities")}><Profile2User size={18} variant={workspace.view === "entities" ? "Bold" : "Linear"} /><span>Entities</span></button>
      <button className={workspace.view === "permissions" ? "nav-item active" : "nav-item"} aria-label="Permissions" disabled={!workspace.session} onClick={() => workspace.setView("permissions")}><SecuritySafe size={18} variant={workspace.view === "permissions" ? "Bold" : "Linear"} /><span>Access</span></button>
      <button className={workspace.view === "auth" ? "nav-item active" : "nav-item"} aria-label="Authentication" disabled={!workspace.session} onClick={() => workspace.setView("auth")}><LoginCurve size={18} variant={workspace.view === "auth" ? "Bold" : "Linear"} /><span>Auth</span></button>
      <button className={workspace.view === "extensions" ? "nav-item active" : "nav-item"} aria-label="Extensions" disabled={!workspace.session} onClick={() => workspace.setView("extensions")}><Code size={18} variant={workspace.view === "extensions" ? "Bold" : "Linear"} /><span>Extend</span></button>
      <button className={workspace.view === "global" ? "nav-item active" : "nav-item"} aria-label="Global config" disabled={!workspace.session} onClick={() => workspace.setView("global")}><Global size={18} variant={workspace.view === "global" ? "Bold" : "Linear"} /><span>Global</span></button>
      <button className={workspace.view === "settings" ? "nav-item active" : "nav-item"} aria-label="AI provider settings" disabled={!workspace.session} onClick={() => workspace.setView("settings")}><Global size={18} variant={workspace.view === "settings" ? "Bold" : "Linear"} /><span>Settings</span></button>
      <button className={workspace.view === "workflow" ? "nav-item active" : "nav-item"} aria-label="Workflows and diagnostics" disabled={!workspace.session} onClick={() => workspace.setView("workflow")}><Monitor size={18} variant={workspace.view === "workflow" ? "Bold" : "Linear"} /><span>Operate</span></button>
      <button className={workspace.view === "generator" ? "nav-item active" : "nav-item"} aria-label="Next.js generator" disabled={!workspace.session} onClick={() => workspace.setView("generator")}><Magicpen size={18} variant={workspace.view === "generator" ? "Bold" : "Linear"} /><span>Generate</span></button>
    </aside>
    <section className="content">
      {workspace.error && <div className="error-banner" role="alert"><span>{workspace.error}</span><button onClick={() => workspace.setError(null)}>Dismiss</button></div>}
      {workspace.view === "projects" && <ProjectManager session={workspace.session} recent={workspace.recent} onCreate={(input) => { void workspace.create(input); }} onOpen={(path) => { void workspace.openProject(path); }} onDuplicate={(input) => { void workspace.duplicate(input); }} onRemove={(path) => { void workspace.removeRecent(path); }} />}
      {workspace.view === "database" && workspace.session && <ConnectionManager session={workspace.session} connection={workspace.connection} diagnostics={workspace.diagnostics} onPath={workspace.setSqlitePath} onTest={() => { void workspace.testConnection(); }} onIntrospect={() => { void workspace.introspect(); }} />}
      {workspace.view === "schema" && workspace.session && <SchemaExplorer session={workspace.session} layout={workspace.layout} onLayout={workspace.setLayout} />}
      {workspace.view === "editor" && workspace.session && <SchemaEditor session={workspace.session} onPlan={workspace.planSchema} onApply={workspace.applySchema} />}
      {workspace.view === "ai" && workspace.session && <AiDesigner key={`${workspace.session.path}-ai`} session={workspace.session} onPlan={workspace.planSchema} onApply={workspace.applySchema} onOpenSchema={() => workspace.setView("schema")} />}
      {workspace.view === "entities" && workspace.session && <EntityEditor key={`${workspace.session.path}-entities`} session={workspace.session} onCommit={workspace.commitBlueprint} />}
      {workspace.view === "permissions" && workspace.session && <PermissionEditor key={`${workspace.session.path}-permissions`} session={workspace.session} onCommit={workspace.commitBlueprint} />}
      {workspace.view === "auth" && workspace.session && <AuthEditor key={`${workspace.session.path}-auth`} session={workspace.session} onCommit={workspace.commitBlueprint} />}
      {workspace.view === "extensions" && workspace.session && <ExtensionEditor key={`${workspace.session.path}-extensions`} session={workspace.session} onCommit={workspace.commitBlueprint} />}
      {workspace.view === "global" && workspace.session && <GlobalEditor key={`${workspace.session.path}-global`} session={workspace.session} onCommit={workspace.commitBlueprint} />}
      {workspace.view === "settings" && workspace.session && <AiSettings key={`${workspace.session.path}-settings`} session={workspace.session} onCommit={workspace.commitBlueprint} />}
      {workspace.view === "workflow" && workspace.session && <WorkflowPanel key={`${workspace.session.path}-workflow`} session={workspace.session} onRecover={workspace.recover} />}
      {workspace.view === "generator" && workspace.session && <GeneratorPanel key={`${workspace.session.path}-generator`} session={workspace.session} onGenerated={workspace.refreshProject} />}
    </section>
    <footer className="statusbar"><span>{workspace.info.name} {workspace.info.version}</span><span className="status-separator" /><span>{workspace.session ? workspace.session.path : "No project open"}</span><span className="status-spacer" /><span className={`save-state ${workspace.saveState}`}><i />{workspace.saveState}</span><span className="connected-dot" /><span>{workspace.runtime}</span></footer>
      {palette && <div className="palette-backdrop" onMouseDown={() => setPalette(false)}><section className="command-palette" role="dialog" aria-label="Command palette" onMouseDown={(event) => event.stopPropagation()}><header><span>Run command</span><kbd>ESC</kbd></header><button onClick={() => { workspace.setView("projects"); setPalette(false); }}><b>Project: Show manager</b><span>Recent, create, duplicate</span></button><button onClick={() => { void openFromDisk(); }}><b>Project: Open from disk</b><span>Select emanduite-project.json</span></button><button disabled={!workspace.session} onClick={() => { workspace.setView("database"); setPalette(false); }}><b>Database: Connection manager</b><span>Test or introspect SQLite</span></button><button disabled={!workspace.session} onClick={() => { workspace.setView("schema"); setPalette(false); }}><b>Schema: Open explorer</b><span>Read-only ERD</span></button><button disabled={!workspace.session} onClick={() => { workspace.setView("workflow"); setPalette(false); }}><b>Operate: Workflows and diagnostics</b><span>Run, monitor, recover, export support</span></button><button disabled={!workspace.session} onClick={() => { workspace.setView("generator"); setPalette(false); }}><b>Generate: Next.js application</b><span>Preview ownership and write deterministic output</span></button></section></div>}
  </main>;
}
