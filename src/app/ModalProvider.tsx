import { createContext, useCallback, useContext, useEffect, useRef, useState, type ReactNode } from "react";

type DialogTone = "info" | "success" | "error" | "danger";

interface DialogOptions {
  title: string;
  description: string;
  tone?: DialogTone;
  confirmLabel?: string;
  cancelLabel?: string;
}

interface ActiveDialog extends Required<Omit<DialogOptions, "tone" | "cancelLabel">> {
  tone: DialogTone;
  cancelLabel: string;
  resolve: (accepted: boolean) => void;
}

interface ModalContextValue {
  confirm: (options: DialogOptions) => Promise<boolean>;
  inform: (options: Omit<DialogOptions, "cancelLabel">) => Promise<void>;
}

const ModalContext = createContext<ModalContextValue | null>(null);
const standaloneModal: ModalContextValue = { confirm: async () => true, inform: async () => undefined };

export function ModalProvider({ children }: { children: ReactNode }) {
  const [dialog, setDialog] = useState<ActiveDialog | null>(null);
  const cancelButton = useRef<HTMLButtonElement>(null);

  const confirm = useCallback((options: DialogOptions) => new Promise<boolean>((resolve) => {
    setDialog({ tone: options.tone ?? "info", confirmLabel: options.confirmLabel ?? "Continue", cancelLabel: options.cancelLabel ?? "Cancel", ...options, resolve });
  }), []);

  const inform = useCallback(async (options: Omit<DialogOptions, "cancelLabel">) => {
    await confirm({ ...options, confirmLabel: options.confirmLabel ?? "Got it", cancelLabel: "" });
  }, [confirm]);

  const close = useCallback((accepted: boolean) => {
    setDialog((current) => { current?.resolve(accepted); return null; });
  }, []);

  useEffect(() => {
    if (!dialog) return;
    const closeOnEscape = (event: KeyboardEvent) => { if (event.key === "Escape") close(false); };
    window.addEventListener("keydown", closeOnEscape);
    window.requestAnimationFrame(() => (dialog.cancelLabel ? cancelButton.current : null)?.focus());
    return () => window.removeEventListener("keydown", closeOnEscape);
  }, [close, dialog]);

  return <ModalContext.Provider value={{ confirm, inform }}>
    {children}
    {dialog && <div className="app-dialog-backdrop" onMouseDown={() => close(false)}>
      <section className={`app-dialog app-dialog-${dialog.tone}`} role={dialog.tone === "danger" || dialog.tone === "error" ? "alertdialog" : "dialog"} aria-modal="true" aria-labelledby="app-dialog-title" aria-describedby="app-dialog-description" onMouseDown={(event) => event.stopPropagation()}>
        <div className="app-dialog-icon" aria-hidden="true">{dialog.tone === "success" ? "✓" : dialog.tone === "error" || dialog.tone === "danger" ? "!" : "i"}</div>
        <div className="app-dialog-copy"><h2 id="app-dialog-title">{dialog.title}</h2><p id="app-dialog-description">{dialog.description}</p></div>
        <div className="app-dialog-actions">{dialog.cancelLabel && <button className="secondary" ref={cancelButton} onClick={() => close(false)}>{dialog.cancelLabel}</button>}<button className={dialog.tone === "danger" || dialog.tone === "error" ? "primary destructive-button" : "primary"} onClick={() => close(true)}>{dialog.confirmLabel}</button></div>
      </section>
    </div>}
  </ModalContext.Provider>;
}

export function useModal() {
  const context = useContext(ModalContext);
  return context ?? standaloneModal;
}
