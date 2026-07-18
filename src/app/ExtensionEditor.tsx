import { useEffect, useState } from "react";
import type { BlueprintV1, ExtensionConfig } from "../contracts/blueprint";
import type { ExtensionDocument } from "../contracts/schema-editor";
import type { ProjectSession } from "../contracts/workspace";
import * as api from "../lib/tauri";
import { assertCommandData } from "../contracts/commands";

const uuid = () => crypto.randomUUID();

export function ExtensionEditor({ session, onCommit }: { session: ProjectSession; onCommit: (update: (blueprint: BlueprintV1) => BlueprintV1) => void }) {
  const [manifest, setManifest] = useState<Record<string, ExtensionConfig>>(() => structuredClone(session.blueprint.extensions));
  const [selected, setSelected] = useState(Object.keys(manifest)[0] ?? "");
  const [newPath, setNewPath] = useState("");
  const [language, setLanguage] = useState("typescript");
  const [document, setDocument] = useState<ExtensionDocument | null>(null);
  const [savedContent, setSavedContent] = useState("");
  const [busy, setBusy] = useState(false);
  const config = manifest[selected];
  const dirty = document?.content !== savedContent || JSON.stringify(manifest) !== JSON.stringify(session.blueprint.extensions);

  useEffect(() => {
    if (!config) { setDocument(null); setSavedContent(""); return; }
    let live = true;
    void api.loadExtensionFile(session.path, config.path, config.language).then(assertCommandData).then((value) => { if (live) { setDocument(value); setSavedContent(value.content); } }).catch(() => undefined);
    return () => { live = false; };
  }, [config?.id, session.path]);

  const add = () => {
    const path = newPath.trim(); if (!path) return;
    const key = path; const item: ExtensionConfig = { id: uuid(), path, language, ownership: "userOwned" };
    setManifest((value) => ({ ...value, [key]: item })); setSelected(key); setNewPath("");
  };
  const validate = async () => { if (!document || !config) return; setBusy(true); const value = await api.validateExtensionFile(config.path, config.language, document.content).then(assertCommandData).catch(() => null); if (value) setDocument(value); setBusy(false); };
  const save = async (format: boolean) => {
    if (!document || !config) return; setBusy(true);
    const value = await api.saveExtensionFile(session.path, config.path, config.language, document.content, format).then(assertCommandData).catch(() => null);
    if (value) { setDocument(value); if (value.valid) { setSavedContent(value.content); onCommit((blueprint) => ({ ...blueprint, extensions: manifest })); } }
    setBusy(false);
  };
  return <div className="page extension-page"><div className="page-heading"><div><span className="eyebrow">EXTENSION EDITOR</span><h1>User-owned files</h1><p>Files live under the project extension root and are never generator-owned.</p></div><div className="editor-actions"><span className={dirty ? "draft-indicator dirty" : "draft-indicator"}>{dirty ? "Dirty" : "Saved"}</span><button className="secondary" disabled={!document || busy} onClick={() => { void validate(); }}>Validate</button><button className="secondary" disabled={!document || busy} onClick={() => { void save(true); }}>Format</button><button className="primary" disabled={!document || busy} onClick={() => { void save(false); }}>Save file</button></div></div>
    <section className="panel extension-workspace"><aside><div className="extension-add"><input value={newPath} onChange={(event) => setNewPath(event.target.value)} placeholder="hooks/example.ts" /><select value={language} onChange={(event) => setLanguage(event.target.value)}><option value="typescript">TypeScript</option><option value="javascript">JavaScript</option><option value="json">JSON</option><option value="css">CSS</option></select><button onClick={add}>Add</button></div>{Object.entries(manifest).map(([key, item]) => <button className={selected === key ? "active" : ""} onClick={() => setSelected(key)} key={item.id}><b>{item.path}</b><span>{item.language} · user-owned</span></button>)}</aside><div className="code-editor">{document && config ? <><header><span>{config.path}</span><code>{config.ownership}</code></header><textarea value={document.content} onChange={(event) => setDocument({ ...document, content: event.target.value })} spellCheck={false} /><footer className={document.valid ? "valid" : "invalid"}>{document.valid ? "Syntax boundary valid" : document.diagnostics.join(" · ")}</footer></> : <div className="empty-state"><strong>No extension selected</strong><span>Add a JSON, TypeScript, JavaScript, or CSS file.</span></div>}</div></section>
  </div>;
}
