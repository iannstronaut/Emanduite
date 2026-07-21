import type { BlueprintV1, DatabaseConfig, Table } from "./blueprint";

export interface ProjectSession {
  path: string;
  blueprint: BlueprintV1;
}

export interface RecentProject {
  path: string;
  name: string;
  lastOpenedAt: string;
}

export interface ExplorerLayout {
  panX: number;
  panY: number;
  zoom: number;
  selectedTableId: string | null;
}

export interface ConnectionStatus {
  provider: string;
  databaseLabel: string;
  sqliteVersion?: string;
}

export interface DatabaseDiagnostic {
  code: string;
  object: string;
  message: string;
}

export interface IntrospectionResult {
  tables: Table[];
  diagnostics: DatabaseDiagnostic[];
}

export interface CreateProjectInput {
  directory: string;
  name: string;
  sqlitePath: string;
  superadminEmail: string;
  superadminPassword: string;
}

export interface DuplicateProjectInput {
  sourcePath: string;
  targetDirectory: string;
  name: string;
}

export type SaveState = "saved" | "dirty" | "saving" | "error";
export type WorkspaceView = "projects" | "database" | "schema" | "editor" | "ai" | "entities" | "permissions" | "auth" | "extensions" | "global" | "settings" | "workflow" | "generator";

export type DatabaseConnection = DatabaseConfig;
