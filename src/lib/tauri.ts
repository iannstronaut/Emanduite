import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import type { BlueprintV1, DatabaseConfig } from "../contracts/blueprint";
import type { AppInfo, CommandResponse, ValidationDiagnostic } from "../contracts/commands";
import type {
  ConnectionStatus,
  CreateProjectInput,
  DuplicateProjectInput,
  ExplorerLayout,
  IntrospectionResult,
  ProjectSession,
  RecentProject
} from "../contracts/workspace";
import type { ApplyResult, ExtensionDocument, MigrationPlan, SchemaOperation } from "../contracts/schema-editor";
import type { ProjectHealth, WorkflowDefinition, WorkflowTask } from "../contracts/workflow";

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
export const validateBlueprint = (blueprint: BlueprintV1) =>
  command<ValidationDiagnostic[]>("validate_blueprint_command", { value: blueprint });
export const duplicateProject = (input: DuplicateProjectInput) =>
  command<ProjectSession>("duplicate_project_command", { ...input });
export const removeRecentProject = (path: string) =>
  command<void>("remove_recent_project", { path });
export const testSqliteConnection = (config: DatabaseConfig) =>
  command<ConnectionStatus>("test_sqlite_connection", { config });
export const introspectSqlite = (config: DatabaseConfig) =>
  command<IntrospectionResult>("introspect_sqlite", { config });
export const planSqliteSchemaChanges = (config: DatabaseConfig, operations: SchemaOperation[]) =>
  command<MigrationPlan>("plan_sqlite_schema_changes", { config, operations });
export const applySqliteSchemaPlan = (planId: string, confirmationToken?: string | null) =>
  command<ApplyResult>("apply_sqlite_schema_plan", { planId, confirmationToken: confirmationToken ?? null });
export const getExplorerLayout = (projectPath: string) =>
  command<ExplorerLayout>("get_explorer_layout", { projectPath });
export const saveExplorerLayout = (projectPath: string, layout: ExplorerLayout) =>
  command<void>("save_explorer_layout", { projectPath, layout });
export const loadExtensionFile = (projectPath: string, relativePath: string, language: string) =>
  command<ExtensionDocument>("load_extension_file", { projectPath, relativePath, language });
export const validateExtensionFile = (relativePath: string, language: string, content: string) =>
  command<ExtensionDocument>("validate_extension_file", { relativePath, language, content });
export const saveExtensionFile = (projectPath: string, relativePath: string, language: string, content: string, format: boolean) =>
  command<ExtensionDocument>("save_extension_file", { projectPath, relativePath, language, content, format });
export const listWorkflowDefinitions = () => command<WorkflowDefinition[]>("list_workflow_definitions");
export const listWorkflowTasks = () => command<WorkflowTask[]>("list_workflow_tasks");
export const startRegisteredWorkflow = (projectPath: string, workflowId: string, workingDirectory?: string | null) =>
  command<WorkflowTask>("start_registered_workflow", { projectPath, workflowId, workingDirectory: workingDirectory ?? null });
export const cancelRegisteredWorkflow = (taskId: string) =>
  command<void>("cancel_registered_workflow", { taskId });
export const diagnoseProject = (projectPath: string) =>
  command<ProjectHealth>("diagnose_project_command", { projectPath });
export const recoverProject = (projectPath: string) =>
  command<ProjectSession>("recover_project_command", { projectPath });
export const exportSupportBundle = (projectPath: string, destinationDirectory: string) =>
  command<string>("export_support_bundle_command", { projectPath, destinationDirectory });

export async function selectProjectDirectory(): Promise<string | null> {
  const result = await open({ directory: true, multiple: false, title: "Select project directory" });
  return typeof result === "string" ? result : null;
}

export async function selectSupportDirectory(): Promise<string | null> {
  const result = await open({ directory: true, multiple: false, title: "Export redacted support bundle" });
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
