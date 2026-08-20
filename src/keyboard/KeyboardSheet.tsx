import { useEffect, useRef, useState } from "react";
import { api } from "../api";
import type { KeyboardState } from "../types";

export function KeyboardSheet({
  snapshot,
  focused,
  onClose,
  onChange,
  onSubmit,
}: {
  snapshot: KeyboardState;
  focused: boolean;
  onClose: () => void;
  onChange: (previous: string, text: string) => void;
  onSubmit: (previous: string, text: string) => void;
}) {
  const [value, setValue] = useState(snapshot.text);
  const [clipStatus, setClipStatus] = useState<string>();
  const synced = useRef(snapshot.text);
  const editingUntil = useRef(0);
  const timer = useRef<number | null>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  const flushPending = () => {
    if (timer.current) {
      window.clearTimeout(timer.current);
      timer.current = null;
    }
  };

  useEffect(() => {
    inputRef.current?.focus();
    return () => flushPending();
  }, []);

  useEffect(() => {
    if (Date.now() < editingUntil.current) return;
    if (snapshot.text === value) {
      synced.current = snapshot.text;
      return;
    }
    setValue(snapshot.text);
    synced.current = snapshot.text;
  }, [snapshot.text]);

  const editLocally = (next: string) => {
    setValue(next);
    editingUntil.current = Date.now() + 900;
    if (timer.current) window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => {
      const previous = synced.current;
      if (previous === next) return;
      synced.current = next;
      onChange(previous, next);
    }, 160);
  };

  const handleFetchClipboard = async () => {
    try {
      setClipStatus("Fetching TV clipboard...");
      const text = await api.getClipboard();
      if (text) {
        editLocally(text);
        setClipStatus("Copied from TV clipboard!");
      } else {
        setClipStatus("TV clipboard is empty");
      }
      setTimeout(() => setClipStatus(undefined), 2500);
    } catch (e) {
      setClipStatus(`Error: ${e}`);
    }
  };

  const handleSendClipboard = async () => {
    try {
      setClipStatus("Sending to TV clipboard...");
      await api.setClipboard(value);
      setClipStatus("Sent to TV clipboard!");
      setTimeout(() => setClipStatus(undefined), 2500);
    } catch (e) {
      setClipStatus(`Error: ${e}`);
    }
  };

  return (
    <div
      className="sheet-backdrop"
      onClick={() => {
        flushPending();
        onClose();
      }}
    >
      <div className="sheet glass" onClick={(e) => e.stopPropagation()}>
        <div className="sheet-toolbar">
          <button
            className="surface-btn"
            style={{ width: "auto" }}
            onClick={() => {
              flushPending();
              onClose();
            }}
          >
            Close
          </button>
          <h2>Keyboard & Clipboard Sync</h2>
          <span />
        </div>
        <p className="hint">
          {focused
            ? "Mirrors the TV text box. Type here, or on the remote — not both at once."
            : "No text field focused. Type anyway; it'll go to whatever opens next."}
        </p>

        {clipStatus && (
          <div className="hint" style={{ color: "var(--accent-color)", fontWeight: 500 }}>
            {clipStatus}
          </div>
        )}

        <input
          ref={inputRef}
          className="field"
          style={{ fontSize: 18 }}
          value={value}
          onChange={(e) => editLocally(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Escape") {
              e.preventDefault();
              flushPending();
              onClose();
            }
            if (e.key === "Enter") {
              e.preventDefault();
              flushPending();
              const previous = synced.current;
              synced.current = value;
              onSubmit(previous, value);
            }
          }}
        />

        <div className="row" style={{ marginTop: 8, gap: 8 }}>
          <button className="surface-btn" onClick={handleFetchClipboard} style={{ fontSize: "0.85rem" }}>
            📋 Fetch from TV
          </button>
          <button className="surface-btn" onClick={handleSendClipboard} style={{ fontSize: "0.85rem" }}>
            📤 Push to TV
          </button>
        </div>

        <p className="hint" style={{ marginTop: 12 }}>
          Remote typing updates this box. We only send what you type here.
        </p>
        <div className="row">
          <button
            className="surface-btn"
            onClick={() => {
              const previous = synced.current;
              setValue("");
              synced.current = "";
              editingUntil.current = Date.now() + 900;
              if (previous) onChange(previous, "");
              inputRef.current?.focus();
            }}
          >
            Clear field
          </button>
          <button
            className="primary-btn"
            onClick={() => {
              flushPending();
              const previous = synced.current;
              synced.current = value;
              onSubmit(previous, value);
            }}
          >
            Submit ⏎
          </button>
        </div>
      </div>
    </div>
  );
}
