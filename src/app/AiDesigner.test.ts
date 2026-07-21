import { describe, expect, it } from "vitest";
import { createDesign, SYSTEM_TABLES } from "./AiDesigner";

describe("AI database designer safeguards", () => {
  it("keeps Emanduite system table names out of inventory proposals", () => {
    const proposal = createDesign("Buat aplikasi inventaris gudang dengan produk dan supplier");
    expect(proposal.tables.map((table) => table.name).some((name) => SYSTEM_TABLES.has(name))).toBe(false);
    expect(proposal.tables.map((table) => table.name)).toEqual(["inv_suppliers", "inv_products", "inv_stock_movements"]);
  });
});
