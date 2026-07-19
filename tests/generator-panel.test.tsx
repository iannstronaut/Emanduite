import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { GeneratorPanel } from "../src/app/GeneratorPanel";
import type { BlueprintV1 } from "../src/contracts/blueprint";
import type { ProjectSession } from "../src/contracts/workspace";

const mocks = vi.hoisted(() => ({ preview: vi.fn(), generate: vi.fn() }));

vi.mock("../src/lib/tauri", () => ({
  selectProjectDirectory: vi.fn(async () => null),
  previewGeneration: mocks.preview,
  generateProject: mocks.generate,
}));

const blueprint = {
  schemaVersion: 1,
  projectId: "00000000-0000-4000-8000-000000000001",
  projectName: "Phase 5 fixture",
  generatedWith: { emanduite: "0.1.0", template: "desktop-foundation" },
  databases: { sides: [], main: {
    id: "00000000-0000-4000-8000-000000000002", name: "Main SQLite", provider: "sqlite",
    capabilities: ["read", "schema"], connection: { kind: "sqlite", path: "C:\\fixture.sqlite" }, tables: [],
  } },
  entities: { users: { id: "entity-1", databaseId: "db-1", tableId: "table-1", fields: {} } },
  resources: {}, roles: {}, menus: [], extensions: {},
  global: { template: "default", settings: {} },
} as BlueprintV1;
const session: ProjectSession = { path: "C:\\project\\emanduite-project.json", blueprint };

describe("Phase 5 generation panel", () => {
  it("previews ownership and generates only after a valid preview", async () => {
    mocks.preview.mockResolvedValueOnce({ ok: true, data: {
      targetDirectory: "C:\\generated", templateId: "next-admin-v1", templateVersion: "1.0.0",
      blueprintHash: "abc", files: ["package.json", "prisma/schema.prisma"],
      generatedFileCount: 1, userFileCount: 1, entityCount: 1,
    } });
    mocks.generate.mockResolvedValueOnce({ ok: true, data: {
      targetDirectory: "C:\\generated", manifestPath: "C:\\generated\\.emanduite\\manifest.json",
      writtenFileCount: 2, preservedFileCount: 0, deletedFileCount: 0, conflicts: [],
    } });
    const generated = vi.fn(async () => undefined);

    render(<GeneratorPanel session={session} onGenerated={generated} />);
    const generate = screen.getByRole("button", { name: "Generate" });
    expect(generate).toBeDisabled();
    fireEvent.change(screen.getByLabelText("Target directory"), { target: { value: "C:\\generated" } });
    fireEvent.click(screen.getByRole("button", { name: "Preview files" }));

    expect(await screen.findByText("prisma/schema.prisma")).toBeInTheDocument();
    expect(generate).toBeEnabled();
    fireEvent.click(generate);
    await waitFor(() => expect(mocks.generate).toHaveBeenCalledWith(session.path, "C:\\generated"));
    expect(await screen.findByText(/Generation complete: 2 written/)).toBeInTheDocument();
    expect(generated).toHaveBeenCalledOnce();
  });
});
