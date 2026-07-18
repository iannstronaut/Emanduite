import { useEffect, useMemo, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import type { CommandError, CommandResponse } from "../contracts/commands";
import type { ProjectSession } from "../contracts/workspace";
import type { ProjectHealth, WorkflowDefinition, WorkflowOutputEvent, WorkflowTask, WorkflowTaskEvent } from "../contracts/workflow";
import * as api from "../lib/tauri";

function unwrap<T>(response: CommandResponse<T>): T {
  if (!response.ok) throw response.error;
  return response.data;
}

function message(error: unknown) {
  const command = error as Partial<CommandError>;
  return command.message ?? (error instanceof Error ? error.message : "Operation failed");
}

export function WorkflowPanel({ session, onRecover }: { session: ProjectSession; onRecover: () => Promise<unknown> }) {
  const [definitions, setDefinitions] = useState<WorkflowDefinition[]>([]);
  const [tasks, setTasks] = useState<WorkflowTask[]>([]);
  const [selectedTaskId, setSelectedTaskId] = useState("");
  const [health, setHealth] = useState<ProjectHealth | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const selected = useMemo(() => tasks.find((task) => task.id === selectedTaskId) ?? tasks[0], [tasks, selectedTaskId]);

  const refreshHealth = async () => {
    setBusy("diagnostics"); setError(null);
    try { setHealth(unwrap(await api.diagnoseProject(session.path))); }
    catch (value) { setError(message(value)); }
    finally { setBusy(null); }
  };

  useEffect(() => {
    let live = true;
    const unlisten: Array<() => void> = [];
    void Promise.all([api.listWorkflowDefinitions(), api.listWorkflowTasks(), api.diagnoseProject(session.path)])
      .then(([workflowResponse, taskResponse, healthResponse]) => {
        if (!live) return;
        setDefinitions(unwrap(workflowResponse));
        const next = unwrap(taskResponse); setTasks(next); setSelectedTaskId(next[0]?.id ?? "");
        setHealth(unwrap(healthResponse));
      })
      .catch((value) => live && setError(message(value)));
    void listen<WorkflowOutputEvent>("workflow://output", ({ payload }) => {
      if (!live) return;
      setTasks((current) => current.map((task) => task.id === payload.taskId ? { ...task, output: [...task.output, payload.output].slice(-500) } : task));
    }).then((dispose) => live ? unlisten.push(dispose) : dispose());
    void listen<WorkflowTaskEvent>("workflow://task", ({ payload }) => {
      if (!live) return;
      setTasks((current) => [payload.task, ...current.filter((task) => task.id !== payload.task.id)]);
    }).then((dispose) => live ? unlisten.push(dispose) : dispose());
    return () => { live = false; unlisten.forEach((dispose) => dispose()); };
  }, [session.path]);

  const start = async (workflow: WorkflowDefinition) => {
    setBusy(workflow.id); setError(null); setNotice(null);
    try {
      const task = unwrap(await api.startRegisteredWorkflow(session.path, workflow.id));
      setTasks((current) => [task, ...current]); setSelectedTaskId(task.id);
    } catch (value) { setError(message(value)); }
    finally { setBusy(null); }
  };

  const cancel = async (task: WorkflowTask) => {
    setBusy(task.id); setError(null);
    try { unwrap(await api.cancelRegisteredWorkflow(task.id)); }
    catch (value) { setError(message(value)); }
    finally { setBusy(null); }
  };

  const recover = async () => {
    if (!window.confirm("Restore the last-known-good Blueprint? The corrupt file will be archived.")) return;
    setBusy("recovery"); setError(null);
    try {
      const recovered = await onRecover();
      if (!recovered) throw new Error("Recovery did not complete");
      setNotice("Blueprint recovered from the last-known-good snapshot");
      await refreshHealth();
    }
    catch (value) { setError(message(value)); }
    finally { setBusy(null); }
  };

  const exportBundle = async () => {
    const directory = await api.selectSupportDirectory(); if (!directory) return;
    setBusy("support"); setError(null); setNotice(null);
    try { const path = unwrap(await api.exportSupportBundle(session.path, directory)); setNotice(`Redacted support bundle exported: ${path}`); }
    catch (value) { setError(message(value)); }
    finally { setBusy(null); }
  };

  return <div className="page workflow-page">
    <div className="page-heading"><div><span className="eyebrow">WORKFLOW RUNNER / DIAGNOSTICS</span><h1>Controlled task workspace</h1><p>Only registered argument-array workflows can run inside a verified project or target directory.</p></div><div className="editor-actions"><button className="secondary" disabled={busy === "diagnostics"} onClick={() => { void refreshHealth(); }}>Check health</button><button className="primary" disabled={busy === "support"} onClick={() => { void exportBundle(); }}>Export support bundle</button></div></div>
    {error && <div className="inline-alert error" role="alert">{error}</div>}{notice && <div className="inline-alert success" role="status">{notice}</div>}
    <div className="workflow-layout">
      <section className="panel workflow-catalog"><div className="panel-title"><span>Registered workflows</span><small>{definitions.length} allowlisted</small></div>{definitions.map((workflow) => <article className="workflow-card" key={workflow.id}><div><b>{workflow.label}</b><p>{workflow.description}</p><code>{workflow.executable} {workflow.arguments.join(" ")}</code></div><div><span>{workflow.timeoutSeconds}s timeout</span><button className="primary" disabled={busy === workflow.id || tasks.some((task) => task.workflowId === workflow.id && task.status === "running")} onClick={() => { void start(workflow); }}>Run</button></div></article>)}</section>
      <section className="panel task-console"><div className="panel-title"><span>Task output</span><small aria-live="polite">{selected?.status ?? "idle"}</small></div>{selected ? <><header className="task-summary"><div><b>{selected.label}</b><code>{selected.workingDirectory}</code></div><span className={`task-status ${selected.status}`}>{selected.status}</span>{selected.status === "running" && <button className="secondary danger" disabled={busy === selected.id} onClick={() => { void cancel(selected); }}>Cancel task</button>}</header><div className="console-output" role="log" aria-label="Redacted workflow output" aria-live="polite">{selected.output.length ? selected.output.map((output) => <div className={output.stream} key={output.sequence}><time>{new Date(output.timestamp).toLocaleTimeString()}</time><span>{output.line || " "}</span></div>) : <div className="console-empty">Waiting for output…</div>}</div>{selected.message && <footer className="task-message">{selected.message}{selected.exitCode !== undefined && ` · exit ${selected.exitCode}`}</footer>}</> : <div className="empty-state"><strong>No workflow history</strong><span>Run an allowlisted workflow to stream its redacted output here.</span></div>}</section>
      <section className="panel task-history"><div className="panel-title"><span>Local history</span><small>{tasks.length}/100</small></div>{tasks.length ? tasks.map((task) => <button className={task.id === selected?.id ? "active" : ""} onClick={() => setSelectedTaskId(task.id)} key={task.id}><span className={`task-dot ${task.status}`} /><b>{task.label}</b><small>{new Date(task.startedAt).toLocaleString()}</small><em>{task.status}</em></button>) : <div className="empty-state"><span>History is stored locally in app data.</span></div>}</section>
      <section className="panel health-panel"><div className="panel-title"><span>Project diagnostics</span><small>{health?.status ?? "checking"}</small></div>{health ? <><div className={`health-summary ${health.status}`}><b>{health.status}</b><span>Checked {new Date(health.checkedAt).toLocaleString()}</span>{health.recoveryAvailable && <button className="secondary" disabled={busy === "recovery"} onClick={() => { void recover(); }}>Recover snapshot</button>}</div>{health.diagnostics.length ? health.diagnostics.map((item, index) => <div className={`health-item ${item.severity}`} key={`${item.code}-${index}`}><code>{item.code}</code><span>{item.message}</span></div>) : <div className="empty-state compact-health"><strong>No diagnostics</strong><span>Blueprint references and local dependencies are healthy.</span></div>}</> : <div className="empty-state"><span>Reading project diagnostics…</span></div>}</section>
    </div>
  </div>;
}
