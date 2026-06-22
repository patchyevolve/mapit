import { useAppState } from "../store";

export function LoadingScreen() {
  const { state } = useAppState();
  const p = state.project;

  return (
    <div className="flex flex-col items-center justify-center h-full bg-mapit-bg gap-6">
      {/* Logo */}
      <div className="text-4xl font-bold bg-gradient-to-r from-mapit-accent to-mapit-success bg-clip-text text-transparent select-none tracking-tight">
        mapit
      </div>

      {/* Animated pulse rings */}
      <div className="relative w-16 h-16 flex items-center justify-center">
        <div className="absolute w-16 h-16 rounded-full border-2 border-mapit-accent/20 animate-ping" />
        <div
          className="absolute w-12 h-12 rounded-full border-2 border-mapit-accent/40 animate-ping"
          style={{ animationDelay: "0.15s" }}
        />
        <div className="w-6 h-6 border-2 border-mapit-accent border-t-transparent rounded-full animate-spin" />
      </div>

      {/* Status */}
      {p ? (
        <div className="text-center space-y-1.5">
          <p className="text-sm font-mono text-mapit-text">
            {p.project_root.split("/").slice(-2).join("/")}
          </p>
          <p className="text-xs text-mapit-muted">
            {p.file_count} files · {p.symbol_count} symbols · loading graph…
          </p>
          <div className="flex justify-center gap-1.5 mt-2">
            {p.languages?.map((lang) => (
              <span
                key={lang}
                className="text-xs bg-mapit-surface border border-mapit-border rounded px-2 py-0.5 text-mapit-muted font-mono"
              >
                {lang}
              </span>
            ))}
          </div>
        </div>
      ) : (
        <p className="text-sm text-mapit-muted animate-pulse">
          Connecting to mapit server…
        </p>
      )}
    </div>
  );
}
