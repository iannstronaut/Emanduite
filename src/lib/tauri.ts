import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import type { BlueprintV1, DatabaseConfig } from "../contracts/blueprint";
import type { AppInfo, CommandResponse } from "../contracts/commands";
import type {
  ConnectionStatus,
  CreateProjectInput,
  DuplicateProjectInput,
  ExplorerLayout,
  IntrospectionResult,
  ProjectSession,
  RecentProject
} from "../contracts/workspace";

const command = <T>(name: string, args?: Record<string, unknown>) =>
  invoke<CommandResponse<T>>(name, args);

export const getAppInfo = () => command<AppInfo>("get_app_info");
export const listRecentProjects = () => command<RecentProject[]>("list_recent_projects");
export const getActiveProjectPath = () => command<string | null>("get_active_project_path");
export const createProject = (input: CreateProjectInput) =>
  command<ProjectSession>("create_project_command", { ...input });
export const openProject = (path: string) =>
  command<ProjectSession>("open_project_command", { path });
export const saveProject = (path: string, blueprint: BlueprintV1) =>
  command<ProjectSession>("save_project_command", { path, blueprint });
export const duplicateProject = (input: DuplicateProjectInput) =>
  command<ProjectSession>("duplicate_project_command", { ...input });
export const removeRecentProject = (path: string) =>
  command<void>("remove_recent_project", { path });
export const testSqliteConnection = (config: DatabaseConfig) =>
  command<ConnectionStatus>("test_sqlite_connection", { config });
export const introspectSqlite = (config: DatabaseConfig) =>
  command<IntrospectionResult>("introspect_sqlite", { config });
export const getExplorerLayout = (projectPath: string) =>
  command<ExplorerLayout>("get_explorer_layout", { projectPath });
export const saveExplorerLayout = (projectPath: string, layout: ExplorerLayout) =>
  command<void>("save_explorer_layout", { projectPath, layout });

export async function selectProjectDirectory(): Promise<string | null> {
  const result = await open({ directory: true, multiple: false, title: "Select project directory" });
  return typeof result === "string" ? result : null;
}

export async function selectSqliteFile(): Promise<string | null> {
  const result = await open({
    multiple: false,
    title: "Select SQLite database",
    filters: [{ name: "SQLite", extensions: ["sqlite", "sqlite3", "db"] }]
  });
  return typeof result === "string" ? result : null;
}

export async function selectBlueprintFile(): Promise<string | null> {
  const result = await open({
    multiple: false,
    title: "Open Emanduite project",
    filters: [{ name: "Emanduite Project", extensions: ["json"] }]
  });
  return typeof result === "string" ? result : null;
}
