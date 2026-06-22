import { useState } from "react";
import { useAppState } from "../store";
import { api } from "../api-client";

export function SettingsPanel() {
  const { state, dispatch } = useAppState();
  const p = state.project;
  const [saving, setSaving] = useState(false);

  return (
    <div className="flex flex-col h-full bg-mapit-bg">
      <div className="flex items-center justify-between px-4 py-2 bg-mapit-surface border-b border-mapit-border">
        <h2 className="text-sm font-semibold text-mapit-text">Settings</h2>
        <button
          className="text-mapit-muted hover:text-mapit-text"
          onClick={() => dispatch({ type: "SET_SCREEN", screen: "system_overview" })}
        >
          Back
        </button>
      </div>

      <div className="flex-1 overflow-y-auto p-4 space-y-4 text-sm">
        <Section label="Provider">
          <div className="flex gap-2">
            <span className="text-mapit-text">{p?.provider ?? "—"}</span>
            <span className="text-mapit-muted">/</span>
            <span className="text-mapit-text">{p?.model ?? "—"}</span>
          </div>
          <p className="text-xs text-mapit-muted mt-1">
            Change with <code className="bg-mapit-surface px-1 rounded">mapit config set-provider</code>
          </p>
        </Section>

        <Section label="Project">
          <p className="text-mapit-text break-all">{p?.project_root}</p>
          <div className="grid grid-cols-2 gap-2 mt-2 text-xs text-mapit-muted">
            <span>{p?.file_count ?? "?"} files</span>
            <span>{p?.symbol_count ?? "?"} symbols</span>
            <span>{p?.edge_count ?? "?"} edges</span>
            <span>{p?.ai_annotation_coverage_pct ?? "?"}% annotated</span>
          </div>
        </Section>

        <Section label="Re-annotate">
          <p className="text-xs text-mapit-muted mb-2">
            Re-run AI enrichment with the current model for all symbols.
          </p>
          <button
            disabled={saving}
            className="px-3 py-1.5 text-xs rounded bg-mapit-accent text-white hover:opacity-90
                       disabled:opacity-50 transition-opacity"
            onClick={async () => {
              setSaving(true);
              try {
                await api.annotate(true, true);
                // Refresh project info and flaws
                const project = await api.project();
                dispatch({ type: "SET_PROJECT", project });
                const flawsRes = await api.flaws();
                dispatch({ type: "SET_FLAWS", flaws: flawsRes.flaws });
              } finally {
                setSaving(false);
              }
            }}
          >
            {saving ? "Annotating…" : "Re-annotate everything"}
          </button>
        </Section>
      </div>
    </div>
  );
}

function Section({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="bg-mapit-surface border border-mapit-border rounded-lg p-3">
      <h3 className="text-xs uppercase tracking-wider text-mapit-muted mb-2">{label}</h3>
      {children}
    </div>
  );
}
