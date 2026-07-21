import { useCallback, useEffect, useRef, useState } from "react";
import type { AppInfo, CommandError, CommandResponse } from "../contracts/commands";
import type { BlueprintV1 } from "../contracts/blueprint";
import type {
  ConnectionStatus,
  CreateProjectInput,
  DatabaseDiagnostic,
  DuplicateProjectInput,
  ExplorerLayout,
  ProjectSession,
  RecentProject,
  SaveState,
  WorkspaceView
} from "../contracts/workspace";
import * as api from "../lib/tauri";
import type { MigrationPlan, SchemaOperation } from "../contracts/schema-editor";

const fallbackInfo: AppInfo = {
  name: "Emanduite",
  version: "web-preview",
  phase: "Phase 5 - Next.js Generator",
  blueprintSchemaVersion: 1,
  databaseProviders: ["sqlite"]
};

function unwrap<T>(response: CommandResponse<T>): T {
  if (!response.ok) throw response.error;
  return response.data;
}

function commandMessage(error: unknown): string {
  if (typeof error === "string" && error.trim()) return error;
  if (error instanceof Error && error.message) return error.message;
  const value = error as Partial<CommandError>;
  if (typeof value?.message === "string" && value.message) return value.message;
  if (value && typeof value === "object") {
    try {
      const detail = JSON.stringify(value);
      if (detail && detail !== "{}") return detail;
    } catch { /* Keep the safe fallback below. */ }
  }
  return "Operation failed without a diagnostic from the desktop runtime.";
}

export function useWorkspace() {
  const [info, setInfo] = useState(fallbackInfo);
  const [runtime, setRuntime] = useState("Browser preview");
  const [view, setView] = useState<WorkspaceView>("projects");
  const [session, setSession] = useState<ProjectSession | null>(null);
  const [recent, setRecent] = useState<RecentProject[]>([]);
  const [saveState, setSaveState] = useState<SaveState>("saved");
  const [error, setError] = useState<string | null>(null);
  const [connection, setConnection] = useState<ConnectionStatus | null>(null);
  const [diagnostics, setDiagnostics] = useState<DatabaseDiagnostic[]>([]);
  const [layout, setLayoutState] = useState<ExplorerLayout>({ panX: 32, panY: 32, zoom: 1, selectedTableId: null });
  const autosave = useRef<ReturnType<typeof setTimeout> | null>(null);
  const layoutSave = useRef<ReturnType<typeof setTimeout> | null>(null);

  const refreshRecent = useCallback(async () => {
    setRecent(unwrap(await api.listRecentProjects()));
  }, []);

  const activate = useCallback(async (next: ProjectSession) => {
    setSession(next);
    setSaveState("saved");
    setConnection(null);
    setDiagnostics([]);
    setError(null);
    setLayoutState(unwrap(await api.getExplorerLayout(next.path)));
  }, []);

  useEffect(() => {
    let live = true;
    void (async () => {
      try {
        const appInfo = unwrap(await api.getAppInfo());
        if (!live) return;
        setInfo(appInfo);
        setRuntime("Tauri runtime connected");
        const [items, active] = await Promise.all([
          api.listRecentProjects().then(unwrap),
          api.getActiveProjectPath().then(unwrap)
        ]);
        if (!live) return;
        setRecent(items);
        if (active) await activate(unwrap(await api.openProject(active)));
      } catch {
        // Browser preview intentionally runs without Tauri IPC.
      }
    })();
    return () => { live = false; };
  }, [activate]);

  useEffect(() => () => {
    if (autosave.current) clearTimeout(autosave.current);
    if (layoutSave.current) clearTimeout(layoutSave.current);
  }, []);

  const run = useCallback(async <T,>(operation: () => Promise<T>): Promise<T | undefined> => {
    setError(null);
    try { return await operation(); }
    catch (value) { setError(commandMessage(value)); return undefined; }
  }, []);

  const create = (input: CreateProjectInput) => run(async () => {
    const next = unwrap(await api.createProject(input));
    await activate(next); await refreshRecent(); setView("database"); return next;
  });

  const openProject = (path: string) => run(async () => {
    const next = unwrap(await api.openProject(path));
    await activate(next); await refreshRecent(); setView("database"); return next;
  });

  const duplicate = (input: DuplicateProjectInput) => run(async () => {
    const next = unwrap(await api.duplicateProject(input));
    await activate(next); await refreshRecent(); return next;
  });

  const recover = () => run(async () => {
    if (!session) return;
    const next = unwrap(await api.recoverProject(session.path));
    await activate(next);
    return next;
  });

  const refreshProject = () => run(async () => {
    if (!session) return;
    const next = unwrap(await api.openProject(session.path));
    await activate(next);
    return next;
  });

  const removeRecent = (path: string) => run(async () => {
    unwrap(await api.removeRecentProject(path));
    if (session?.path === path) setSession(null);
    await refreshRecent();
  });

  const updateBlueprint = useCallback((update: (current: BlueprintV1) => BlueprintV1) => {
    setSession((current) => current ? { ...current, blueprint: update(current.blueprint) } : current);
    setSaveState("dirty");
  }, []);

  const commitBlueprint = useCallback((update: (current: BlueprintV1) => BlueprintV1) => {
    if (!session) return;
    const blueprint = update(session.blueprint);
    void run(async () => {
      const invalid = unwrap(await api.validateBlueprint(blueprint));
      if (invalid.length) {
        const first = invalid[0];
        throw new Error(`${first.path}: ${first.message}`);
      }
      setSession((current) => current ? { ...current, blueprint } : current);
      setSaveState("dirty");
    });
  }, [run, session]);

  useEffect(() => {
    if (!session || saveState !== "dirty") return;
    if (autosave.current) clearTimeout(autosave.current);
    autosave.current = setTimeout(() => {
      setSaveState("saving");
      void api.saveProject(session.path, session.blueprint).then((response) => {
        const saved = unwrap(response);
        setSession(saved);
        setSaveState("saved");
        void refreshRecent();
      }).catch((value) => {
        setSaveState("error");
        setError(commandMessage(value));
      });
    }, 700);
  }, [session, saveState, refreshRecent]);

  const setSqlitePath = (path: string) => updateBlueprint((blueprint) => ({
    ...blueprint,
    databases: {
      ...blueprint.databases,
      main: { ...blueprint.databases.main, connection: { kind: "sqlite", path }, tables: [] }
    }
  }));

  const testConnection = () => run(async () => {
    if (!session) return;
    const status = unwrap(await api.testSqliteConnection(session.blueprint.databases.main));
    setConnection(status); return status;
  });

  const introspect = () => run(async () => {
    if (!session) return;
    const result = unwrap(await api.introspectSqlite(session.blueprint.databases.main));
    setDiagnostics(result.diagnostics);
    updateBlueprint((blueprint) => ({
      ...blueprint,
      databases: {
        ...blueprint.databases,
        main: { ...blueprint.databases.main, tables: result.tables }
      }
    }));
    setView("schema"); return result;
  });

  const planSchema = (operations: SchemaOperation[]) => run(async () => {
    if (!session) return;
    return unwrap(await api.planSqliteSchemaChanges(session.blueprint.databases.main, operations));
  });

  const applySchema = (plan: MigrationPlan, confirmationToken?: string | null) => run(async () => {
    if (!session) return;
    const applied = unwrap(await api.applySqliteSchemaPlan(plan.id, confirmationToken));
    const result = unwrap(await api.introspectSqlite(session.blueprint.databases.main));
    setDiagnostics(result.diagnostics);
    updateBlueprint((blueprint) => ({
      ...blueprint,
      databases: {
        ...blueprint.databases,
        main: { ...blueprint.databases.main, tables: result.tables }
      }
    }));
    return applied;
  });

  const setLayout = useCallback((next: ExplorerLayout) => {
    setLayoutState(next);
    if (!session) return;
    if (layoutSave.current) clearTimeout(layoutSave.current);
    layoutSave.current = setTimeout(() => {
      void api.saveExplorerLayout(session.path, next).catch(() => undefined);
    }, 400);
  }, [session]);

  return {
    info, runtime, view, setView, session, recent, saveState, error, setError,
    connection, diagnostics, layout, setLayout, create, openProject, duplicate,
    removeRecent, recover, refreshProject, setSqlitePath, testConnection, introspect, updateBlueprint, commitBlueprint,
    planSchema, applySchema
  };
}
