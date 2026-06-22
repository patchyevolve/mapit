import { useAppState } from "../store";

export function MapProgressScreen() {
  const { state } = useAppState();
  const progress = state.mapProgress;

  const pct = progress && progress.total > 0
    ? Math.round((progress.current / progress.total) * 100)
    : null;

  return (
    <div className="flex flex-col items-center justify-center min-h-screen bg-mapit-bg gap-4">
      <div className="w-8 h-8 border-2 border-mapit-accent border-t-transparent rounded-full animate-spin" />
      <p className="text-mapit-text text-lg font-semibold">
        {progress?.phase === "ai_enrichment"
          ? "AI enrichment in progress…"
          : "Mapping your codebase…"}
      </p>
      {pct !== null && (
        <div className="w-64">
          <div className="flex justify-between text-xs text-mapit-muted mb-1">
            <span>{progress!.current} / {progress!.total}</span>
            <span>{pct}%</span>
          </div>
          <div className="w-full h-2 bg-mapit-surface rounded-full overflow-hidden">
            <div
              className="h-full bg-mapit-accent rounded-full transition-all duration-300"
              style={{ width: `${pct}%` }}
            />
          </div>
        </div>
      )}
      <p className="text-mapit-muted text-sm">
        {progress?.currentFile || (
          progress?.phase === "structural"
            ? "Analyzing source files…"
            : "Summarizing symbols…"
        )}
      </p>
    </div>
  );
}
