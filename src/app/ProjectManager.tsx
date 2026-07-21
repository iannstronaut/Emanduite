import { useState } from "react";
import type { ProjectSession, RecentProject } from "../contracts/workspace";
import { selectBlueprintFile, selectProjectDirectory, selectSqliteFile } from "../lib/tauri";

interface Props {
  session: ProjectSession | null;
  recent: RecentProject[];
  onCreate: (input: { directory: string; name: string; sqlitePath: string; superadminEmail: string; superadminPassword: string }) => void;
  onOpen: (path: string) => void;
  onDuplicate: (input: { sourcePath: string; targetDirectory: string; name: string }) => void;
  onRemove: (path: string) => void;
}

export function ProjectManager({ session, recent, onCreate, onOpen, onDuplicate, onRemove }: Props) {
  const [name, setName] = useState("");
  const [directory, setDirectory] = useState("");
  const [sqlitePath, setSqlitePath] = useState("");
  const [superadminEmail, setSuperadminEmail] = useState("superadmin@local");
  const [superadminPassword, setSuperadminPassword] = useState("");

  const browseDirectory = async () => { const value = await selectProjectDirectory(); if (value) setDirectory(value); };
  const browseSqlite = async () => { const value = await selectSqliteFile(); if (value) setSqlitePath(value); };
  const openFromDisk = async () => { const value = await selectBlueprintFile(); if (value) onOpen(value); };
  const duplicate = async () => {
    if (!session) return;
    const targetDirectory = await selectProjectDirectory();
    if (targetDirectory) onDuplicate({ sourcePath: session.path, targetDirectory, name: `${session.blueprint.projectName} Copy` });
  };

  return <div className="page project-page">
    <div className="page-heading"><div><span className="eyebrow">PROJECT MANAGER</span><h1>Local workspaces</h1><p>Create, reopen, or duplicate a versioned Emanduite project.</p></div><button className="secondary" onClick={openFromDisk}>Open project file</button></div>
    <div className="project-layout">
      <form className="panel create-panel" onSubmit={(event) => { event.preventDefault(); onCreate({ name, directory, sqlitePath, superadminEmail, superadminPassword }); }}>
        <div className="panel-title"><span>New project</span><small>SQLite-first</small></div>
        <label>Project name<input value={name} onChange={(event) => setName(event.target.value)} placeholder="Inventory Workspace" required /></label>
        <label>Project directory<div className="field-action"><input value={directory} readOnly placeholder="Select an empty directory" /><button type="button" onClick={browseDirectory}>Browse</button></div></label>
        <label>SQLite database<div className="field-action"><input value={sqlitePath} readOnly placeholder="Select .sqlite, .sqlite3, or .db" /><button type="button" onClick={browseSqlite}>Browse</button></div></label>
        <label>Superadmin email<input type="email" value={superadminEmail} onChange={(event) => setSuperadminEmail(event.target.value)} required /></label>
        <label>Superadmin password<input type="password" value={superadminPassword} onChange={(event) => setSuperadminPassword(event.target.value)} minLength={12} placeholder="At least 12 characters" required /></label>
        <button className="primary" disabled={!name.trim() || !directory || !sqlitePath || !superadminEmail.trim() || superadminPassword.length < 12}>Create project</button>
      </form>
      <section className="panel recent-panel">
        <div className="panel-title"><span>Recent projects</span><small>{recent.length}/12</small></div>
        {recent.length === 0 ? <div className="empty-state"><strong>No recent projects</strong><span>Create a project or open an existing `emanduite-project.json`.</span></div> : <div className="recent-list">{recent.map((item) => <article className={session?.path === item.path ? "recent-item active" : "recent-item"} key={item.path}>
          <button className="recent-main" onClick={() => onOpen(item.path)}><strong>{item.name}</strong><span>{item.path}</span><time>{new Date(item.lastOpenedAt).toLocaleString()}</time></button>
          <button className="icon-button" title="Remove recent reference" onClick={() => onRemove(item.path)}>×</button>
        </article>)}</div>}
        <div className="panel-actions"><button className="secondary" disabled={!session} onClick={duplicate}>Duplicate active project</button></div>
      </section>
    </div>
  </div>;
}
