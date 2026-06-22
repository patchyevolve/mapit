import { useState, useRef, useEffect } from "react";
import { useAppState } from "../store";
import { api } from "../api-client";
import type { SearchResult, NodeType } from "../types";

export function SearchBar() {
  const { dispatch } = useAppState();
  const [q, setQ] = useState("");
  const [results, setResults] = useState<SearchResult[]>([]);
  const [open, setOpen] = useState(false);
  const [loading, setLoading] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const handler = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, []);

  useEffect(() => {
    const debounceTimer = setTimeout(() => {
      if (!q.trim()) {
        setResults([]);
        setOpen(false);
        return;
      }
      handleSearch();
    }, 300);
    return () => clearTimeout(debounceTimer);
  }, [q]);

  const handleSearch = async () => {
    if (!q.trim()) return;
    setLoading(true);
    setOpen(true);
    try {
      const res = await api.search(q.trim());
      setResults(res.results);
    } catch (e) {
      console.error("search failed", e);
    } finally {
      setLoading(false);
    }
  };

  const handleSelect = (r: SearchResult) => {
    setOpen(false);
    setQ("");
    setResults([]);
    dispatch({ type: "SET_OVERLAY", overlay: null });
    const nodeType = r.node.type as NodeType;
    switch (nodeType) {
      case "function":
      case "type":
      case "macro":
      case "global":
        dispatch({
          type: "SET_OVERLAY",
          overlay: { kind: "function_detail", node_id: r.node.id },
        });
        break;
      case "file":
      case "feature":
        dispatch({
          type: "SET_BREADCRUMB",
          breadcrumb: [{ label: r.node.name, node_id: r.node.id }],
        });
        dispatch({ type: "SET_SCREEN", screen: "system_overview" });
        break;
      case "external":
        dispatch({
          type: "SET_OVERLAY",
          overlay: { kind: "external_detail", node_id: r.node.id },
        });
        break;
      case "module":
        dispatch({
          type: "SET_OVERLAY",
          overlay: { kind: "function_detail", node_id: r.node.id },
        });
        break;
    }
  };

  return (
    <div ref={ref} className="relative">
      <input
        type="text"
        placeholder="Search symbols, files…"
        value={q}
        onChange={(e) => setQ(e.target.value)}
        onKeyDown={(e) => e.key === "Enter" && handleSearch()}
        onFocus={() => results.length > 0 && setOpen(true)}
        className="w-64 px-3 py-1.5 text-sm bg-mapit-bg border border-mapit-border rounded
                   text-mapit-text placeholder-mapit-muted focus:outline-none focus:border-mapit-accent"
      />
      {open && (
        <div className="absolute top-full left-0 right-0 mt-1 bg-mapit-surface border border-mapit-border rounded shadow-lg max-h-80 overflow-y-auto z-50">
          {loading && (
            <div className="px-3 py-2 text-xs text-mapit-muted">Searching…</div>
          )}
          {!loading && results.length === 0 && q && (
            <div className="px-3 py-2 text-xs text-mapit-muted">No results</div>
          )}
          {results.map((r) => (
            <button
              key={r.node.id}
              className="w-full text-left px-3 py-2 hover:bg-mapit-bg transition-colors"
              onClick={() => handleSelect(r)}
            >
              <div className="flex items-center gap-2">
                <span className="text-xs font-mono text-mapit-accent">{r.node.type}</span>
                <span className="text-sm text-mapit-text">{r.node.name}</span>
                <span className="text-xs text-mapit-muted ml-auto">{r.match_reason}</span>
              </div>
              {r.node.file_path && (
                <p className="text-xs text-mapit-muted truncate mt-0.5">{r.node.file_path}</p>
              )}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
