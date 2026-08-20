import { useEffect, useRef } from "react";

const HOME_DOUBLE_MS = 250;

type Options = {
  enabled: boolean;
  sheetOpen: boolean;
  streaming: boolean;
  onCommand: (command: string) => void;
  onScreenshot: () => void;
  onToggleRecord: () => void;
};

function isTextTarget(target: EventTarget | null) {
  if (!(target instanceof HTMLElement)) return false;
  const tag = target.tagName;
  return (
    tag === "INPUT" ||
    tag === "TEXTAREA" ||
    target.isContentEditable
  );
}

export function useRemoteKeys({
  enabled,
  sheetOpen,
  streaming,
  onCommand,
  onScreenshot,
  onToggleRecord,
}: Options) {
  const returnHeld = useRef(false);
  const homeTimer = useRef<number | null>(null);
  const homeArmed = useRef(false);

  useEffect(() => {
    const fireHome = () => {
      homeTimer.current = null;
      homeArmed.current = false;
      onCommand("home");
    };

    const onDown = (e: KeyboardEvent) => {
      if (sheetOpen || isTextTarget(e.target)) return;
      if (e.metaKey || e.ctrlKey || e.altKey || e.shiftKey) return;

      if (e.code === "Backquote") {
        e.preventDefault();
        window.dispatchEvent(new CustomEvent("toggle-inspect"));
        return;
      }

      if (e.code === "KeyS") {
        if (!e.repeat && streaming) onScreenshot();
        e.preventDefault();
        return;
      }
      if (e.code === "KeyR") {
        if (!e.repeat && streaming) onToggleRecord();
        e.preventDefault();
        return;
      }

      if (!enabled) return;

      const map: Record<string, string> = {
        ArrowLeft: "left",
        ArrowRight: "right",
        ArrowUp: "up",
        ArrowDown: "down",
        Space: "play_pause",
        Escape: "menu",
        KeyP: "previous",
        KeyN: "next",
      };

      if (map[e.code]) {
        e.preventDefault();
        if (!e.repeat) onCommand(map[e.code]);
        return;
      }

      if (e.code === "Enter") {
        e.preventDefault();
        if (!returnHeld.current && e.repeat) {
          returnHeld.current = true;
          onCommand("select_hold");
        }
        return;
      }

      if (e.code === "KeyH") {
        e.preventDefault();
        if (e.repeat) return;
        if (homeArmed.current) {
          if (homeTimer.current) window.clearTimeout(homeTimer.current);
          homeTimer.current = null;
          homeArmed.current = false;
          onCommand("home_double");
        } else {
          homeArmed.current = true;
          homeTimer.current = window.setTimeout(fireHome, HOME_DOUBLE_MS);
        }
      }
    };

    const onUp = (e: KeyboardEvent) => {
      if (sheetOpen || isTextTarget(e.target)) return;
      if (!enabled) return;
      if (e.code === "Enter") {
        e.preventDefault();
        if (!returnHeld.current) onCommand("select");
        returnHeld.current = false;
      }
    };

    window.addEventListener("keydown", onDown);
    window.addEventListener("keyup", onUp);
    return () => {
      window.removeEventListener("keydown", onDown);
      window.removeEventListener("keyup", onUp);
      if (homeTimer.current) window.clearTimeout(homeTimer.current);
    };
  }, [enabled, sheetOpen, streaming, onCommand, onScreenshot, onToggleRecord]);
}
