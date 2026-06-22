export function LoadingScreen() {
  return (
    <div className="flex flex-col items-center justify-center min-h-screen bg-mapit-bg gap-4">
      <div className="w-8 h-8 border-2 border-mapit-accent border-t-transparent rounded-full animate-spin" />
      <p className="text-mapit-muted text-sm">Connecting to mapit…</p>
    </div>
  );
}
