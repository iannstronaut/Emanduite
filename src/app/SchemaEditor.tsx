import { useMemo, useState } from "react";
import type { CanonicalType, Column, Table } from "../contracts/blueprint";
import type { ApplyResult, MigrationPlan, SchemaOperation } from "../contracts/schema-editor";
import type { ProjectSession } from "../contracts/workspace";

interface Props {
  session: ProjectSession;
  onPlan: (operations: SchemaOperation[]) => Promise<MigrationPlan | undefined>;
  onApply: (plan: MigrationPlan, token?: string | null) => Promise<ApplyResult | undefined>;
}

const uuid = () => crypto.randomUUID();
const nativeToCanonical = (value: string): CanonicalType => {
  const type = value.toUpperCase();
  if (type.includes("INT")) return "integer";
  if (type.includes("REAL") || type.includes("FLOA") || type.includes("DOUB")) return "real";
  if (type.includes("DEC") || type.includes("NUM")) return "decimal";
  if (type.includes("BLOB")) return "bytes";
  if (type.includes("BOOL")) return "boolean";
  if (type.includes("DATE") && type.includes("TIME")) return "dateTime";
  if (type === "DATE") return "date";
  if (type.includes("JSON")) return "json";
  return "text";
};

export function SchemaEditor({ session, onPlan, onApply }: Props) {
  const tables = session.blueprint.databases.main.tables;
  const [operations, setOperations] = useState<SchemaOperation[]>([]);
  const [redo, setRedo] = useState<SchemaOperation[]>([]);
  const [tableName, setTableName] = useState("");
  const [selectedTable, setSelectedTable] = useState(tables[0]?.name ?? "");
  const [columnName, setColumnName] = useState("");
  const [columnType, setColumnType] = useState("TEXT");
  const [nullable, setNullable] = useState(true);
  const [selectedColumn, setSelectedColumn] = useState("");
  const [renamedColumn, setRenamedColumn] = useState("");
  const [targetTable, setTargetTable] = useState("");
  const [fromColumn, setFromColumn] = useState("");
  const [toColumn, setToColumn] = useState("");
  const [plan, setPlan] = useState<MigrationPlan | null>(null);
  const [confirmation, setConfirmation] = useState("");
  const [applying, setApplying] = useState(false);
  const currentTable = tables.find((table) => table.name === selectedTable);
  const target = tables.find((table) => table.name === targetTable);
  const destructive = operations.some((item) => item.kind === "dropTable" || item.kind === "dropColumn" || item.kind === "dropForeignKey");
  const summary = useMemo(() => operations.map((item) => item.kind.replace(/([A-Z])/g, " $1").toLowerCase()), [operations]);

  const push = (operation: SchemaOperation) => { setOperations((items) => [...items, operation]); setRedo([]); setPlan(null); };
  const addTable = () => {
    if (!tableName.trim()) return;
    const table: Table = { id: uuid(), name: tableName.trim(), columns: [{ id: uuid(), name: "id", nativeType: "INTEGER", canonicalType: "integer", nullable: false, primaryKey: true }], foreignKeys: [], indexes: [] };
    push({ kind: "addTable", operationId: uuid(), table }); setTableName("");
  };
  const addColumn = () => {
    if (!selectedTable || !columnName.trim()) return;
    const column: Column = { id: uuid(), name: columnName.trim(), nativeType: columnType, canonicalType: nativeToCanonical(columnType), nullable, primaryKey: false };
    push({ kind: "addColumn", operationId: uuid(), tableName: selectedTable, column }); setColumnName("");
  };
  const preview = async () => { const value = await onPlan(operations); if (value) { setPlan(value); setConfirmation(""); } };
  const apply = async () => {
    if (!plan) return; setApplying(true);
    const result = await onApply(plan, plan.destructive ? plan.confirmationToken : null);
    setApplying(false);
    if (result) { setOperations([]); setRedo([]); setPlan(null); setConfirmation(""); }
  };

  return <div className="page editor-page">
    <div className="page-heading"><div><span className="eyebrow">VISUAL SCHEMA EDITOR</span><h1>Operation workspace</h1><p>Draft changes first. SQLite is modified only after server-generated preview and confirmation.</p></div><div className="editor-actions"><button className="secondary" disabled={!operations.length} onClick={() => { const last = operations.at(-1); if (last) { setOperations((items) => items.slice(0, -1)); setRedo((items) => [last, ...items]); setPlan(null); } }}>Undo</button><button className="secondary" disabled={!redo.length} onClick={() => { const next = redo[0]; setRedo((items) => items.slice(1)); setOperations((items) => [...items, next]); }}>Redo</button><button className="secondary danger" disabled={!operations.length} onClick={() => { setOperations([]); setRedo([]); setPlan(null); }}>Discard</button><button className="primary" disabled={!operations.length} onClick={() => { void preview(); }}>Preview migration</button></div></div>
    <div className="editor-grid">
      <section className="panel editor-tools"><div className="panel-title"><span>Schema operations</span><small>main SQLite</small></div>
        <div className="tool-block"><h3>Add table</h3><div className="inline-form"><input value={tableName} onChange={(event) => setTableName(event.target.value)} placeholder="table_name" /><button onClick={addTable}>Add</button></div></div>
        <div className="tool-block"><h3>Target table</h3><select value={selectedTable} onChange={(event) => { setSelectedTable(event.target.value); setSelectedColumn(""); }}><option value="">Select table</option>{tables.map((table) => <option key={table.id}>{table.name}</option>)}</select></div>
        <div className="tool-block"><h3>Add column</h3><input value={columnName} onChange={(event) => setColumnName(event.target.value)} placeholder="column_name" /><div className="inline-form"><select value={columnType} onChange={(event) => setColumnType(event.target.value)}>{["TEXT", "INTEGER", "REAL", "DECIMAL(10,2)", "BOOLEAN", "BLOB", "DATE", "DATETIME", "JSON"].map((type) => <option key={type}>{type}</option>)}</select><label className="check"><input type="checkbox" checked={nullable} onChange={(event) => setNullable(event.target.checked)} /> nullable</label><button onClick={addColumn}>Add</button></div></div>
        <div className="tool-block"><h3>Drop column</h3><div className="inline-form"><select value={selectedColumn} onChange={(event) => setSelectedColumn(event.target.value)}><option value="">Select column</option>{currentTable?.columns.map((column) => <option key={column.id}>{column.name}</option>)}</select><button className="danger" disabled={!selectedColumn} onClick={() => push({ kind: "dropColumn", operationId: uuid(), tableName: selectedTable, columnName: selectedColumn })}>Drop</button></div></div>
        <div className="tool-block"><h3>Rename column</h3><div className="inline-form"><select value={selectedColumn} onChange={(event) => setSelectedColumn(event.target.value)}><option value="">Select column</option>{currentTable?.columns.map((column) => <option key={column.id}>{column.name}</option>)}</select><input value={renamedColumn} onChange={(event) => setRenamedColumn(event.target.value)} placeholder="new_name" /><button disabled={!selectedColumn || !renamedColumn.trim()} onClick={() => { push({ kind: "renameColumn", operationId: uuid(), tableName: selectedTable, from: selectedColumn, to: renamedColumn.trim() }); setRenamedColumn(""); }}>Rename</button></div></div>
        <div className="tool-block"><h3>Add relation</h3><select value={targetTable} onChange={(event) => { setTargetTable(event.target.value); setToColumn(""); }}><option value="">Referenced table</option>{tables.filter((table) => table.name !== selectedTable).map((table) => <option key={table.id}>{table.name}</option>)}</select><div className="inline-form"><select value={fromColumn} onChange={(event) => setFromColumn(event.target.value)}><option value="">From column</option>{currentTable?.columns.map((column) => <option key={column.id}>{column.name}</option>)}</select><select value={toColumn} onChange={(event) => setToColumn(event.target.value)}><option value="">To column</option>{target?.columns.map((column) => <option key={column.id}>{column.name}</option>)}</select><button disabled={!fromColumn || !toColumn || !targetTable} onClick={() => push({ kind: "addForeignKey", operationId: uuid(), tableName: selectedTable, foreignKey: { id: uuid(), fromColumn, toTable: targetTable, toColumn, onUpdate: "CASCADE", onDelete: "RESTRICT" } })}>Link</button></div></div>
        <div className="tool-block"><h3>Existing relations</h3>{currentTable?.foreignKeys.length ? currentTable.foreignKeys.map((foreignKey) => <div className="relation-operation" key={foreignKey.id}><code>{foreignKey.fromColumn} → {foreignKey.toTable}.{foreignKey.toColumn}</code><button className="danger" onClick={() => push({ kind: "dropForeignKey", operationId: uuid(), tableName: selectedTable, foreignKeyId: foreignKey.id })}>Drop</button></div>) : <small>No relation on this table</small>}</div>
        <button className="danger wide" disabled={!selectedTable} onClick={() => push({ kind: "dropTable", operationId: uuid(), tableName: selectedTable })}>Drop selected table</button>
      </section>
      <section className="panel operation-queue"><div className="panel-title"><span>Pending operation list</span><small>{operations.length}</small></div>{operations.length === 0 ? <div className="empty-state"><strong>No pending operations</strong><span>Every edit becomes an explicit, reviewable operation.</span></div> : operations.map((operation, index) => <div className={operation.kind.startsWith("drop") ? "operation destructive" : "operation"} key={operation.operationId}><b>{index + 1}</b><span>{summary[index]}</span><code>{"tableName" in operation ? operation.tableName : operation.kind === "addTable" ? operation.table.name : ""}</code></div>)}<div className="queue-footer"><span>{destructive ? "Destructive confirmation required" : "Non-destructive draft"}</span><strong>{operations.length} changes</strong></div></section>
    </div>
    {plan && <div className="modal-backdrop"><section className="migration-modal" role="dialog" aria-label="Migration preview"><header><div><span className="eyebrow">MIGRATION PREVIEW</span><h2>{plan.destructive ? "Destructive changes detected" : "Ready for review"}</h2></div><button onClick={() => setPlan(null)}>×</button></header><div className="plan-meta"><span>Plan {plan.id}</span><span>Backup required</span><span>{plan.statements.length} statements</span></div><pre>{plan.sqlPreview}</pre>{plan.destructive && <label>Type <b>APPLY</b> to confirm<input value={confirmation} onChange={(event) => setConfirmation(event.target.value)} autoFocus /></label>}<footer><button className="secondary" onClick={() => setPlan(null)}>Cancel</button><button className={plan.destructive ? "primary destructive-button" : "primary"} disabled={applying || (plan.destructive && confirmation !== "APPLY")} onClick={() => { void apply(); }}>{applying ? "Applying…" : "Apply migration"}</button></footer></section></div>}
  </div>;
}
