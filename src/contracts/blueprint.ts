export const BLUEPRINT_SCHEMA_VERSION = 1 as const;

export type DatabaseProvider = "sqlite" | "postgresql" | "mysql";
export type Capability = "read" | "create" | "update" | "delete" | "schema";
export type CanonicalType =
  | "integer"
  | "real"
  | "decimal"
  | "boolean"
  | "text"
  | "bytes"
  | "date"
  | "dateTime"
  | "json"
  | "unknown";

export type ConnectionConfig =
  | { kind: "sqlite"; path: string }
  | { kind: "server"; host: string; port: number; database: string; username: string };

export interface GeneratedWith {
  emanduite: string;
  template: string;
}

export interface Column {
  id: string;
  name: string;
  nativeType: string;
  canonicalType: CanonicalType;
  nullable: boolean;
  primaryKey: boolean;
  defaultValue?: string;
}

export interface ForeignKey {
  id: string;
  fromColumn: string;
  toTable: string;
  toColumn: string;
  onUpdate?: string;
  onDelete?: string;
}

export interface DatabaseIndex {
  id: string;
  name: string;
  unique: boolean;
  columns: string[];
}

export interface Table {
  id: string;
  name: string;
  columns: Column[];
  foreignKeys: ForeignKey[];
  indexes: DatabaseIndex[];
}

export interface DatabaseConfig {
  id: string;
  name: string;
  provider: DatabaseProvider;
  capabilities: Capability[];
  connection: ConnectionConfig;
  secretRef?: string;
  tables: Table[];
}

export interface AuthConfig {
  databaseId: string;
  userEntityId: string;
  externalIdFieldId: string;
  identifierFieldId: string;
  passwordFieldId: string;
}

export interface EntityFieldConfig {
  id: string;
  columnId: string;
  control: string;
  showInList: boolean;
  showInView: boolean;
}

export interface EntityConfig {
  id: string;
  databaseId: string;
  tableId: string;
  fields: Record<string, EntityFieldConfig>;
}

export interface ResourceConfig {
  id: string;
  key: string;
  resourceType: string;
}

export interface BlueprintV1 {
  schemaVersion: typeof BLUEPRINT_SCHEMA_VERSION;
  projectId: string;
  projectName: string;
  generatedWith: GeneratedWith;
  targetDirectory?: string;
  databases: { main: DatabaseConfig; sides: DatabaseConfig[] };
  auth?: AuthConfig;
  entities: Record<string, EntityConfig>;
  resources: Record<string, ResourceConfig>;
}
