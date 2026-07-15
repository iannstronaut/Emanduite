import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { App } from "../src/app/App";

vi.mock("../src/lib/tauri", () => ({ getAppInfo: vi.fn().mockRejectedValue(new Error("not in tauri")) }));

describe("App", () => {
  it("renders the Phase 1 desktop foundation status", () => {
    render(<App />);
    expect(screen.getByText("Core contracts are ready for implementation.")).toBeInTheDocument();
    expect(screen.getByText("sqlite")).toBeInTheDocument();
  });
});
