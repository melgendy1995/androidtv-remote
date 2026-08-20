import type { CaptureToast } from "../types";

export function CaptureBar({
  streaming,
  recording,
  toast,
  error,
  onSnap,
  onToggleRecord,
  onReveal,
}: {
  streaming: boolean;
  recording: boolean;
  toast: CaptureToast | null;
  error?: string;
  onSnap: () => void;
  onToggleRecord: () => void;
  onReveal: (path: string) => void;
}) {
  return (
    <>
      <div className="capture-bar">
        <button
          className="surface-btn"
          disabled={!streaming}
          title="Screenshot (S)"
          onClick={onSnap}
        >
          📸 Snap
        </button>
        <button
          className={recording ? "danger-btn" : "primary-btn"}
          disabled={!streaming}
          title="Toggle recording (R)"
          onClick={onToggleRecord}
        >
          {recording ? "⏹ Stop" : "⏺ Rec"}
        </button>
      </div>
      {toast ? (
        <div className="toast">
          <span>
            {toast.kind === "screenshot" ? "Screenshot" : "Recording"} saved:{" "}
            {toast.name}
          </span>
          <button onClick={() => onReveal(toast.path)}>Reveal</button>
        </div>
      ) : null}
      {error ? <div className="error-line">{error}</div> : null}
    </>
  );
}
