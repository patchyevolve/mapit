interface ConfirmDialogProps {
  title: string;
  message: string;
  confirmLabel: string;
  onConfirm: () => void;
  onCancel: () => void;
  danger?: boolean;
}

export function ConfirmDialog({ title, message, confirmLabel, onConfirm, onCancel, danger = false }: ConfirmDialogProps) {
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60">
      <div className="bg-mapit-surface border border-mapit-border rounded-lg p-6 max-w-sm w-full shadow-2xl mx-4">
        <h3 className="text-sm font-semibold text-mapit-text mb-2">{title}</h3>
        <p className="text-sm text-mapit-muted mb-4">{message}</p>
        <div className="flex justify-end gap-2">
          <button
            type="button"
            onClick={onCancel}
            className="px-3 py-1.5 text-sm rounded border border-mapit-border text-mapit-text hover:border-mapit-accent/50 transition-colors focus:ring-2 focus:ring-mapit-accent focus:outline-none"
          >
            Cancel
          </button>
          <button
            type="button"
            onClick={onConfirm}
            className={`px-3 py-1.5 text-sm rounded text-white transition-opacity hover:opacity-90 focus:ring-2 focus:ring-mapit-accent focus:outline-none ${danger ? "bg-mapit-danger" : "bg-mapit-accent"}`}
          >
            {confirmLabel}
          </button>
        </div>
      </div>
    </div>
  );
}
