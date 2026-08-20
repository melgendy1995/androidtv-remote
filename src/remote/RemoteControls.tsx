import { useRef } from "react";

const HOLD_MS = 600;
const HOME_DOUBLE_MS = 250;

export function RemoteControls({
  connected,
  onCommand,
}: {
  connected: boolean;
  onCommand: (command: string) => void;
}) {
  const holdTimer = useRef<number | null>(null);
  const held = useRef(false);
  const homeTimer = useRef<number | null>(null);
  const homeArmed = useRef(false);

  const startSelect = () => {
    held.current = false;
    holdTimer.current = window.setTimeout(() => {
      held.current = true;
      onCommand("select_hold");
    }, HOLD_MS);
  };

  const endSelect = () => {
    if (holdTimer.current) window.clearTimeout(holdTimer.current);
    holdTimer.current = null;
    if (!held.current) onCommand("select");
  };

  const homeTap = () => {
    if (homeArmed.current) {
      if (homeTimer.current) window.clearTimeout(homeTimer.current);
      homeTimer.current = null;
      homeArmed.current = false;
      onCommand("home_double");
      return;
    }
    homeArmed.current = true;
    homeTimer.current = window.setTimeout(() => {
      homeArmed.current = false;
      homeTimer.current = null;
      onCommand("home");
    }, HOME_DOUBLE_MS);
  };

  return (
    <div className={`remote${connected ? "" : " disabled"}`}>
      <div className="row" style={{ width: "100%", justifyContent: "space-between", marginBottom: 6 }}>
        <button className="surface-btn" onClick={() => onCommand("power")} title="Power">
          ⏻ Power
        </button>
        <button className="surface-btn" onClick={() => onCommand("vol_mute")} title="Mute">
          🔇 Mute
        </button>
      </div>

      <div className="dpad">
        <div className="dpad-ring glass glass-circle" />
        <button className="dpad-btn up" onClick={() => onCommand("up")}>
          ▲
        </button>
        <button className="dpad-btn left" onClick={() => onCommand("left")}>
          ◀
        </button>
        <button className="dpad-btn right" onClick={() => onCommand("right")}>
          ▶
        </button>
        <button className="dpad-btn down" onClick={() => onCommand("down")}>
          ▼
        </button>
        <button
          className="dpad-center glass glass-circle"
          onMouseDown={startSelect}
          onMouseUp={endSelect}
          onMouseLeave={() => {
            if (holdTimer.current) window.clearTimeout(holdTimer.current);
          }}
        >
          ●
        </button>
      </div>

      <button
        className="surface-btn prominent"
        onClick={() => onCommand("menu")}
      >
        ↩ Back
      </button>

      <div className="row">
        <button className="surface-btn" onClick={() => onCommand("vol_down")} title="Volume Down">
          🔉 Vol-
        </button>
        <button className="primary-btn" onClick={() => onCommand("play_pause")}>
          ⏯
        </button>
        <button className="surface-btn" onClick={() => onCommand("vol_up")} title="Volume Up">
          🔊 Vol+
        </button>
      </div>

      <div className="row">
        <button className="surface-btn" onClick={() => onCommand("previous")}>
          ⏮
        </button>
        <button className="surface-btn" onClick={() => onCommand("next")}>
          ⏭
        </button>
      </div>

      <div className="row">
        <button className="surface-btn" onClick={homeTap}>
          ⌂ TV
        </button>
        <button className="surface-btn" onClick={() => onCommand("home_hold")}>
          ⌂⌂ Apps
        </button>
      </div>
    </div>
  );
}
