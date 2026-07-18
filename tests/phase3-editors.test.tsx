import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { PermissionEditor } from "../src/app/ConfigEditors";
import { SchemaEditor } from "../src/app/SchemaEditor";
import type { BlueprintV1 } from "../src/contracts/blueprint";
import type { MigrationPlan, SchemaOperation } from "../src/contracts/schema-editor";
import type { ProjectSession } from "../src/contracts/workspace";

const blueprint: BlueprintV1 = {
  schemaVersion: 1,
  projectId: "00000000-0000-4000-8000-000000000001",
  projectName: "Phase 3 fixture",
  generatedWith: { emanduite: "0.1.0", template: "desktop-foundation" },
  databases: {
    sides: [],
    main: {
      id: "00000000-0000-4000-8000-000000000002",
      name: "Main SQLite",
      provider: "sqlite",
      capabilities: ["read", "schema"],
      connection: { kind: "sqlite", path: "C:\\fixture.sqlite" },
      tables: [{
        id: "00000000-0000-4000-8000-000000000003",
        name: "users",
        columns: [{ id: "00000000-0000-4000-8000-000000000004", name: "id", nativeType: "INTEGER", canonicalType: "integer", nullable: false, primaryKey: true }],
        foreignKeys: [], indexes: []
      }]
    }
  },
  entities: {}, resources: {}, roles: {}, menus: [], extensions: {},
  global: { template: "default", settings: {} }
};
const session: ProjectSession = { path: "C:\\project\\emanduite-project.json", blueprint };

describe("Phase 3 editor contracts", () => {
  it("requires preview and explicit confirmation before a destructive apply", async () => {
    const plan = (operations: SchemaOperation[]): MigrationPlan => ({
      id: "plan-id", schemaFingerprint: "schema-v1", operations, statements: ["DROP TABLE users"], sqlPreview: "DROP TABLE users;",
      destructive: true, requiresBackup: true, confirmationToken: "server-token"
    });
    const onPlan = vi.fn(async (operations: SchemaOperation[]) => plan(operations));
    const onApply = vi.fn(async () => ({ planId: "plan-id", backupPath: "backup.sqlite", statementsApplied: 1 }));
    render(<SchemaEditor session={session} onPlan={onPlan} onApply={onApply} />);

    fireEvent.click(screen.getByRole("button", { name: "Drop selected table" }));
    fireEvent.click(screen.getByRole("button", { name: "Preview migration" }));
    await screen.findByRole("dialog", { name: "Migration preview" });
    expect(onPlan).toHaveBeenCalledWith([expect.objectContaining({ kind: "dropTable", tableName: "users" })]);
    const apply = screen.getByRole("button", { name: "Apply migration" });
    expect(apply).toBeDisabled();
    fireEvent.change(screen.getByLabelText(/Type APPLY/i), { target: { value: "APPLY" } });
    fireEvent.click(apply);
    await waitFor(() => expect(onApply).toHaveBeenCalledWith(expect.objectContaining({ id: "plan-id" }), "server-token"));
  });

  it("stores authorization as resource actions while menu mapping stays separate", () => {
    const onCommit = vi.fn();
    render(<PermissionEditor session={session} onCommit={onCommit} />);
    fireEvent.change(screen.getByPlaceholderText("resource key"), { target: { value: "users" } });
    fireEvent.click(screen.getByRole("button", { name: "Add resource" }));
    fireEvent.change(screen.getByPlaceholderText("role key"), { target: { value: "admin" } });
    fireEvent.click(screen.getByRole("button", { name: "Add role" }));
    fireEvent.click(screen.getByLabelText("admin users read"));
    const actionResource = screen.getByLabelText("Action resource") as HTMLSelectElement;
    fireEvent.change(actionResource, { target: { value: actionResource.options[1].value } });
    fireEvent.change(screen.getByPlaceholderText("custom action"), { target: { value: "export" } });
    fireEvent.click(screen.getByRole("button", { name: "Add action" }));
    fireEvent.click(screen.getByLabelText("admin users export"));
    fireEvent.click(screen.getByRole("button", { name: "Add menu item" }));
    fireEvent.click(screen.getByRole("button", { name: "Add menu item" }));
    const secondParent = screen.getByLabelText("Menu 2 parent") as HTMLSelectElement;
    fireEvent.change(secondParent, { target: { value: secondParent.options[1].value } });
    fireEvent.click(screen.getByRole("button", { name: "Apply config" }));

    const update = onCommit.mock.calls[0][0] as (value: BlueprintV1) => BlueprintV1;
    const result = update(blueprint);
    const resource = result.resources.users;
    expect(resource.actions).toContain("export");
    expect(result.roles.admin.permissions[resource.id]).toEqual(["read", "export"]);
    expect(result.menus[0].resourceId).toBe(resource.id);
    expect(result.menus[1].parentId).toBe(result.menus[0].id);
  });
});
