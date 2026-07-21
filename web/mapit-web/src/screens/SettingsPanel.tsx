import { useState, useEffect, useRef } from "react";
import { useAppState } from "../store";
import { api } from "../api-client";
import type { AppConfig } from "../types";
import { ConfirmDialog } from "../components/ConfirmDialog";

// ─── Status types ──────────────────────────────────────────────────────────────

type ConnStatus = "untested" | "testing" | "ok" | "slow" | "error";

interface ConnResult {
  message: string;
  latencyMs?: number;
  models: string[];
}

interface ChatTestResult {
  ok: boolean;
  response?: string;
  error?: string;
  latencyMs?: number;
}

// ─── Toast notification ────────────────────────────────────────────────────────

interface ToastMsg {
  id: number;
  text: string;
  kind: "ok" | "warn" | "error";
}

function useToasts() {
  const [toasts, setToasts] = useState<ToastMsg[]>([]);
  const counter = useRef(0);

  const push = (
    text: string,
    kind: ToastMsg["kind"] = "ok",
    durationMs = 4000,
  ) => {
    const id = ++counter.current;
    setToasts((prev) => [...prev, { id, text, kind }]);
    setTimeout(
      () => setToasts((prev) => prev.filter((t) => t.id !== id)),
      durationMs,
    );
  };

  return { toasts, push };
}

function ToastStack({ toasts }: { toasts: ToastMsg[] }) {
  if (toasts.length === 0) return null;
  return (
    <div className="fixed top-14 left-1/2 -translate-x-1/2 z-[60] flex flex-col gap-2 pointer-events-none">
      {toasts.map((t) => (
        <div
          key={t.id}
          className={`px-4 py-2.5 rounded-lg shadow-2xl border text-sm font-medium text-center min-w-[240px] max-w-sm animate-in fade-in slide-in-from-top-2 ${
            t.kind === "ok"
              ? "bg-mapit-success/15 border-mapit-success/40 text-mapit-success"
              : t.kind === "warn"
                ? "bg-mapit-warning/15 border-mapit-warning/40 text-mapit-warning"
                : "bg-mapit-danger/15 border-mapit-danger/40 text-mapit-danger"
          }`}
        >
          {t.kind === "ok" ? "✓ " : t.kind === "warn" ? "⚠ " : "✕ "}
          {t.text}
        </div>
      ))}
    </div>
  );
}

// ─── Connection status dot ─────────────────────────────────────────────────────

function ConnDot({ status }: { status: ConnStatus }) {
  const cfg = {
    untested: { color: "bg-mapit-muted", label: "Not tested" },
    testing: { color: "bg-mapit-accent animate-pulse", label: "Testing…" },
    ok: { color: "bg-mapit-success", label: "Connected" },
    slow: { color: "bg-mapit-warning", label: "Connected (slow)" },
    error: { color: "bg-mapit-danger", label: "Failed" },
  }[status];
  return (
    <span className="flex items-center gap-1.5 text-xs text-mapit-muted">
      <span className={`w-2 h-2 rounded-full inline-block ${cfg.color}`} />
      {cfg.label}
    </span>
  );
}

// ─── Latency badge ─────────────────────────────────────────────────────────────

function LatencyBadge({ ms }: { ms?: number }) {
  if (!ms) return null;
  const color =
    ms < 500
      ? "text-mapit-success"
      : ms < 2000
        ? "text-mapit-warning"
        : "text-mapit-danger";
  return <span className={`text-xs font-mono ${color}`}>{ms}ms</span>;
}

// ─── Main component ────────────────────────────────────────────────────────────

export function SettingsPanel() {
  const { state, dispatch } = useAppState();
  const { toasts, push } = useToasts();

  // ── Config form state ──
  const [config, setConfig] = useState<AppConfig | null>(null);
  const [provider, setProvider] = useState("ollama");
  const [baseUrl, setBaseUrl] = useState("");
  const [apiKey, setApiKey] = useState("");
  const [model, setModel] = useState("");
  const [saving, setSaving] = useState(false);

  // ── Connection test state ──
  const [connStatus, setConnStatus] = useState<ConnStatus>("untested");
  const [connResult, setConnResult] = useState<ConnResult | null>(null);
  const [fetchedModels, setFetchedModels] = useState<string[]>([]);

  // ── Test chat state ──
  const [testMsg, setTestMsg] = useState("Respond with exactly one word: OK");
  const [chatResult, setChatResult] = useState<ChatTestResult | null>(null);
  const [chatTesting, setChatTesting] = useState(false);

  // ── Re-annotate state ──
  const [reannotating, setReannotating] = useState(false);
  const [showConfirm, setShowConfirm] = useState(false);

  // ── Load config on mount ──
  useEffect(() => {
    api
      .config()
      .then((cfg) => {
        setConfig(cfg);
        setProvider(cfg.provider);
        setBaseUrl(cfg.base_url);
        setModel(cfg.model);
      })
      .catch(console.error);
  }, []);

  // ── Helpers ──
  const saveSettings = async (): Promise<boolean> => {
    setSaving(true);
    try {
      const upd: Record<string, string> = {};
      if (provider !== config?.provider) upd.provider = provider;
      if (baseUrl !== config?.base_url) upd.base_url = baseUrl;
      if (model !== config?.model) upd.model = model;
      if (apiKey) upd.api_key = apiKey;

      const updated = await api.updateConfig(upd);
      setConfig(updated);
      setApiKey("");
      return true;
    } catch (e) {
      push(`Save failed: ${e instanceof Error ? e.message : "error"}`, "error");
      return false;
    } finally {
      setSaving(false);
    }
  };

  const handleSave = async () => {
    const ok = await saveSettings();
    if (ok) push("Settings saved", "ok");
  };

  // ── Test connection ──
  const handleTestConnection = async () => {
    // Save first so the server tests with current values
    setSaving(true);
    try {
      const upd: Record<string, string> = {};
      if (provider !== config?.provider) upd.provider = provider;
      if (baseUrl !== config?.base_url) upd.base_url = baseUrl;
      if (model !== config?.model) upd.model = model;
      if (apiKey) upd.api_key = apiKey;
      const updated = await api.updateConfig(upd);
      setConfig(updated);
      setApiKey("");
    } catch {
      push("Failed to save settings before testing", "error");
      setSaving(false);
      return;
    }
    setSaving(false);

    setConnStatus("testing");
    setConnResult(null);
    setFetchedModels([]);

    try {
      const res = await api.testConnection();
      if (res.ok) {
        const ms = res.latency_ms ?? 0;
        setConnStatus(ms > 3000 ? "slow" : "ok");
        setConnResult({
          message: res.message,
          latencyMs: ms,
          models: res.models,
        });
        setFetchedModels(res.models);
        push(
          ms > 3000
            ? `Connected but slow (${ms}ms) — ${res.models.length} models`
            : `${res.message} (${ms}ms)`,
          ms > 3000 ? "warn" : "ok",
        );
        // If current model not in fetched list, suggest the first one
        if (res.models.length > 0 && !res.models.includes(model)) {
          setModel(res.models[0]);
        }
      } else {
        setConnStatus("error");
        setConnResult({ message: res.message, models: [] });
        push(res.message || "Connection failed", "error", 6000);
      }
    } catch (e) {
      setConnStatus("error");
      const msg = e instanceof Error ? e.message : "Connection failed";
      setConnResult({ message: msg, models: [] });
      push(msg, "error", 6000);
    }
  };

  // ── Test chat ──
  const handleTestChat = async () => {
    if (!testMsg.trim()) return;
    // Save current settings first so the server uses latest values
    try {
      const saved = await saveSettings();
      if (!saved) {
        setChatTesting(false);
        return;
      }
    } catch (e) {
      const msg = e instanceof Error ? e.message : "Failed to save settings";
      setChatResult({ ok: false, error: msg });
      push(msg, "error", 6000);
      setChatTesting(false);
      return;
    }
    setChatTesting(true);
    setChatResult(null);
    try {
      const res = await api.testChat(testMsg.trim());
      setChatResult({
        ok: res.ok,
        response: res.response,
        error: res.error,
        latencyMs: res.latency_ms,
      });
      if (res.ok) {
        push(`Model responded in ${res.latency_ms}ms`, "ok");
      } else {
        push(res.error ?? "Model did not respond", "error", 6000);
      }
    } catch (e) {
      const msg = e instanceof Error ? e.message : "Request failed";
      setChatResult({ ok: false, error: msg });
      push(msg, "error", 6000);
    } finally {
      setChatTesting(false);
    }
  };

  // ── Re-annotate ──
  const handleReannotate = async () => {
    setShowConfirm(false);
    setReannotating(true);
    // Show progress bar immediately
    dispatch({
      type: "SET_BG_PROGRESS",
      progress: {
        phase: "ai_enrichment",
        current: 0,
        total: state.project?.symbol_count ?? 0,
        label: "Re-annotating all symbols…",
      },
    });
    try {
      await api.annotate(true, true);
      push("Re-annotation started — watch the progress bar", "ok");
    } catch (e) {
      dispatch({ type: "SET_BG_PROGRESS", progress: null });
      push(
        `Annotation failed: ${e instanceof Error ? e.message : "error"}`,
        "error",
      );
    } finally {
      setReannotating(false);
    }
  };

  const placeholderUrl =
    provider === "ollama"
      ? "http://localhost:11434"
      : "https://openrouter.ai/api";

  return (
    <div className="flex flex-col h-full bg-mapit-bg">
      <ToastStack toasts={toasts} />

      {showConfirm && (
        <ConfirmDialog
          title="Re-annotate everything?"
          message={`Re-run AI enrichment on all ${state.project?.symbol_count ?? "?"} symbols with the current model. This may take a while${provider !== "ollama" ? " and use API credits" : ""}.`}
          confirmLabel="Re-annotate all"
          onConfirm={handleReannotate}
          onCancel={() => setShowConfirm(false)}
        />
      )}

      {/* Header */}
      <div className="flex items-center justify-between px-4 py-2 bg-mapit-surface border-b border-mapit-border">
        <h2 className="text-sm font-semibold text-mapit-text">Settings</h2>
        <button
          type="button"
          className="text-xs text-mapit-muted hover:text-mapit-text px-2 py-1 rounded focus:ring-2 focus:ring-mapit-accent focus:outline-none"
          onClick={() =>
            dispatch({ type: "SET_SCREEN", screen: "system_overview" })
          }
        >
          ← Back to graph
        </button>
      </div>

      <div className="flex-1 overflow-y-auto p-4 space-y-4 min-h-0">
        {/* ── API Connection ── */}
        <Section label="API Connection">
          {config === null ? (
            <p className="text-xs text-mapit-muted">Loading…</p>
          ) : (
            <div className="space-y-3">
              {/* Provider select */}
              <Field label="Provider">
                <select
                  value={provider}
                  onChange={(e) => {
                    setProvider(e.target.value);
                    setConnStatus("untested");
                    setFetchedModels([]);
                  }}
                  className="w-full px-3 py-1.5 text-sm bg-mapit-bg border border-mapit-border rounded text-mapit-text focus:outline-none focus:border-mapit-accent"
                >
                  <option value="ollama">🦙 Ollama (local)</option>
                  <option value="openai-compatible">
                    🌐 OpenAI-Compatible (remote)
                  </option>
                </select>
              </Field>

              {/* Base URL */}
              <Field
                label={provider === "ollama" ? "Ollama URL" : "API Base URL"}
              >
                <input
                  type="text"
                  value={baseUrl}
                  onChange={(e) => {
                    setBaseUrl(e.target.value);
                    setConnStatus("untested");
                  }}
                  placeholder={placeholderUrl}
                  className="w-full px-3 py-1.5 text-sm bg-mapit-bg border border-mapit-border rounded text-mapit-text placeholder-mapit-muted font-mono focus:outline-none focus:border-mapit-accent"
                />
                {provider === "openai-compatible" && (
                  <p className="text-xs text-mapit-muted mt-1">
                    Do NOT include{" "}
                    <code className="font-mono bg-mapit-surface2 px-1 rounded">
                      /v1
                    </code>{" "}
                    — e.g.{" "}
                    <code className="font-mono bg-mapit-surface2 px-1 rounded">
                      https://openrouter.ai/api
                    </code>
                  </p>
                )}
              </Field>

              {/* API Key (openai-compatible only) */}
              {provider === "openai-compatible" && (
                <Field
                  label={
                    <span>
                      API Key{" "}
                      {config.api_key_set ? (
                        <span className="text-mapit-success font-normal normal-case">
                          (set ✓)
                        </span>
                      ) : (
                        <span className="text-mapit-warning font-normal normal-case">
                          (not set)
                        </span>
                      )}
                    </span>
                  }
                >
                  <input
                    type="password"
                    value={apiKey}
                    onChange={(e) => setApiKey(e.target.value)}
                    placeholder={
                      config.api_key_set
                        ? "Leave blank to keep current key"
                        : "sk-…"
                    }
                    className="w-full px-3 py-1.5 text-sm bg-mapit-bg border border-mapit-border rounded text-mapit-text placeholder-mapit-muted focus:outline-none focus:border-mapit-accent"
                  />
                </Field>
              )}

              {/* Test connection button + status */}
              <div className="flex items-center gap-3 flex-wrap pt-1">
                <button
                  type="button"
                  disabled={connStatus === "testing" || saving}
                  onClick={handleTestConnection}
                  className="flex items-center gap-2 px-4 py-1.5 text-sm rounded border border-mapit-border bg-mapit-surface2 text-mapit-text hover:border-mapit-accent/60 hover:bg-mapit-surface transition-all focus:ring-2 focus:ring-mapit-accent focus:outline-none disabled:opacity-50 disabled:cursor-not-allowed"
                >
                  {connStatus === "testing" || saving ? (
                    <span className="w-3.5 h-3.5 border-2 border-mapit-accent border-t-transparent rounded-full animate-spin inline-block" />
                  ) : (
                    <svg
                      width="14"
                      height="14"
                      viewBox="0 0 24 24"
                      fill="none"
                      stroke="currentColor"
                      strokeWidth="2"
                    >
                      <path d="M5 12h14M12 5l7 7-7 7" />
                    </svg>
                  )}
                  {saving
                    ? "Saving…"
                    : connStatus === "testing"
                      ? "Testing…"
                      : "Test Connection"}
                </button>
                <ConnDot status={connStatus} />
                {connResult?.latencyMs && (
                  <LatencyBadge ms={connResult.latencyMs} />
                )}
              </div>

              {/* Connection result */}
              {connResult && (
                <div
                  className={`rounded-lg border px-3 py-2 text-xs space-y-1 ${
                    connStatus === "ok" || connStatus === "slow"
                      ? "bg-mapit-success/8 border-mapit-success/30 text-mapit-success"
                      : "bg-mapit-danger/8 border-mapit-danger/30 text-mapit-danger"
                  }`}
                >
                  <p className="font-medium">{connResult.message}</p>
                  {connResult.models.length > 0 && (
                    <p className="text-mapit-muted">
                      Models: {connResult.models.slice(0, 5).join(", ")}
                      {connResult.models.length > 5 &&
                        ` +${connResult.models.length - 5} more`}
                    </p>
                  )}
                </div>
              )}

              {/* Model selection */}
              <Field label="Model">
                {fetchedModels.length > 0 ? (
                  <select
                    value={model}
                    onChange={(e) => setModel(e.target.value)}
                    className="w-full px-3 py-1.5 text-sm bg-mapit-bg border border-mapit-border rounded text-mapit-text focus:outline-none focus:border-mapit-accent"
                  >
                    {fetchedModels.map((m) => (
                      <option key={m} value={m}>
                        {m}
                      </option>
                    ))}
                  </select>
                ) : (
                  <div className="flex gap-2">
                    <input
                      type="text"
                      value={model}
                      onChange={(e) => setModel(e.target.value)}
                      placeholder={
                        provider === "ollama"
                          ? "qwen2.5-coder:7b"
                          : "gpt-4o-mini"
                      }
                      className="flex-1 px-3 py-1.5 text-sm bg-mapit-bg border border-mapit-border rounded text-mapit-text placeholder-mapit-muted focus:outline-none focus:border-mapit-accent"
                    />
                    <span className="text-xs text-mapit-muted self-center">
                      (test connection to fetch list)
                    </span>
                  </div>
                )}
              </Field>

              {/* Save button */}
              <div className="flex items-center gap-2 pt-1 border-t border-mapit-border">
                <button
                  type="button"
                  disabled={saving}
                  onClick={handleSave}
                  className="px-4 py-1.5 text-sm rounded bg-mapit-accent text-white hover:opacity-90 transition-opacity focus:ring-2 focus:ring-mapit-accent focus:outline-none disabled:opacity-50"
                >
                  {saving ? "Saving…" : "Save Settings"}
                </button>
                <span className="text-xs text-mapit-muted">
                  (settings are also auto-saved when you test)
                </span>
              </div>
            </div>
          )}
        </Section>

        {/* ── Test Message ── */}
        <Section label="Test Message">
          <p className="text-xs text-mapit-muted mb-3">
            Send a live message through the configured model to verify the full
            pipeline works end-to-end.
          </p>

          <div className="space-y-2">
            <Field label="Message">
              <div className="flex gap-2">
                <input
                  type="text"
                  value={testMsg}
                  onChange={(e) => setTestMsg(e.target.value)}
                  onKeyDown={(e) =>
                    e.key === "Enter" && !chatTesting && handleTestChat()
                  }
                  placeholder="Respond with exactly one word: OK"
                  className="flex-1 px-3 py-1.5 text-sm bg-mapit-bg border border-mapit-border rounded text-mapit-text placeholder-mapit-muted focus:outline-none focus:border-mapit-accent"
                />
                <button
                  type="button"
                  disabled={chatTesting || !testMsg.trim()}
                  onClick={handleTestChat}
                  className="flex items-center gap-2 px-4 py-1.5 text-sm rounded bg-mapit-surface2 border border-mapit-border text-mapit-text hover:border-mapit-accent/60 transition-all focus:ring-2 focus:ring-mapit-accent focus:outline-none disabled:opacity-50 disabled:cursor-not-allowed whitespace-nowrap"
                >
                  {chatTesting ? (
                    <span className="w-3.5 h-3.5 border-2 border-mapit-accent border-t-transparent rounded-full animate-spin inline-block" />
                  ) : (
                    <svg
                      width="14"
                      height="14"
                      viewBox="0 0 24 24"
                      fill="none"
                      stroke="currentColor"
                      strokeWidth="2"
                    >
                      <line x1="22" y1="2" x2="11" y2="13" />
                      <polygon points="22 2 15 22 11 13 2 9 22 2" />
                    </svg>
                  )}
                  {chatTesting ? "Sending…" : "Send"}
                </button>
              </div>
            </Field>

            {/* Quick test presets */}
            <div className="flex flex-wrap gap-1.5">
              {[
                "Respond with exactly one word: OK",
                "What is 2+2? Answer in one number only.",
                "List 3 colors, comma-separated, nothing else.",
              ].map((preset) => (
                <button
                  key={preset}
                  type="button"
                  onClick={() => setTestMsg(preset)}
                  className="text-xs px-2 py-0.5 rounded bg-mapit-surface border border-mapit-border text-mapit-muted hover:text-mapit-text hover:border-mapit-accent/40 transition-colors"
                >
                  {preset.length > 30 ? preset.slice(0, 28) + "…" : preset}
                </button>
              ))}
            </div>

            {/* Chat result */}
            {chatResult && (
              <div
                className={`rounded-lg border text-xs overflow-hidden ${
                  chatResult.ok
                    ? "border-mapit-success/30 bg-mapit-success/8"
                    : "border-mapit-danger/30 bg-mapit-danger/8"
                }`}
              >
                <div
                  className={`flex items-center justify-between px-3 py-1.5 border-b ${
                    chatResult.ok
                      ? "border-mapit-success/20"
                      : "border-mapit-danger/20"
                  }`}
                >
                  <span
                    className={`font-semibold ${chatResult.ok ? "text-mapit-success" : "text-mapit-danger"}`}
                  >
                    {chatResult.ok ? "✓ Model responded" : "✕ Model error"}
                  </span>
                  {chatResult.latencyMs && (
                    <LatencyBadge ms={chatResult.latencyMs} />
                  )}
                </div>
                <div className="px-3 py-2">
                  {chatResult.ok && chatResult.response ? (
                    <pre className="text-mapit-text font-mono whitespace-pre-wrap break-all leading-relaxed">
                      {chatResult.response}
                    </pre>
                  ) : (
                    <p className="text-mapit-danger">{chatResult.error}</p>
                  )}
                </div>
              </div>
            )}
          </div>
        </Section>

        {/* ── Project info ── */}
        <Section label="Project">
          <div className="space-y-1 text-xs text-mapit-muted">
            <p className="text-mapit-text font-mono break-all">
              {state.project?.project_root ?? "—"}
            </p>
            <div className="grid grid-cols-2 gap-x-4 gap-y-1 mt-2">
              <span>{state.project?.file_count ?? "?"} files</span>
              <span>{state.project?.symbol_count ?? "?"} symbols</span>
              <span>{state.project?.edge_count ?? "?"} edges</span>
              <span>
                {state.project?.ai_annotation_coverage_pct != null
                  ? `${state.project.ai_annotation_coverage_pct.toFixed(0)}% annotated`
                  : "0% annotated"}
              </span>
            </div>
            {state.project?.languages && state.project.languages.length > 0 && (
              <p className="mt-1">
                Languages: {state.project.languages.join(", ")}
              </p>
            )}
          </div>
        </Section>

        {/* ── AI Enrichment ── */}
        <Section label="AI Enrichment">
          <p className="text-xs text-mapit-muted mb-2">
            Re-run AI summarization and flaw detection on all symbols. Existing
            annotations are kept until explicitly overwritten.
          </p>
          <button
            type="button"
            disabled={reannotating}
            onClick={() => setShowConfirm(true)}
            className="px-3 py-1.5 text-xs rounded bg-mapit-accent text-white hover:opacity-90 disabled:opacity-50 transition-opacity focus:ring-2 focus:ring-mapit-accent focus:outline-none"
          >
            {reannotating ? "Annotating…" : "Re-annotate everything"}
          </button>
          {reannotating && (
            <p className="text-xs text-mapit-muted mt-2">
              Running in background — nodes update live via WebSocket.
            </p>
          )}
        </Section>
      </div>
    </div>
  );
}

// ─── Helpers ───────────────────────────────────────────────────────────────────

function Section({
  label,
  children,
}: {
  label: string;
  children: React.ReactNode;
}) {
  return (
    <div className="bg-mapit-surface border border-mapit-border rounded-lg p-4">
      <h3 className="text-xs uppercase tracking-widest text-mapit-muted mb-3 font-semibold">
        {label}
      </h3>
      {children}
    </div>
  );
}

function Field({
  label,
  children,
}: {
  label: React.ReactNode;
  children: React.ReactNode;
}) {
  return (
    <div>
      <label className="block text-xs text-mapit-muted mb-1">{label}</label>
      {children}
    </div>
  );
}
