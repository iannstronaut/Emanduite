import type { Column, DatabaseConfig, ForeignKey, Table } from "./blueprint";

export type SchemaOperation =
  | { kind: "addTable"; operationId: string; table: Table }
  | { kind: "dropTable"; operationId: string; tableName: string }
  | { kind: "addColumn"; operationId: string; tableName: string; column: Column }
  | { kind: "dropColumn"; operationId: string; tableName: string; columnName: string }
  | { kind: "renameColumn"; operationId: string; tableName: string; from: string; to: string }
  | { kind: "addForeignKey"; operationId: string; tableName: string; foreignKey: ForeignKey }
  | { kind: "dropForeignKey"; operationId: string; tableName: string; foreignKeyId: string };

export interface MigrationPlan {
  id: string;
  schemaFingerprint: string;
  operations: SchemaOperation[];
  statements: string[];
  sqlPreview: string;
  destructive: boolean;
  requiresBackup: boolean;
  confirmationToken: string | null;
}

export interface ApplyResult {
  planId: string;
  backupPath: string;
  statementsApplied: number;
}

export interface ExtensionDocument {
  path: string;
  language: string;
  content: string;
  valid: boolean;
  diagnostics: string[];
}

export type PlanInput = { config: DatabaseConfig; operations: SchemaOperation[] };
