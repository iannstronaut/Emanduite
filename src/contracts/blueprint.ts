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
  registrationPolicy: "disabled" | "inviteOnly" | "open";
  passwordLogin: boolean;
}

export interface ValidationRule { kind: string; value?: unknown; message?: string; }
export interface FieldOption { label: string; value: string; }
export interface RelationDisplay {
  targetEntityId: string;
  displayFieldId: string;
  missingBehavior: "empty" | "rawValue" | "error";
}

export interface EntityFieldConfig {
  id: string;
  columnId: string;
  control: string;
  showInList: boolean;
  showInView: boolean;
  showInForm: boolean;
  required: boolean;
  validation: ValidationRule[];
  options: FieldOption[];
  relationDisplay?: RelationDisplay;
}

export interface EntityConfig {
  id: string;
  label?: string;
  databaseId: string;
  tableId: string;
  fields: Record<string, EntityFieldConfig>;
}

export interface ResourceConfig {
  id: string;
  key: string;
  resourceType: string;
  actions: string[];
}

export interface RoleConfig {
  id: string;
  key: string;
  label: string;
  permissions: Record<string, string[]>;
}

export interface MenuItem {
  id: string;
  label: string;
  resourceId?: string;
  parentId?: string;
  order: number;
}

export interface ExtensionConfig {
  id: string;
  path: string;
  language: string;
  ownership: "userOwned";
}

export interface GlobalConfig {
  template: string;
  settings: Record<string, unknown>;
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
  roles: Record<string, RoleConfig>;
  menus: MenuItem[];
  extensions: Record<string, ExtensionConfig>;
  global: GlobalConfig;
}
