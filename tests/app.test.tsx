import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { App } from "../src/app/App";

vi.mock("../src/lib/tauri", () => ({
  getAppInfo: vi.fn().mockRejectedValue(new Error("not in tauri")),
  listRecentProjects: vi.fn(),
  getActiveProjectPath: vi.fn(),
  openProject: vi.fn(),
  getExplorerLayout: vi.fn(),
  selectBlueprintFile: vi.fn()
}));

describe("Phase 2 workspace", () => {
  it("renders the Project Manager in browser preview", () => {
    render(<App />);
    expect(screen.getByText("Local workspaces")).toBeInTheDocument();
    expect(screen.getByText("New project")).toBeInTheDocument();
    expect(screen.getByText("No recent projects")).toBeInTheDocument();
  });

  it("opens the command palette with Ctrl+K", () => {
    render(<App />);
    fireEvent.keyDown(window, { key: "k", ctrlKey: true });
    expect(screen.getByRole("dialog", { name: "Command palette" })).toBeInTheDocument();
    expect(screen.getByText("Project: Show manager")).toBeInTheDocument();
  });
});
