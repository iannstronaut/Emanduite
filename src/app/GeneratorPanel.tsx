import { useEffect, useState } from "react";
import type { CommandError, CommandResponse } from "../contracts/commands";
import type { GenerationPreview, GenerationResult } from "../contracts/generator";
import type { ProjectSession } from "../contracts/workspace";
import * as api from "../lib/tauri";

function unwrap<T>(response: CommandResponse<T>): T { if (!response.ok) throw response.error; return response.data; }
function message(error: unknown) { const command = error as Partial<CommandError>; return command.message ?? (error instanceof Error ? error.message : "Generation failed"); }

export function GeneratorPanel({ session, onGenerated }: { session: ProjectSession; onGenerated: () => Promise<unknown> }) {
  const [target, setTarget] = useState(session.blueprint.targetDirectory ?? "");
  const [preview, setPreview] = useState<GenerationPreview | null>(null);
  const [result, setResult] = useState<GenerationResult | null>(null);
  const [busy, setBusy] = useState<"preview" | "generate" | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => { setTarget(session.blueprint.targetDirectory ?? ""); setPreview(null); setResult(null); }, [session.path, session.blueprint.targetDirectory]);

  const browse = async () => { const directory = await api.selectProjectDirectory(); if (directory) { setTarget(directory); setPreview(null); setResult(null); } };
  const runPreview = async () => { setBusy("preview"); setError(null); setResult(null); try { setPreview(unwrap(await api.previewGeneration(session.path, target))); } catch (value) { setError(message(value)); } finally { setBusy(null); } };
  const generate = async () => { setBusy("generate"); setError(null); try { const next = unwrap(await api.generateProject(session.path, target)); setResult(next); await onGenerated(); } catch (value) { setError(message(value)); } finally { setBusy(null); } };

  return <div className="page generator-page">
    <div className="page-heading"><div><span className="eyebrow">NEXT.JS / PRISMA SQLITE</span><h1>Generate application</h1><p>Preview deterministic, ownership-aware files before writing the target workspace.</p></div></div>
    {error && <div className="inline-alert error" role="alert">{error}</div>}
    {result && <div className={result.conflicts.length ? "inline-alert error" : "inline-alert success"} role="status">{result.conflicts.length ? `${result.conflicts.length} conflict(s) preserved; review artifacts before building.` : `Generation complete: ${result.writtenFileCount} written, ${result.preservedFileCount} unchanged.`}</div>}
    <div className="generator-layout">
      <section className="panel generator-config"><div className="panel-title"><span>Generator input</span><small>Blueprint v1</small></div><label>Target directory<div className="field-action"><input aria-label="Target directory" value={target} onChange={(event) => { setTarget(event.target.value); setPreview(null); }} placeholder="Absolute empty or managed directory"/><button onClick={() => { void browse(); }}>Browse</button></div></label><dl><dt>Template</dt><dd>next-admin-v1@1.0.0</dd><dt>Provider</dt><dd>SQLite / Prisma 7</dd><dt>Entities</dt><dd>{Object.keys(session.blueprint.entities).length}</dd></dl><div className="panel-actions"><button className="secondary" disabled={!target || busy !== null} onClick={() => { void runPreview(); }}>Preview files</button><button className="primary" disabled={!preview || preview.targetDirectory !== target || busy !== null} onClick={() => { void generate(); }}>Generate</button></div></section>
      <section className="panel generator-preview"><div className="panel-title"><span>Ownership plan</span><small>{preview ? `${preview.files.length} files` : "not previewed"}</small></div>{preview ? <><div className="generation-summary"><span><b>{preview.generatedFileCount}</b> generated-owned</span><span><b>{preview.userFileCount}</b> user-owned</span><span><b>{preview.entityCount}</b> concrete CRUD routes</span></div><div className="generation-files">{preview.files.map((file) => <code key={file}>{file}</code>)}</div></> : <div className="empty-state"><strong>Choose a target and preview</strong><span>The backend validates Blueprint, SQLite provider, entity primary keys, controls, and path confinement first.</span></div>}</section>
      {result?.conflicts.length ? <section className="panel generator-conflicts"><div className="panel-title"><span>Conflicts</span><small>manual changes preserved</small></div>{result.conflicts.map((conflict) => <article key={conflict.path}><b>{conflict.path}</b><span>{conflict.reason}</span><code>{conflict.artifactPath}</code></article>)}</section> : null}
    </div>
  </div>;
}
