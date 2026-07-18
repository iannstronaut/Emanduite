import { useMemo, useRef, type PointerEvent, type WheelEvent } from "react";
import type { Table } from "../contracts/blueprint";
import type { ExplorerLayout, ProjectSession } from "../contracts/workspace";

interface Props { session: ProjectSession; layout: ExplorerLayout; onLayout: (layout: ExplorerLayout) => void; }

const position = (index: number) => ({ x: (index % 3) * 320, y: Math.floor(index / 3) * 300 });

export function SchemaExplorer({ session, layout, onLayout }: Props) {
  const tables = session.blueprint.databases.main.tables;
  const drag = useRef<{ x: number; y: number; panX: number; panY: number } | null>(null);
  const selected = tables.find((table) => table.id === layout.selectedTableId) ?? null;
  const indexes = useMemo(() => new Map(tables.map((table, index) => [table.name, index])), [tables]);

  const pointerDown = (event: PointerEvent<HTMLDivElement>) => {
    if ((event.target as HTMLElement).closest(".table-node")) return;
    event.currentTarget.setPointerCapture(event.pointerId);
    drag.current = { x: event.clientX, y: event.clientY, panX: layout.panX, panY: layout.panY };
  };
  const pointerMove = (event: PointerEvent<HTMLDivElement>) => {
    if (!drag.current) return;
    onLayout({ ...layout, panX: drag.current.panX + event.clientX - drag.current.x, panY: drag.current.panY + event.clientY - drag.current.y });
  };
  const wheel = (event: WheelEvent<HTMLDivElement>) => {
    event.preventDefault();
    onLayout({ ...layout, zoom: Math.max(.35, Math.min(2.5, layout.zoom - event.deltaY * .001)) });
  };
  const select = (table: Table) => onLayout({ ...layout, selectedTableId: table.id });

  if (tables.length === 0) return <div className="page"><div className="empty-large"><span className="eyebrow">SCHEMA EXPLORER</span><h1>No schema loaded</h1><p>Open Database, test the connection, then introspect the SQLite schema.</p></div></div>;

  return <div className="schema-page">
    <div className="schema-toolbar"><div><span className="eyebrow">SCHEMA EXPLORER</span><strong>{tables.length} tables</strong></div><div className="zoom-controls"><button onClick={() => onLayout({ ...layout, zoom: Math.max(.35, layout.zoom - .1) })}>−</button><span>{Math.round(layout.zoom * 100)}%</span><button onClick={() => onLayout({ ...layout, zoom: Math.min(2.5, layout.zoom + .1) })}>+</button><button onClick={() => onLayout({ panX: 32, panY: 32, zoom: 1, selectedTableId: layout.selectedTableId })}>Reset</button></div></div>
    <div className="schema-workspace">
      <div className="schema-canvas" onPointerDown={pointerDown} onPointerMove={pointerMove} onPointerUp={() => { drag.current = null; }} onWheel={wheel}>
        <div className="schema-world" style={{ transform: `translate(${layout.panX}px, ${layout.panY}px) scale(${layout.zoom})` }}>
          <svg className="relation-layer" width="960" height={Math.max(600, Math.ceil(tables.length / 3) * 300)}>{tables.flatMap((table, sourceIndex) => table.foreignKeys.map((fk) => {
            const targetIndex = indexes.get(fk.toTable); if (targetIndex === undefined) return null;
            const source = position(sourceIndex); const target = position(targetIndex);
            return <line key={fk.id} x1={source.x + 130} y1={source.y + 90} x2={target.x + 130} y2={target.y + 90} />;
          }))}</svg>
          {tables.map((table, index) => { const at = position(index); return <button className={selected?.id === table.id ? "table-node selected" : "table-node"} style={{ left: at.x, top: at.y }} key={table.id} onClick={() => select(table)}>
            <header><span>{table.name}</span><small>{table.columns.length} cols</small></header>
            <div>{table.columns.slice(0, 8).map((column) => <span className="column-row" key={column.id}><i>{column.primaryKey ? "PK" : table.foreignKeys.some((fk) => fk.fromColumn === column.name) ? "FK" : ""}</i><b>{column.name}</b><em>{column.canonicalType}</em></span>)}</div>
            {table.columns.length > 8 && <footer>+{table.columns.length - 8} columns</footer>}
          </button>; })}
        </div>
      </div>
      <aside className="properties-panel">{selected ? <><div className="panel-title"><span>{selected.name}</span><small>read-only</small></div><dl><dt>Stable ID</dt><dd>{selected.id}</dd><dt>Columns</dt><dd>{selected.columns.length}</dd><dt>Foreign keys</dt><dd>{selected.foreignKeys.length}</dd><dt>Indexes</dt><dd>{selected.indexes.length}</dd></dl><h3>Indexes</h3>{selected.indexes.map((index) => <div className="property-item" key={index.id}><strong>{index.name}</strong><span>{index.columns.join(", ") || "expression"}{index.unique ? " · unique" : ""}</span></div>)}</> : <div className="empty-state"><strong>Select a table</strong><span>Inspect columns, relations, and indexes without modifying the database.</span></div>}</aside>
    </div>
  </div>;
}
