import { useAppState } from "../store";
import type { AppScreen, Node } from "../types";

function screenForBreadcrumbDepth(
  crumbs: { label: string; node_id?: string }[],
  allNodes: Map<string, Node>,
): AppScreen {
  if (crumbs.length === 0) return "system_overview";
  const last = crumbs[crumbs.length - 1];
  if (!last.node_id) return "system_overview";
  const node = allNodes.get(last.node_id);
  if (node?.type === "feature") return "expanded_feature";
  if (node?.type === "file") return "expanded_file";
  return "system_overview";
}

export function Breadcrumb() {
  const { state, dispatch } = useAppState();
  const crumbs = state.breadcrumb;

  if (crumbs.length === 0) return null;

  const navigateTo = (newCrumbs: typeof crumbs) => {
    dispatch({ type: "SET_BREADCRUMB", breadcrumb: newCrumbs });
    dispatch({ type: "SET_OVERLAY", overlay: null });
    dispatch({
      type: "SET_SCREEN",
      screen: screenForBreadcrumbDepth(newCrumbs, state.allNodes),
    });
  };

  const handleCrumbClick = (i: number) => {
    navigateTo(crumbs.slice(0, i + 1));
  };

  const handleCollapse = () => {
    navigateTo([]);
  };

  return (
    <nav className="flex items-center gap-1 text-sm text-mapit-muted px-4 py-2 bg-mapit-surface border-b border-mapit-border shrink-0">
      <button
        type="button"
        className="hover:text-mapit-accent transition-colors focus:ring-2 focus:ring-mapit-accent focus:outline-none rounded"
        onClick={handleCollapse}
      >
        Overview
      </button>
      {crumbs.map((cr, i) => (
        <span key={i} className="flex items-center gap-1">
          <span className="mx-1">›</span>
          {i < crumbs.length - 1 && cr.node_id ? (
            <button
              type="button"
              className="hover:text-mapit-accent transition-colors focus:ring-2 focus:ring-mapit-accent focus:outline-none rounded"
              onClick={() => handleCrumbClick(i)}
            >
              {cr.label}
            </button>
          ) : (
            <span className="text-mapit-text">{cr.label}</span>
          )}
        </span>
      ))}
      <button
        type="button"
        className="ml-auto text-xs text-mapit-muted hover:text-mapit-text focus:ring-2 focus:ring-mapit-accent focus:outline-none rounded px-1"
        onClick={handleCollapse}
      >
        collapse
      </button>
    </nav>
  );
}
