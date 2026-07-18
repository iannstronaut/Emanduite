import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { SchemaExplorer } from "../src/app/SchemaExplorer";
import type { ProjectSession } from "../src/contracts/workspace";

const session: ProjectSession = {
  path: "C:\\workspace\\emanduite-project.json",
  blueprint: {
    schemaVersion: 1,
    projectId: "project-id",
    projectName: "Explorer Fixture",
    generatedWith: { emanduite: "0.1.0", template: "desktop-foundation" },
    databases: {
      sides: [],
      main: {
        id: "database-id",
        name: "Main SQLite",
        provider: "sqlite",
        capabilities: ["read"],
        connection: { kind: "sqlite", path: "C:\\fixture.sqlite" },
        tables: [
          {
            id: "users-id", name: "users", foreignKeys: [], indexes: [],
            columns: [{ id: "user-col", name: "id", nativeType: "INTEGER", canonicalType: "integer", nullable: false, primaryKey: true }]
          },
          {
            id: "posts-id", name: "posts", indexes: [{ id: "index-id", name: "posts_user_idx", unique: false, columns: ["user_id"] }],
            foreignKeys: [{ id: "fk-id", fromColumn: "user_id", toTable: "users", toColumn: "id" }],
            columns: [{ id: "post-col", name: "user_id", nativeType: "INTEGER", canonicalType: "integer", nullable: false, primaryKey: false }]
          }
        ]
      }
    },
    entities: {}, resources: {}
  }
};

describe("SchemaExplorer", () => {
  it("renders relations and selects a table for the properties panel", () => {
    const onLayout = vi.fn();
    const { container, rerender } = render(<SchemaExplorer session={session} layout={{ panX: 32, panY: 32, zoom: 1, selectedTableId: null }} onLayout={onLayout} />);
    expect(container.querySelectorAll(".relation-layer line")).toHaveLength(1);
    fireEvent.click(screen.getByRole("button", { name: /posts/i }));
    expect(onLayout).toHaveBeenCalledWith(expect.objectContaining({ selectedTableId: "posts-id" }));
    rerender(<SchemaExplorer session={session} layout={{ panX: 32, panY: 32, zoom: 1, selectedTableId: "posts-id" }} onLayout={onLayout} />);
    expect(screen.getByText("Stable ID")).toBeInTheDocument();
    expect(screen.getByText("posts_user_idx")).toBeInTheDocument();
  });
});
