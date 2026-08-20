import type { NowPlaying } from "../types";

export function NowOnDevice({ now }: { now: NowPlaying | null }) {
  const title = now?.title || "Nothing playing";
  const sub = [now?.artist].filter(Boolean).join(" — ");
  return (
    <div className="now-card glass glass-card">
      <div className="now-art">📺</div>
      <div className="now-meta">
        <div className="now-title">{title}</div>
        {sub ? <div className="now-sub">{sub}</div> : null}
        <div className="now-app">{now?.label || now?.packageName || "—"}</div>
      </div>
    </div>
  );
}
