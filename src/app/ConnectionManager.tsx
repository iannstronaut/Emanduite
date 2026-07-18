import type { ConnectionStatus, DatabaseDiagnostic, ProjectSession } from "../contracts/workspace";
import { selectSqliteFile } from "../lib/tauri";

interface Props {
  session: ProjectSession;
  connection: ConnectionStatus | null;
  diagnostics: DatabaseDiagnostic[];
  onPath: (path: string) => void;
  onTest: () => void;
  onIntrospect: () => void;
}

export function ConnectionManager({ session, connection, diagnostics, onPath, onTest, onIntrospect }: Props) {
  const config = session.blueprint.databases.main;
  const sqlitePath = config.connection.kind === "sqlite" ? config.connection.path : "";
  const browse = async () => { const value = await selectSqliteFile(); if (value) onPath(value); };
  return <div className="page database-page">
    <div className="page-heading"><div><span className="eyebrow">DATABASE CONNECTION</span><h1>{config.name}</h1><p>Test and introspect the main SQLite database through the Rust adapter.</p></div><span className={connection ? "connection-pill connected" : "connection-pill"}>{connection ? "Connected" : "Not tested"}</span></div>
    <div className="database-grid">
      <section className="panel connection-panel">
        <div className="panel-title"><span>SQLite source</span><small>main database</small></div>
        <label>Absolute file path<div className="field-action"><input value={sqlitePath} readOnly /><button onClick={browse}>Change</button></div></label>
        <div className="capability-row">{config.capabilities.map((item) => <span key={item}>{item}</span>)}</div>
        <div className="panel-actions"><button className="secondary" onClick={onTest}>Test connection</button><button className="primary" onClick={onIntrospect}>Introspect schema</button></div>
      </section>
      <section className="panel connection-detail">
        <div className="panel-title"><span>Adapter status</span><small>SQLx</small></div>
        {connection ? <dl><dt>Provider</dt><dd>{connection.provider}</dd><dt>SQLite version</dt><dd>{connection.sqliteVersion ?? "Unknown"}</dd><dt>Resolved path</dt><dd>{connection.databaseLabel}</dd></dl> : <div className="empty-state"><strong>Connection not tested</strong><span>A failed connection never changes the saved project state.</span></div>}
      </section>
    </div>
    {diagnostics.length > 0 && <section className="panel diagnostics"><div className="panel-title"><span>Provider diagnostics</span><small>{diagnostics.length}</small></div>{diagnostics.map((item, index) => <div className="diagnostic" key={`${item.code}-${item.object}-${index}`}><code>{item.code}</code><strong>{item.object}</strong><span>{item.message}</span></div>)}</section>}
  </div>;
}
