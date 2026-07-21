import { useEffect, useMemo, useState } from "react";
import type { CanonicalType, Column, ForeignKey, Table } from "../contracts/blueprint";
import type { ApplyResult, MigrationPlan, SchemaOperation } from "../contracts/schema-editor";
import type { ProjectSession } from "../contracts/workspace";
import * as api from "../lib/tauri";

export const SYSTEM_TABLES = new Set(["mst_roles", "mst_users", "sys_resources", "sys_permissions", "sys_audit_logs"]);
const uuid = () => crypto.randomUUID();

interface DesignProposal {
  title: string;
  summary: string;
  assumptions: string[];
  tables: Table[];
}

interface AiConfig {
  provider: "openaiCompatible";
  baseUrl: string;
  model: string;
  maxOutputTokens: number;
  temperature: number;
  apiKeySecretRef?: string;
}

interface ConversationEntry {
  id: string;
  role: "user" | "assistant" | "system";
  message: string;
  state?: "working" | "error";
}

interface Props {
  session: ProjectSession;
  onPlan: (operations: SchemaOperation[]) => Promise<MigrationPlan | undefined>;
  onApply: (plan: MigrationPlan, token?: string | null) => Promise<ApplyResult | undefined>;
  onOpenSchema: () => void;
}

const column = (name: string, nativeType: string, canonicalType: CanonicalType, nullable = false, primaryKey = false, defaultValue?: string): Column => ({ id: uuid(), name, nativeType, canonicalType, nullable, primaryKey, defaultValue });
const table = (name: string, columns: Column[], foreignKeys: ForeignKey[] = []): Table => ({ id: uuid(), name, columns, foreignKeys, indexes: [] });
const relation = (fromColumn: string, toTable: string, toColumn = "id"): ForeignKey => ({ id: uuid(), fromColumn, toTable, toColumn, onUpdate: "CASCADE", onDelete: "RESTRICT" });

function inventoryDesign(): DesignProposal {
  return {
    title: "Inventory operations", summary: "Products, suppliers, and immutable stock movement history.",
    assumptions: ["Current stock is cached on inv_products for fast reads.", "Every stock change is recorded in inv_stock_movements.", "System tables and the default superadmin stay untouched."],
    tables: [
      table("inv_suppliers", [column("id", "INTEGER", "integer", false, true), column("name", "TEXT", "text"), column("email", "TEXT", "text", true), column("phone", "TEXT", "text", true), column("created_at", "DATETIME", "dateTime", false, false, "CURRENT_TIMESTAMP")]),
      table("inv_products", [column("id", "INTEGER", "integer", false, true), column("sku", "TEXT", "text"), column("name", "TEXT", "text"), column("supplier_id", "INTEGER", "integer", true), column("stock_on_hand", "INTEGER", "integer", false, false, "0"), column("reorder_point", "INTEGER", "integer", false, false, "0"), column("created_at", "DATETIME", "dateTime", false, false, "CURRENT_TIMESTAMP")], [relation("supplier_id", "inv_suppliers")]),
      table("inv_stock_movements", [column("id", "INTEGER", "integer", false, true), column("product_id", "INTEGER", "integer"), column("movement_type", "TEXT", "text"), column("quantity", "INTEGER", "integer"), column("note", "TEXT", "text", true), column("created_at", "DATETIME", "dateTime", false, false, "CURRENT_TIMESTAMP")], [relation("product_id", "inv_products")])
    ]
  };
}

function salesDesign(): DesignProposal {
  return {
    title: "Sales workflow", summary: "Customers, order headers, and order line items.",
    assumptions: ["Amounts are stored as decimal values.", "Order line items keep a price snapshot.", "System tables and the default superadmin stay untouched."],
    tables: [
      table("crm_customers", [column("id", "INTEGER", "integer", false, true), column("name", "TEXT", "text"), column("email", "TEXT", "text", true), column("phone", "TEXT", "text", true), column("created_at", "DATETIME", "dateTime", false, false, "CURRENT_TIMESTAMP")]),
      table("sales_orders", [column("id", "INTEGER", "integer", false, true), column("customer_id", "INTEGER", "integer"), column("status", "TEXT", "text", false, false, "'draft'"), column("ordered_at", "DATETIME", "dateTime", false, false, "CURRENT_TIMESTAMP"), column("total_amount", "DECIMAL(12,2)", "decimal", false, false, "0")], [relation("customer_id", "crm_customers")]),
      table("sales_order_items", [column("id", "INTEGER", "integer", false, true), column("order_id", "INTEGER", "integer"), column("description", "TEXT", "text"), column("quantity", "INTEGER", "integer"), column("unit_price", "DECIMAL(12,2)", "decimal")], [relation("order_id", "sales_orders")])
    ]
  };
}

function workDesign(): DesignProposal {
  return {
    title: "Project workspace", summary: "Projects and tasks with owner-ready fields.",
    assumptions: ["Task ownership can later link to mst_users after explicit review.", "Tasks use a small, generator-friendly status field.", "System tables and the default superadmin stay untouched."],
    tables: [
      table("pm_projects", [column("id", "INTEGER", "integer", false, true), column("name", "TEXT", "text"), column("description", "TEXT", "text", true), column("status", "TEXT", "text", false, false, "'active'"), column("created_at", "DATETIME", "dateTime", false, false, "CURRENT_TIMESTAMP")]),
      table("pm_tasks", [column("id", "INTEGER", "integer", false, true), column("project_id", "INTEGER", "integer"), column("title", "TEXT", "text"), column("description", "TEXT", "text", true), column("status", "TEXT", "text", false, false, "'todo'"), column("due_at", "DATETIME", "dateTime", true), column("created_at", "DATETIME", "dateTime", false, false, "CURRENT_TIMESTAMP")], [relation("project_id", "pm_projects")])
    ]
  };
}

export function createDesign(prompt: string): DesignProposal {
  const value = prompt.toLowerCase();
  if (/(inventory|inventaris|stock|gudang|warehouse|produk)/.test(value)) return inventoryDesign();
  if (/(sales|penjualan|order|customer|pelanggan)/.test(value)) return salesDesign();
  return workDesign();
}

function cleanName(value: string) { return value.trim().toLowerCase().replace(/[^a-z0-9_]+/g, "_").replace(/^_+|_+$/g, ""); }

function readAiConfig(session: ProjectSession): AiConfig | null {
  const candidate = session.blueprint.global.settings.ai;
  if (!candidate || typeof candidate !== "object" || Array.isArray(candidate)) return null;
  const value = candidate as Partial<AiConfig>;
  if (value.provider !== "openaiCompatible" || !value.baseUrl || !value.model || !value.apiKeySecretRef) return null;
  return {
    provider: "openaiCompatible", baseUrl: value.baseUrl, model: value.model,
    temperature: typeof value.temperature === "number" ? value.temperature : 0.2,
    maxOutputTokens: typeof value.maxOutputTokens === "number" ? value.maxOutputTokens : 1800,
    apiKeySecretRef: value.apiKeySecretRef
  };
}

function unwrap<T>(value: { ok: true; data: T } | { ok: false; error: { message: string } }): T {
  if (!value.ok) throw new Error(value.error.message);
  return value.data;
}

function canonicalType(nativeType: string): CanonicalType {
  const value = nativeType.toUpperCase();
  if (value.includes("INT")) return "integer";
  if (value.includes("DECIMAL") || value.includes("NUMERIC")) return "decimal";
  if (value.includes("REAL") || value.includes("FLOAT") || value.includes("DOUBLE")) return "real";
  if (value.includes("BOOL")) return "boolean";
  if (value.includes("DATE") || value.includes("TIME")) return "dateTime";
  if (value.includes("BLOB")) return "bytes";
  if (value.includes("JSON")) return "json";
  return "text";
}

function externalDesign(value: unknown): DesignProposal {
  if (!value || typeof value !== "object" || Array.isArray(value)) throw new Error("AI provider returned an invalid design proposal.");
  const data = value as { title?: unknown; summary?: unknown; assumptions?: unknown; tables?: unknown };
  if (!Array.isArray(data.tables) || !data.tables.length) throw new Error("AI provider did not propose any tables.");
  const names = new Set<string>();
  const tables = data.tables.map((raw, index) => {
    if (!raw || typeof raw !== "object" || Array.isArray(raw)) throw new Error(`AI proposal table ${index + 1} is invalid.`);
    const item = raw as { name?: unknown; columns?: unknown; foreignKeys?: unknown };
    const name = cleanName(typeof item.name === "string" ? item.name : "");
    if (!/^[a-z][a-z0-9_]*$/.test(name) || SYSTEM_TABLES.has(name) || names.has(name)) throw new Error(`AI proposal contains an invalid, duplicate, or protected table name: ${name || "(empty)"}.`);
    names.add(name);
    if (!Array.isArray(item.columns) || !item.columns.length) throw new Error(`AI proposal table ${name} has no columns.`);
    const columnNames = new Set<string>();
    const columns = item.columns.map((rawColumn, columnIndex) => {
      if (!rawColumn || typeof rawColumn !== "object" || Array.isArray(rawColumn)) throw new Error(`AI proposal column ${columnIndex + 1} in ${name} is invalid.`);
      const field = rawColumn as { name?: unknown; nativeType?: unknown; nullable?: unknown; primaryKey?: unknown; defaultValue?: unknown };
      const columnName = cleanName(typeof field.name === "string" ? field.name : "");
      if (!/^[a-z][a-z0-9_]*$/.test(columnName) || columnNames.has(columnName)) throw new Error(`AI proposal has an invalid or duplicate column in ${name}.`);
      columnNames.add(columnName);
      const nativeType = typeof field.nativeType === "string" && field.nativeType.trim() ? field.nativeType.trim().toUpperCase() : "TEXT";
      return column(columnName, nativeType, canonicalType(nativeType), Boolean(field.nullable), Boolean(field.primaryKey), typeof field.defaultValue === "string" ? field.defaultValue : undefined);
    });
    const foreignKeys = Array.isArray(item.foreignKeys) ? item.foreignKeys.flatMap((rawForeignKey) => {
      if (!rawForeignKey || typeof rawForeignKey !== "object" || Array.isArray(rawForeignKey)) return [];
      const foreignKey = rawForeignKey as { fromColumn?: unknown; toTable?: unknown; toColumn?: unknown; onDelete?: unknown };
      const fromColumn = cleanName(typeof foreignKey.fromColumn === "string" ? foreignKey.fromColumn : "");
      const toTable = cleanName(typeof foreignKey.toTable === "string" ? foreignKey.toTable : "");
      if (!columnNames.has(fromColumn) || !toTable || SYSTEM_TABLES.has(toTable)) return [];
      const next = relation(fromColumn, toTable, cleanName(typeof foreignKey.toColumn === "string" ? foreignKey.toColumn : "id") || "id");
      if (typeof foreignKey.onDelete === "string") next.onDelete = foreignKey.onDelete.toUpperCase();
      return [next];
    }) : [];
    return table(name, columns, foreignKeys);
  });
  return {
    title: typeof data.title === "string" && data.title.trim() ? data.title.trim() : "AI database design",
    summary: typeof data.summary === "string" && data.summary.trim() ? data.summary.trim() : "A reviewable database proposal from the configured AI provider.",
    assumptions: Array.isArray(data.assumptions) ? data.assumptions.filter((item): item is string => typeof item === "string" && Boolean(item.trim())).slice(0, 8) : [],
    tables
  };
}

export function AiDesigner({ session, onPlan, onApply, onOpenSchema }: Props) {
  const [prompt, setPrompt] = useState("Design an inventory workspace for a small warehouse with products, suppliers, and stock movements.");
  const [proposal, setProposal] = useState<DesignProposal | null>(null);
  const [plan, setPlan] = useState<MigrationPlan | null>(null);
  const [applying, setApplying] = useState(false);
  const [designing, setDesigning] = useState(false);
  const [elapsed, setElapsed] = useState(0);
  const [notice, setNotice] = useState("");
  const [providerError, setProviderError] = useState("");
  const [previewError, setPreviewError] = useState("");
  const [conversation, setConversation] = useState<ConversationEntry[]>([]);
  const aiConfig = useMemo(() => readAiConfig(session), [session]);
  const existing = useMemo(() => new Set(session.blueprint.databases.main.tables.map((item) => item.name)), [session]);
  const errors = useMemo(() => {
    if (!proposal) return [] as string[];
    const names = proposal.tables.map((item) => cleanName(item.name));
    const duplicate = names.find((name, index) => names.indexOf(name) !== index);
    const invalid = names.find((name) => !/^[a-z][a-z0-9_]*$/.test(name));
    const protectedName = names.find((name) => SYSTEM_TABLES.has(name));
    const conflict = names.find((name) => existing.has(name));
    return [duplicate && `Duplicate proposed table: ${duplicate}`, invalid && "Table names must be lower snake_case and start with a letter.", protectedName && `${protectedName} is a protected Emanduite system table.`, conflict && `${conflict} already exists in the current Blueprint.`].filter(Boolean) as string[];
  }, [proposal, existing]);

  useEffect(() => {
    if (!designing) return;
    const startedAt = Date.now();
    const timer = window.setInterval(() => setElapsed(Math.floor((Date.now() - startedAt) / 1000)), 500);
    return () => window.clearInterval(timer);
  }, [designing]);

  const updateTable = (id: string, update: (current: Table) => Table) => setProposal((current) => current ? { ...current, tables: current.tables.map((item) => item.id === id ? update(item) : item) } : current);
  const removeTable = (id: string) => setProposal((current) => current ? { ...current, tables: current.tables.filter((item) => item.id !== id).map((item) => ({ ...item, foreignKeys: item.foreignKeys.filter((foreignKey) => foreignKey.toTable !== current.tables.find((candidate) => candidate.id === id)?.name) })) } : current);
  const design = async () => {
    if (!prompt.trim()) return;
    const requestId = uuid();
    setElapsed(0); setDesigning(true); setProviderError(""); setNotice("");
    setConversation((current) => [...current,
      { id: uuid(), role: "user", message: prompt.trim() },
      { id: requestId, role: "system", state: "working", message: aiConfig ? `Preparing protected Blueprint context for ${aiConfig.model}…` : "Preparing local Blueprint context…" }
    ]);
    try {
      setConversation((current) => current.map((item) => item.id === requestId ? { ...item, message: aiConfig ? `Request sent to ${aiConfig.model}. Waiting for its database design…` : "Creating a deterministic local database design…" } : item));
      const next = aiConfig
        ? externalDesign(unwrap(await api.generateOpenAiCompatibleDesign({
            baseUrl: aiConfig.baseUrl, model: aiConfig.model, temperature: aiConfig.temperature,
            maxOutputTokens: aiConfig.maxOutputTokens, secretRef: aiConfig.apiKeySecretRef!, prompt,
            schemaContext: {
              projectName: session.blueprint.projectName,
              protectedTables: [...SYSTEM_TABLES],
              existingTables: session.blueprint.databases.main.tables.map((item) => ({ name: item.name, columns: item.columns.map((field) => ({ name: field.name, nativeType: field.nativeType })) }))
            }
          })))
        : createDesign(prompt);
      setProposal(next); setPlan(null); setPreviewError("");
      setConversation((current) => [...current.map((item) => item.id === requestId ? { ...item, state: undefined, message: "Response received and Blueprint safety checks passed." } : item), { id: uuid(), role: "assistant", message: `${next.summary} Proposed tables: ${next.tables.map((item) => item.name).join(", ")}.` }]);
    } catch (value) {
      const message = value instanceof Error ? value.message : "Unable to create an AI design proposal.";
      setProviderError(message);
      setConversation((current) => current.map((item) => item.id === requestId ? { ...item, state: "error", message: `Design request failed: ${message}` } : item));
    }
    finally { setDesigning(false); }
  };
  const operations = proposal?.tables.map((item) => ({ kind: "addTable" as const, operationId: uuid(), table: { ...item, name: cleanName(item.name) } })) ?? [];
  const preview = async () => {
    if (!proposal || errors.length) return;
    setPreviewError("");
    const next = await onPlan(operations);
    if (next) { setPlan(next); setNotice(""); }
    else setPreviewError("Preview could not be created. The exact desktop-runtime diagnostic is shown in the error banner above.");
  };
  const apply = async () => { if (!plan) return; setApplying(true); const result = await onApply(plan, null); setApplying(false); if (result) { setPlan(null); setNotice("Applied successfully. The Blueprint and SQLite schema now include the approved tables."); } };

  return <div className="page ai-page">
    <div className="page-heading"><div><span className="eyebrow">AI DATABASE DESIGNER</span><h1>Design first, apply only when approved</h1><p>Describe the app. Emanduite prepares a reviewable SQLite Blueprint and ERD without changing its system tables.</p></div><span className="ai-local-pill">{aiConfig ? `AI · ${aiConfig.model}` : "LOCAL DESIGN MODE"}</span></div>
    <section className="panel ai-prompt-panel"><div className="panel-title"><span>What are you building?</span><small>{aiConfig ? "Configured provider + Blueprint context" : "Blueprint context included"}</small></div><textarea value={prompt} onChange={(event) => setPrompt(event.target.value)} placeholder="Example: inventory app with products, suppliers, and stock movement history" /><div className="ai-prompt-footer"><span>Protected: mst_roles, mst_users, sys_resources, sys_permissions, sys_audit_logs</span><button className="primary" disabled={designing || !prompt.trim()} onClick={() => { void design(); }}>{designing ? `Designing… ${elapsed}s` : aiConfig ? "Design with AI" : "Design database"}</button></div>{providerError && <div className="inline-alert error">{providerError}</div>}</section>
    <section className="panel ai-conversation" aria-live="polite"><div className="panel-title"><span>AI conversation</span><small>{designing ? `working · ${elapsed}s` : aiConfig ? `model · ${aiConfig.model}` : "local designer"}</small></div>{conversation.length ? <div className="ai-conversation-list">{conversation.map((entry) => <article className={`ai-message ${entry.role} ${entry.state ?? ""}`} key={entry.id}><strong>{entry.role === "user" ? "You" : entry.role === "assistant" ? "AI designer" : "Process"}</strong><p>{entry.message}</p>{entry.state === "working" && <span className="ai-typing">Working…</span>}</article>)}</div> : <div className="ai-conversation-empty">Your request and each design step will appear here. API keys and sensitive database rows are never shown.</div>}<div className="panel-actions"><button className="secondary" disabled={designing || !conversation.length} onClick={() => setConversation([])}>Clear conversation</button></div></section>
    {!proposal && <section className="ai-empty"><strong>Start with an intent, not SQL</strong><span>The designer proposes only new business tables. Existing Blueprint tables are read as context and never renamed.</span></section>}
    {proposal && <div className="ai-layout"><section className="panel ai-blueprint"><div className="panel-title"><span>{proposal.title}</span><small>{proposal.tables.length} proposed tables</small></div><div className="ai-summary"><p>{proposal.summary}</p><ul>{proposal.assumptions.map((item) => <li key={item}>{item}</li>)}</ul></div>{errors.length > 0 && <div className="inline-alert error">{errors.map((item) => <div key={item}>{item}</div>)}</div>}{previewError && <div className="inline-alert error">{previewError}</div>}<div className="ai-table-list">{proposal.tables.map((item) => <article className="ai-table-editor" key={item.id}><header><input aria-label="Table name" value={item.name} onChange={(event) => updateTable(item.id, (current) => ({ ...current, name: event.target.value }))} /><button className="danger" onClick={() => removeTable(item.id)}>Remove</button></header><div className="ai-column-head"><span>Column</span><span>SQLite type</span><span>Rules</span></div>{item.columns.map((field) => <div className="ai-column-editor" key={field.id}><input aria-label={`${item.name} column`} value={field.name} onChange={(event) => updateTable(item.id, (current) => ({ ...current, columns: current.columns.map((candidate) => candidate.id === field.id ? { ...candidate, name: cleanName(event.target.value) } : candidate) }))} /><select value={field.nativeType} onChange={(event) => updateTable(item.id, (current) => ({ ...current, columns: current.columns.map((candidate) => candidate.id === field.id ? { ...candidate, nativeType: event.target.value, canonicalType: event.target.value.includes("INT") ? "integer" : event.target.value.includes("DECIMAL") ? "decimal" : event.target.value.includes("DATE") ? "dateTime" : "text" } : candidate) }))}>{["INTEGER", "TEXT", "DECIMAL(12,2)", "DATETIME", "BOOLEAN"].map((type) => <option key={type}>{type}</option>)}</select><span>{field.primaryKey ? "PK" : field.nullable ? "optional" : "required"}</span></div>)}{item.foreignKeys.map((foreignKey) => <p className="ai-relation" key={foreignKey.id}>{foreignKey.fromColumn} → {foreignKey.toTable}.{foreignKey.toColumn}</p>)}</article>)}</div><div className="panel-actions"><button className="secondary" onClick={() => { setProposal(null); setPlan(null); setNotice("Design cancelled. Nothing was changed."); }}>Cancel design</button><button className="primary" disabled={errors.length > 0 || !proposal.tables.length} onClick={() => { void preview(); }}>Preview apply</button></div></section><section className="panel ai-erd"><div className="panel-title"><span>Blueprint ERD preview</span><small>proposed only</small></div><div className="ai-erd-canvas">{proposal.tables.map((item) => <article className="ai-erd-node" key={item.id}><header>{cleanName(item.name)}</header>{item.columns.map((field) => <div key={field.id}><b>{field.primaryKey ? "◆" : "·"}</b><span>{field.name}</span><small>{field.nativeType}</small></div>)}</article>)}</div><div className="ai-erd-relations">{proposal.tables.flatMap((item) => item.foreignKeys.map((foreignKey) => <span key={foreignKey.id}>{cleanName(item.name)}.{foreignKey.fromColumn} → {foreignKey.toTable}.{foreignKey.toColumn}</span>))}</div></section></div>}
    {notice && <div className="inline-alert">{notice} {notice.startsWith("Applied") && <button className="secondary" onClick={onOpenSchema}>Open ERD</button>}</div>}
    {plan && <div className="modal-backdrop"><section className="migration-modal" role="dialog" aria-label="AI schema approval"><header><div><span className="eyebrow">APPROVE BLUEPRINT</span><h2>Review SQL before applying</h2></div><button onClick={() => setPlan(null)}>×</button></header><div className="plan-meta"><span>{plan.statements.length} statements</span><span>Non-destructive additions</span><span>SQLite</span></div><pre>{plan.sqlPreview}</pre><footer><button className="secondary" onClick={() => setPlan(null)}>Edit design</button><button className="primary" disabled={applying} onClick={() => { void apply(); }}>{applying ? "Applying..." : "Apply approved design"}</button></footer></section></div>}
  </div>;
}
