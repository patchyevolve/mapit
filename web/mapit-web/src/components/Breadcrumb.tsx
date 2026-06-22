import { useAppState } from "../store";

export function Breadcrumb() {
  const { state, dispatch } = useAppState();
  const crumbs = state.breadcrumb;

  if (crumbs.length === 0) return null;

  const handleCrumbClick = (i: number) => {
    dispatch({ type: "SET_BREADCRUMB", breadcrumb: crumbs.slice(0, i + 1) });
    dispatch({ type: "SET_SCREEN", screen: "system_overview" });
  };

  const handleCollapse = () => {
    dispatch({ type: "SET_BREADCRUMB", breadcrumb: [] });
    dispatch({ type: "SET_SCREEN", screen: "system_overview" });
  };

  return (
    <nav className="flex items-center gap-1 text-sm text-mapit-muted px-4 py-2 bg-mapit-surface border-b border-mapit-border">
      {crumbs.map((cr, i) => (
        <span key={i} className="flex items-center gap-1">
          {i > 0 && <span className="mx-1">›</span>}
          {cr.node_id ? (
            <button
              className="hover:text-mapit-accent transition-colors"
              onClick={() => handleCrumbClick(i)}
            >
              {cr.label}
            </button>
          ) : (
            <span>{cr.label}</span>
          )}
        </span>
      ))}
      <button
        className="ml-auto text-xs text-mapit-muted hover:text-mapit-text"
        onClick={handleCollapse}
      >
        collapse
      </button>
    </nav>
  );
}
