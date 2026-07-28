import { useEffect, useMemo, useState, type ElementType } from "react";
import { createPortal } from "react-dom";
import { Archive, Bag, Book1, Calendar, Card, Category, Chart, ClipboardText, Code, Data, Diagram, DocumentText, Edit, Folder, Global, Home2, LoginCurve, Magicpen, Message, Monitor, Notification, Profile2User, Receipt, SecuritySafe, Setting2, Shop, ShoppingBag, Sun1, TaskSquare, Truck, User, Wallet } from "iconsax-reactjs";

const ICONS: Array<{ key: string; label: string; Icon: ElementType }> = [
  { key: "LayoutDashboard", label: "Dashboard", Icon: Category },
  { key: "House", label: "Home", Icon: Home2 },
  { key: "Database", label: "Database", Icon: Data },
  { key: "GitBranch", label: "Relations", Icon: Diagram },
  { key: "Archive", label: "Archive", Icon: Archive },
  { key: "Package", label: "Packages", Icon: Bag },
  { key: "Briefcase", label: "Work", Icon: Bag },
  { key: "CalendarDays", label: "Calendar", Icon: Calendar },
  { key: "BarChart3", label: "Reports", Icon: Chart },
  { key: "ListTodo", label: "Tasks", Icon: TaskSquare },
  { key: "ClipboardList", label: "Clipboard", Icon: ClipboardText },
  { key: "FileText", label: "Documents", Icon: DocumentText },
  { key: "MessageSquare", label: "Messages", Icon: Message },
  { key: "Bell", label: "Notifications", Icon: Notification },
  { key: "Folder", label: "Folder", Icon: Folder },
  { key: "Users", label: "Users", Icon: Profile2User },
  { key: "UserRound", label: "User", Icon: User },
  { key: "ShieldCheck", label: "Security", Icon: SecuritySafe },
  { key: "Globe2", label: "Global", Icon: Global },
  { key: "Code2", label: "Code", Icon: Code },
  { key: "FilePenLine", label: "Edit", Icon: Edit },
  { key: "WandSparkles", label: "AI design", Icon: Magicpen },
  { key: "Monitor", label: "Monitor", Icon: Monitor },
  { key: "LogIn", label: "Login", Icon: LoginCurve },
  { key: "Sun", label: "Utility", Icon: Sun1 }
  , { key: "Truck", label: "Shipping", Icon: Truck }
  , { key: "Store", label: "Store", Icon: Shop }
  , { key: "ShoppingBag", label: "Shopping", Icon: ShoppingBag }
  , { key: "WalletCards", label: "Wallet", Icon: Wallet }
  , { key: "CreditCard", label: "Payments", Icon: Card }
  , { key: "Receipt", label: "Invoices", Icon: Receipt }
  , { key: "BookOpen", label: "Library", Icon: Book1 }
  , { key: "Settings", label: "Settings", Icon: Setting2 }
];

export function MenuIconPicker({ value, onChange }: { value?: string; onChange: (value: string) => void }) {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const selected = ICONS.find((item) => item.key === value) ?? ICONS[0];
  const available = useMemo(() => ICONS.filter((item) => `${item.key} ${item.label}`.toLowerCase().includes(query.trim().toLowerCase())), [query]);

  useEffect(() => {
    if (!open) return;
    const close = (event: KeyboardEvent) => { if (event.key === "Escape") setOpen(false); };
    window.addEventListener("keydown", close);
    return () => window.removeEventListener("keydown", close);
  }, [open]);

  return <>
    <button type="button" className="entity-icon-trigger" onClick={() => { setQuery(""); setOpen(true); }} aria-haspopup="dialog" aria-expanded={open}>
      <selected.Icon size={18} variant="Linear" /><span><b>{selected.label}</b><small>{selected.key}</small></span><em>Choose icon</em>
    </button>
    {open && createPortal(<div className="app-dialog-backdrop icon-picker-backdrop" onMouseDown={() => setOpen(false)}>
      <section className="icon-picker-dialog" role="dialog" aria-modal="true" aria-label="Choose menu icon" onMouseDown={(event) => event.stopPropagation()}>
        <header><div><span className="eyebrow">ENTITY MENU</span><h2>Choose a sidebar icon</h2><p>Search the available menu icons, then select one for this entity.</p></div><button className="icon-button" type="button" aria-label="Close icon picker" onClick={() => setOpen(false)}>×</button></header>
        <label className="icon-search">Search icons<input autoFocus value={query} onChange={(event) => setQuery(event.target.value)} placeholder="Search database, user, security…" /></label>
        <div className="icon-picker-grid">{available.length ? available.map((item) => <button type="button" className={item.key === selected.key ? "active" : ""} aria-pressed={item.key === selected.key} onClick={() => { onChange(item.key); setOpen(false); }} key={item.key}><item.Icon size={23} variant={item.key === selected.key ? "Bold" : "Linear"} /><span>{item.label}</span><small>{item.key}</small></button>) : <div className="icon-picker-empty">No icon matches “{query}”.</div>}</div>
        <footer><button className="secondary" type="button" onClick={() => setOpen(false)}>Cancel</button></footer>
      </section>
    </div>, document.body)}
  </>;
}
