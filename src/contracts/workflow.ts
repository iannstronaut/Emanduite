export type WorkflowStatus = "running" | "succeeded" | "failed" | "cancelled" | "timedOut";
export type OutputStream = "stdout" | "stderr" | "system";

export interface WorkflowDefinition {
  id: string;
  label: string;
  description: string;
  executable: string;
  arguments: string[];
  timeoutSeconds: number;
  requiresPackageScript?: string;
}

export interface WorkflowOutput {
  sequence: number;
  stream: OutputStream;
  line: string;
  timestamp: string;
}

export interface WorkflowTask {
  id: string;
  workflowId: string;
  label: string;
  workingDirectory: string;
  status: WorkflowStatus;
  startedAt: string;
  finishedAt?: string;
  exitCode?: number;
  message?: string;
  output: WorkflowOutput[];
}

export interface WorkflowOutputEvent { taskId: string; output: WorkflowOutput; }
export interface WorkflowTaskEvent { task: WorkflowTask; }

export type HealthStatus = "healthy" | "degraded" | "recoverable" | "corrupt" | "missing";
export type DiagnosticSeverity = "info" | "warning" | "error";

export interface ProjectDiagnostic {
  code: string;
  severity: DiagnosticSeverity;
  message: string;
}

export interface ProjectHealth {
  status: HealthStatus;
  recoveryAvailable: boolean;
  diagnostics: ProjectDiagnostic[];
  checkedAt: string;
}
