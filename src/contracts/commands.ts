export type ErrorCode = "VALIDATION_ERROR" | "NOT_FOUND" | "CONFLICT" | "IO_ERROR" | "DATABASE_ERROR" | "SECRET_STORE_ERROR" | "UNSUPPORTED_VERSION" | "CAPABILITY_DENIED" | "INVALID_PATH" | "AI_PROVIDER_ERROR" | "INTERNAL_ERROR";

export interface CommandError { code: ErrorCode; message: string; correlationId: string; details?: Record<string, unknown>; }
export type CommandResponse<T> = { ok: true; data: T } | { ok: false; error: CommandError };

export interface AppInfo {
  name: string;
  version: string;
  phase: string;
  blueprintSchemaVersion: number;
  databaseProviders: string[];
}

export interface ValidationDiagnostic { path: string; code: string; message: string; }
export interface SecretReference { secretRef: string; }
export interface SecretPresence { exists: boolean; }

export function assertCommandData<T>(response: CommandResponse<T>): T {
  if (!response.ok) throw new Error(`${response.error.code}: ${response.error.message}`);
  return response.data;
}
