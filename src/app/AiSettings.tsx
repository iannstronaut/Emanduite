import { useEffect, useMemo, useState } from "react";
import type { BlueprintV1 } from "../contracts/blueprint";
import type { ProjectSession } from "../contracts/workspace";
import * as api from "../lib/tauri";

type Commit = (update: (blueprint: BlueprintV1) => BlueprintV1) => void;

interface AiConfig {
  provider: "openaiCompatible";
  baseUrl: string;
  model: string;
  maxOutputTokens: number;
  temperature: number;
  apiKeySecretRef?: string;
}

const defaults: AiConfig = { provider: "openaiCompatible", baseUrl: "https://api.openai.com/v1", model: "gpt-4.1-mini", maxOutputTokens: 1800, temperature: 0.2 };

function readConfig(blueprint: BlueprintV1): AiConfig {
  const candidate = blueprint.global.settings.ai;
  if (!candidate || typeof candidate !== "object" || Array.isArray(candidate)) return defaults;
  const value = candidate as Partial<AiConfig>;
  return { ...defaults, ...value, provider: "openaiCompatible" };
}

function unwrap<T>(value: { ok: true; data: T } | { ok: false; error: { message: string } }): T {
  if (!value.ok) throw new Error(value.error.message);
  return value.data;
}

export function AiSettings({ session, onCommit }: { session: ProjectSession; onCommit: Commit }) {
  const initial = useMemo(() => readConfig(session.blueprint), [session.blueprint]);
  const [config, setConfig] = useState<AiConfig>(initial);
  const [apiKey, setApiKey] = useState("");
  const [keySaved, setKeySaved] = useState(Boolean(initial.apiKeySecretRef));
  const [secretRef, setSecretRef] = useState(initial.apiKeySecretRef ?? "");
  const [models, setModels] = useState<string[]>([]);
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState("");
  const [error, setError] = useState("");

  useEffect(() => { setConfig(initial); setKeySaved(Boolean(initial.apiKeySecretRef)); setSecretRef(initial.apiKeySecretRef ?? ""); setApiKey(""); }, [initial]);

  const fetchModels = async (baseUrl: string, secretRef: string) => {
    const values = unwrap(await api.listOpenAiCompatibleModels(baseUrl, secretRef));
    setModels(values);
    if (values.length && !values.includes(config.model)) setConfig((value) => ({ ...value, model: values[0] }));
    setNotice(values.length ? `${values.length} models loaded from provider.` : "Provider returned no models.");
  };

  const save = async () => {
    setBusy(true); setError(""); setNotice("");
    try {
      const endpoint = new URL(config.baseUrl);
      if (!/^https?:$/.test(endpoint.protocol)) throw new Error("Base URL must start with http:// or https://");
      if (!config.model.trim()) throw new Error("Model is required");
      let apiKeySecretRef = config.apiKeySecretRef;
      if (apiKey.trim()) apiKeySecretRef = unwrap(await api.putSecret(session.blueprint.projectId, "ai.openai-compatible.api-key", apiKey.trim())).secretRef;
      const next = { ...config, baseUrl: config.baseUrl.replace(/\/+$/, ""), model: config.model.trim(), apiKeySecretRef };
      onCommit((blueprint) => ({ ...blueprint, global: { ...blueprint.global, settings: { ...blueprint.global.settings, ai: next } } }));
      setConfig(next); setApiKey(""); setKeySaved(Boolean(apiKeySecretRef)); setSecretRef(apiKeySecretRef ?? ""); setNotice("AI provider settings saved. API key is held in the OS keyring.");
      if (apiKeySecretRef) await fetchModels(next.baseUrl, apiKeySecretRef);
    } catch (value) { setError(value instanceof Error ? value.message : "Unable to save AI settings"); }
    finally { setBusy(false); }
  };

  const removeKey = async () => {
    if (!config.apiKeySecretRef) return;
    setBusy(true); setError("");
    try {
      unwrap(await api.deleteSecret(config.apiKeySecretRef));
      const next = { ...config, apiKeySecretRef: undefined };
      onCommit((blueprint) => ({ ...blueprint, global: { ...blueprint.global, settings: { ...blueprint.global.settings, ai: next } } }));
      setConfig(next); setKeySaved(false); setSecretRef(""); setNotice("Stored API key removed from the OS keyring.");
    } catch (value) { setError(value instanceof Error ? value.message : "Unable to remove API key"); }
    finally { setBusy(false); }
  };

  return <div className="page ai-settings-page"><div className="page-heading"><div><span className="eyebrow">SETTINGS / AI PROVIDER</span><h1>OpenAI-compatible connection</h1><p>Use OpenAI or a compatible gateway. The key never enters the Blueprint file.</p></div><span className={`provider-state ${keySaved ? "configured" : ""}`}>{keySaved ? "KEY CONFIGURED" : "KEY REQUIRED"}</span></div><section className="panel ai-settings-panel"><div className="panel-title"><span>Provider connection</span><small>per project</small></div><div className="ai-settings-form"><label>Protocol<input value="OpenAI Compatible / Chat Completions" readOnly /></label><label>Base URL<input value={config.baseUrl} onChange={(event) => setConfig((value) => ({ ...value, baseUrl: event.target.value }))} placeholder="https://api.openai.com/v1" /></label><label>Model <small>{models.length ? `${models.length} models fetched from this provider.` : "Save provider settings to fetch available models."}</small><div className="model-picker">{models.length ? <select value={config.model} onChange={(event) => setConfig((value) => ({ ...value, model: event.target.value }))}>{!models.includes(config.model) && <option value={config.model}>{config.model}</option>}{models.map((model) => <option value={model} key={model}>{model}</option>)}</select> : <input value={config.model} onChange={(event) => setConfig((value) => ({ ...value, model: event.target.value }))} placeholder="your-model-id" />}<button className="secondary" disabled={busy || !secretRef} onClick={() => { if (secretRef) { setBusy(true); setError(""); void fetchModels(config.baseUrl, secretRef).catch((value) => setError(value instanceof Error ? value.message : "Unable to fetch models")).finally(() => setBusy(false)); } }}>Refresh</button></div></label><label>API key <small>{keySaved ? "A key is stored securely. Enter a new one only to replace it." : "Stored in the OS keyring after save."}</small><input value={apiKey} onChange={(event) => setApiKey(event.target.value)} type="password" autoComplete="off" placeholder={keySaved ? "••••••••" : "sk-..."} /></label><div className="ai-settings-row"><label>Temperature<input type="number" min="0" max="2" step="0.1" value={config.temperature} onChange={(event) => setConfig((value) => ({ ...value, temperature: Number(event.target.value) }))} /></label><label>Max output tokens<input type="number" min="256" max="8000" step="128" value={config.maxOutputTokens} onChange={(event) => setConfig((value) => ({ ...value, maxOutputTokens: Number(event.target.value) }))} /></label></div></div><div className="ai-settings-note"><b>How it will be used</b><span>AI Design will send only the design request plus a compact schema context after an external provider is enabled. It will still return a reviewable Blueprint and require your approval before migration.</span></div><div className="panel-actions">{keySaved && <button className="secondary danger" disabled={busy} onClick={() => { void removeKey(); }}>Remove stored key</button>}<button className="primary" disabled={busy} onClick={() => { void save(); }}>{busy ? "Saving..." : "Save AI settings"}</button></div>{notice && <div className="inline-alert">{notice}</div>}{error && <div className="inline-alert error">{error}</div>}</section></div>;
}
