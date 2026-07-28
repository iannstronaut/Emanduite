import { useEffect, useState } from "react";
import { Code, Data, Diagram, Edit, Folder, Global, LoginCurve, Magicpen, Monitor, Moon, Profile2User, SecuritySafe, Sun1 } from "iconsax-reactjs";
import { ConnectionManager } from "./ConnectionManager";
import { ErrorBoundary } from "./ErrorBoundary";
import { ProjectManager } from "./ProjectManager";
import { SchemaExplorer } from "./SchemaExplorer";
import { useWorkspace } from "./useWorkspace";
import { finishAppStartup, selectBlueprintFile } from "../lib/tauri";
import { SchemaEditor } from "./SchemaEditor";
import { AuthEditor, EntityEditor, GlobalEditor, PermissionEditor, RoleEditor } from "./ConfigEditors";
import { ExtensionEditor } from "./ExtensionEditor";
import { WorkflowPanel } from "./WorkflowPanel";
import { GeneratorPanel } from "./GeneratorPanel";
import { AiDesigner } from "./AiDesigner";
import { AiSettings } from "./AiSettings";
import { ModalProvider, useModal } from "./ModalProvider";
import activeLogo from "../../assets/emanduite.svg";
import inactiveLogo from "../../assets/emanduite_inactive.svg";

export function App() {
  return <ModalProvider><ErrorBoundary><Workspace /></ErrorBoundary></ModalProvider>;
}

function Workspace() {
  const workspace = useWorkspace();
  const { inform } = useModal();
  const [palette, setPalette] = useState(false);
  const [theme, setTheme] = useState<"light" | "dark">(() => localStorage.getItem("emanduite-theme") === "dark" ? "dark" : "light");

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

  useEffect(() => {
    document.documentElement.dataset.theme = theme;
    localStorage.setItem("emanduite-theme", theme);
  }, [theme]);

  useEffect(() => {
    if ("__TAURI_INTERNALS__" in window) void finishAppStartup().catch(() => undefined);
  }, []);

  useEffect(() => {
    if (!workspace.error) return;
    void inform({ title: "Operation needs attention", description: workspace.error, tone: "error" });
    workspace.setError(null);
  }, [inform, workspace.error, workspace.setError]);

  const openFromDisk = async () => {
    const path = await selectBlueprintFile();
    if (path) await workspace.openProject(path);
    setPalette(false);
  };

  const navigationGroup = workspace.view === "projects" ? "project" : workspace.view === "generator" ? "generate" : ["database", "schema", "editor", "ai"].includes(workspace.view) ? "connection" : ["entities", "roles", "permissions", "auth"].includes(workspace.view) ? "access" : "settings";
  const hasSecondaryNavigation = navigationGroup === "connection" || navigationGroup === "access" || navigationGroup === "settings";
  const openGroup = (group: "project" | "connection" | "access" | "settings" | "generate") => {
    if (group === "project") workspace.setView("projects");
    if (group === "connection") workspace.setView("database");
    if (group === "access") workspace.setView("entities");
    if (group === "settings") workspace.setView("global");
    if (group === "generate") workspace.setView("generator");
  };

  return <main className={`workspace-shell${hasSecondaryNavigation ? " has-secondary-navigation" : ""}`}>
    <img className="app-logo" src={workspace.session ? activeLogo : inactiveLogo} alt="Emanduite" />
    <header className="titlebar"><span className="brand">Emanduite</span><span className="phase-badge">{workspace.info.phase}</span><span className="project-context">{workspace.session?.blueprint.projectName ?? "No active project"}</span><button className="theme-toggle" type="button" title={`Switch to ${theme === "light" ? "dark" : "light"} mode`} aria-label={`Switch to ${theme === "light" ? "dark" : "light"} mode`} onClick={() => setTheme((value) => value === "light" ? "dark" : "light")}>{theme === "light" ? <Moon size={16} variant="Linear" /> : <Sun1 size={16} variant="Linear" />}<span>{theme === "light" ? "Dark" : "Light"}</span></button><button className="command-trigger" onClick={() => setPalette(true)}>⌘ Commands <kbd>Ctrl K</kbd></button></header>
    <aside className="primary-navigation" aria-label="Primary navigation">
      <button className={navigationGroup === "project" ? "primary-nav-item active" : "primary-nav-item"} onClick={() => openGroup("project")}><Folder size={19} variant={navigationGroup === "project" ? "Bold" : "Linear"} /><span>Project</span></button>
      <button className={navigationGroup === "connection" ? "primary-nav-item active" : "primary-nav-item"} disabled={!workspace.session} onClick={() => openGroup("connection")}><Data size={19} variant={navigationGroup === "connection" ? "Bold" : "Linear"} /><span>Connection</span></button>
      <button className={navigationGroup === "access" ? "primary-nav-item active" : "primary-nav-item"} disabled={!workspace.session} onClick={() => openGroup("access")}><SecuritySafe size={19} variant={navigationGroup === "access" ? "Bold" : "Linear"} /><span>Access</span></button>
      <button className={navigationGroup === "settings" ? "primary-nav-item active" : "primary-nav-item"} disabled={!workspace.session} onClick={() => openGroup("settings")}><Global size={19} variant={navigationGroup === "settings" ? "Bold" : "Linear"} /><span>Settings</span></button>
      <button className={navigationGroup === "generate" ? "primary-nav-item active" : "primary-nav-item"} disabled={!workspace.session} onClick={() => openGroup("generate")}><Magicpen size={19} variant={navigationGroup === "generate" ? "Bold" : "Linear"} /><span>Generate</span></button>
    </aside>
    {hasSecondaryNavigation && <aside className="navigation secondary-navigation" aria-label={`${navigationGroup} navigation`}>
      <div className="secondary-navigation-heading"><span>{navigationGroup === "connection" ? "Connection" : navigationGroup === "access" ? "Access" : "Settings"}</span><small>{navigationGroup === "connection" ? "DATABASE WORKSPACE" : navigationGroup === "access" ? "IDENTITY & ROLES" : "PROJECT PREFERENCES"}</small></div>
      {navigationGroup === "connection" && <><button className={workspace.view === "database" ? "nav-item active" : "nav-item"} onClick={() => workspace.setView("database")}><Data size={18} variant={workspace.view === "database" ? "Bold" : "Linear"} /><span>Database</span></button><button className={workspace.view === "schema" ? "nav-item active" : "nav-item"} onClick={() => workspace.setView("schema")}><Diagram size={18} variant={workspace.view === "schema" ? "Bold" : "Linear"} /><span>Schema</span></button><button className={workspace.view === "editor" ? "nav-item active" : "nav-item"} onClick={() => workspace.setView("editor")}><Edit size={18} variant={workspace.view === "editor" ? "Bold" : "Linear"} /><span>Editor</span></button><button className={workspace.view === "ai" ? "nav-item active" : "nav-item"} onClick={() => workspace.setView("ai")}><Magicpen size={18} variant={workspace.view === "ai" ? "Bold" : "Linear"} /><span>AI Design</span></button></>}
      {navigationGroup === "access" && <><button className={workspace.view === "entities" ? "nav-item active" : "nav-item"} onClick={() => workspace.setView("entities")}><Profile2User size={18} variant={workspace.view === "entities" ? "Bold" : "Linear"} /><span>Entities</span></button><button className={workspace.view === "permissions" ? "nav-item active" : "nav-item"} onClick={() => workspace.setView("permissions")}><SecuritySafe size={18} variant={workspace.view === "permissions" ? "Bold" : "Linear"} /><span>Authentication</span></button><button className={workspace.view === "roles" ? "nav-item active" : "nav-item"} onClick={() => workspace.setView("roles")}><Profile2User size={18} variant={workspace.view === "roles" ? "Bold" : "Linear"} /><span>Roles</span></button><button className={workspace.view === "auth" ? "nav-item active" : "nav-item"} onClick={() => workspace.setView("auth")}><LoginCurve size={18} variant={workspace.view === "auth" ? "Bold" : "Linear"} /><span>Login setup</span></button></>}
      {navigationGroup === "settings" && <><button className={workspace.view === "global" ? "nav-item active" : "nav-item"} onClick={() => workspace.setView("global")}><Global size={18} variant={workspace.view === "global" ? "Bold" : "Linear"} /><span>Global</span></button><button className={workspace.view === "settings" ? "nav-item active" : "nav-item"} onClick={() => workspace.setView("settings")}><Global size={18} variant={workspace.view === "settings" ? "Bold" : "Linear"} /><span>AI Settings</span></button><button className={workspace.view === "workflow" ? "nav-item active" : "nav-item"} onClick={() => workspace.setView("workflow")}><Monitor size={18} variant={workspace.view === "workflow" ? "Bold" : "Linear"} /><span>Operate</span></button><button className={workspace.view === "extensions" ? "nav-item active" : "nav-item"} onClick={() => workspace.setView("extensions")}><Code size={18} variant={workspace.view === "extensions" ? "Bold" : "Linear"} /><span>Extensions</span></button></>}
    </aside>}
    <section className="content">
      {workspace.view === "projects" && <ProjectManager session={workspace.session} recent={workspace.recent} onCreate={(input) => { void workspace.create(input); }} onOpen={(path) => { void workspace.openProject(path); }} onDuplicate={(input) => { void workspace.duplicate(input); }} onRemove={(path) => { void workspace.removeRecent(path); }} />}
      {workspace.view === "database" && workspace.session && <ConnectionManager session={workspace.session} connection={workspace.connection} diagnostics={workspace.diagnostics} onPath={workspace.setSqlitePath} onTest={() => { void workspace.testConnection(); }} onIntrospect={() => { void workspace.introspect(); }} />}
      {workspace.view === "schema" && workspace.session && <SchemaExplorer session={workspace.session} layout={workspace.layout} onLayout={workspace.setLayout} />}
      {workspace.view === "editor" && workspace.session && <SchemaEditor session={workspace.session} onPlan={workspace.planSchema} onApply={workspace.applySchema} />}
      {workspace.view === "ai" && workspace.session && <AiDesigner key={`${workspace.session.path}-ai`} session={workspace.session} onPlan={workspace.planSchema} onApply={workspace.applySchema} onOpenSchema={() => workspace.setView("schema")} />}
      {workspace.view === "entities" && workspace.session && <EntityEditor key={`${workspace.session.path}-entities`} session={workspace.session} onCommit={workspace.commitBlueprint} />}
      {workspace.view === "roles" && workspace.session && <RoleEditor key={`${workspace.session.path}-roles`} session={workspace.session} onCommit={workspace.commitBlueprint} />}
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
