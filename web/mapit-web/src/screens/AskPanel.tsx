import { useState } from "react";
import { api } from "../api-client";
import { useAppState } from "../store";

export function AskPanel() {
  const [q, setQ] = useState("");
  const [history, setHistory] = useState<Array<{ q: string; answer: string }>>([]);
  const [loading, setLoading] = useState(false);
  const { dispatch } = useAppState();

  const handleAsk = async () => {
    if (!q.trim()) return;
    const question = q.trim();
    setQ("");
    setLoading(true);
    try {
      const res = await api.ask({ question });
      const answerText = `[${res.grounding_status}] ${res.answer}`;
      setHistory((prev) => [...prev, { q: question, answer: answerText }]);
    } catch (e: unknown) {
      const errorText = `Error: ${e instanceof Error ? e.message : "request failed"}`;
      setHistory((prev) => [...prev, { q: question, answer: errorText }]);
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="border-t border-mapit-border bg-mapit-surface h-1/2 flex flex-col">
      <div className="flex items-center justify-between px-4 py-2 border-b border-mapit-border">
        <span className="text-sm font-semibold text-mapit-text">Ask AI</span>
        <button
          className="text-mapit-muted hover:text-mapit-text text-xs"
          onClick={() => dispatch({ type: "SET_OVERLAY", overlay: null })}
        >
          ✕
        </button>
      </div>
      
      {/* Chat History */}
      <div className="flex-1 overflow-y-auto p-4 space-y-3">
        {history.length === 0 ? (
          <div className="text-center text-mapit-muted text-sm">
            Ask a question about the codebase!
          </div>
        ) : (
          history.map((item, index) => (
            <div key={index} className="space-y-2">
              <div className="flex items-start gap-2">
                <span className="text-xs font-bold text-mapit-accent">Q:</span>
                <span className="text-sm text-mapit-text">{item.q}</span>
              </div>
              <div className="flex items-start gap-2">
                <span className="text-xs font-bold text-mapit-success">A:</span>
                <div className="text-sm text-mapit-text whitespace-pre-wrap">{item.answer}</div>
              </div>
            </div>
          ))
        )}
      </div>

      {/* Input Area */}
      <div className="flex items-center gap-2 px-4 py-3 border-t border-mapit-border">
        <input
          type="text"
          placeholder="Ask about the codebase…"
          value={q}
          onChange={(e) => setQ(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && handleAsk()}
          className="flex-1 px-3 py-2 text-sm bg-mapit-bg border border-mapit-border rounded-lg
                     text-mapit-text placeholder-mapit-muted focus:outline-none focus:border-mapit-accent focus:ring-1 focus:ring-mapit-accent"
        />
        <button
          disabled={loading || !q.trim()}
          className="px-4 py-2 text-sm rounded-lg bg-mapit-accent text-white hover:opacity-90
                     disabled:opacity-50 transition-opacity flex items-center gap-1"
          onClick={handleAsk}
        >
          {loading ? (
            <div className="w-4 h-4 border-2 border-white border-t-transparent rounded-full animate-spin" />
          ) : (
            "Send"
          )}
        </button>
      </div>
    </div>
  );
}
