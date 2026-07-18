import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { WorkflowPanel } from "../src/app/WorkflowPanel";
import type { BlueprintV1 } from "../src/contracts/blueprint";
import type { ProjectSession } from "../src/contracts/workspace";

const mocks = vi.hoisted(() => ({
  start: vi.fn(), cancel: vi.fn(), listen: vi.fn(async () => vi.fn())
}));

vi.mock("@tauri-apps/api/event", () => ({ listen: mocks.listen }));
vi.mock("../src/lib/tauri", () => ({
  listWorkflowDefinitions: vi.fn(async () => ({ ok: true, data: [{
    id: "npm-build", label: "Build desktop", description: "Create a production bundle",
    executable: "npm", arguments: ["run", "build"], timeoutSeconds: 300, requiresPackageScript: "build"
  }] })),
  listWorkflowTasks: vi.fn(async () => ({ ok: true, data: [] })),
  diagnoseProject: vi.fn(async () => ({ ok: true, data: {
    status: "healthy", recoveryAvailable: false, checkedAt: "2026-07-19T00:00:00Z", diagnostics: []
  } })),
  startRegisteredWorkflow: mocks.start,
  cancelRegisteredWorkflow: mocks.cancel,
  selectSupportDirectory: vi.fn(async () => null),
  exportSupportBundle: vi.fn()
}));

const blueprint = {
  schemaVersion: 1,
  projectId: "00000000-0000-4000-8000-000000000001",
  projectName: "Phase 4 fixture",
  generatedWith: { emanduite: "0.1.0", template: "desktop-foundation" },
  databases: { sides: [], main: {
    id: "00000000-0000-4000-8000-000000000002", name: "Main SQLite", provider: "sqlite",
    capabilities: ["read", "schema"], connection: { kind: "sqlite", path: "C:\\fixture.sqlite" }, tables: []
  } },
  entities: {}, resources: {}, roles: {}, menus: [], extensions: {},
  global: { template: "default", settings: {} }
} as BlueprintV1;
const session: ProjectSession = { path: "C:\\project\\emanduite-project.json", blueprint };

describe("Phase 4 workflow operations", () => {
  it("shows allowlisted commands and can cancel a running task", async () => {
    mocks.start.mockResolvedValueOnce({ ok: true, data: {
      id: "task-1", workflowId: "npm-build", label: "Build desktop", workingDirectory: "C:\\project",
      status: "running", startedAt: "2026-07-19T00:00:00Z", output: []
    } });
    mocks.cancel.mockResolvedValueOnce({ ok: true, data: undefined });

    render(<WorkflowPanel session={session} onRecover={vi.fn()} />);
    expect(await screen.findByText("Registered workflows")).toBeInTheDocument();
    expect(screen.getByText("npm run build")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Run" }));
    await waitFor(() => expect(mocks.start).toHaveBeenCalledWith(session.path, "npm-build"));
    fireEvent.click(await screen.findByRole("button", { name: "Cancel task" }));
    await waitFor(() => expect(mocks.cancel).toHaveBeenCalledWith("task-1"));
    expect(screen.getByRole("log", { name: "Redacted workflow output" })).toBeInTheDocument();
  });
});
