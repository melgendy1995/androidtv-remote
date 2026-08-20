import { useEffect, useRef } from "react";
import type { RecordingStatus, StreamStatus } from "../types";

function formatElapsed(ms: number) {
  const s = Math.floor(ms / 1000);
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const sec = s % 60;
  const pad = (n: number) => n.toString().padStart(2, "0");
  return h > 0 ? `${h}:${pad(m)}:${pad(sec)}` : `${m}:${pad(sec)}`;
}

function formatSize(bytes: number) {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export function StageView({
  connected,
  stream,
  recording,
  onTap,
}: {
  connected: boolean;
  deviceName?: string;
  stream: StreamStatus;
  recording: RecordingStatus;
  onTap: (x: number, y: number, width: number, height: number) => void;
}) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const decoderRef = useRef<VideoDecoder | null>(null);
  const wsRef = useRef<WebSocket | null>(null);

  // WebCodecs Stream Decoder
  useEffect(() => {
    if (!stream.streaming || stream.videoPort == null) {
      if (decoderRef.current && decoderRef.current.state !== "closed") {
        try {
          decoderRef.current.close();
        } catch {
          /* ignore */
        }
      }
      decoderRef.current = null;
      wsRef.current?.close();
      wsRef.current = null;
      return;
    }

    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    let cancelled = false;
    let isConfigured = false;
    let avccDescription: Uint8Array | null = null;
    let waitForKeyframe = false;
    let timestampUs = 0;

    const createDecoder = () => {
      if (decoderRef.current && decoderRef.current.state !== "closed") {
        try {
          decoderRef.current.close();
        } catch {
          /* ignore */
        }
      }

      const dec = new VideoDecoder({
        output: (frame) => {
          if (canvas.width !== frame.displayWidth)
            canvas.width = frame.displayWidth || 1280;
          if (canvas.height !== frame.displayHeight)
            canvas.height = frame.displayHeight || 720;
          ctx.drawImage(frame, 0, 0);
          frame.close();
        },
        error: (err) => {
          console.warn("VideoDecoder error:", err);
          isConfigured = false;
          waitForKeyframe = true;
        },
      });
      decoderRef.current = dec;
      return dec;
    };

    const configureDecoder = (description: Uint8Array) => {
      if (cancelled) return;
      avccDescription = description;
      const dec = createDecoder();

      let exactCodec = "avc1.42e01e";
      if (description.length >= 4) {
        const profile = description[1].toString(16).padStart(2, "0");
        const compat = description[2].toString(16).padStart(2, "0");
        const level = description[3].toString(16).padStart(2, "0");
        exactCodec = `avc1.${profile}${compat}${level}`;
      }

      const codecsToTry = [
        exactCodec,
        "avc1.42e01e",
        "avc1.4d401f",
        "avc1.640028",
        "avc1.42001e",
      ];

      for (const codec of codecsToTry) {
        try {
          dec.configure({
            codec,
            optimizeForLatency: true,
            description,
          });
          if (dec.state === "configured") {
            isConfigured = true;
            waitForKeyframe = true;
            return;
          }
        } catch (err) {
          console.warn(`Codec ${codec} config failed:`, err);
        }
      }

      try {
        dec.configure({
          codec: exactCodec,
          optimizeForLatency: true,
        });
        isConfigured = dec.state === "configured";
        waitForKeyframe = isConfigured;
      } catch {
        isConfigured = false;
      }
    };

    const ws = new WebSocket(`ws://127.0.0.1:${stream.videoPort}/video`);
    ws.binaryType = "arraybuffer";
    wsRef.current = ws;

    ws.onopen = () => {
      if (cancelled) ws.close();
    };
    ws.onerror = (e) => {
      if (!cancelled) console.warn("WebSocket error:", e);
    };

    ws.onmessage = (ev) => {
      if (cancelled) return;
      if (!(ev.data instanceof ArrayBuffer)) return;
      const bytes = new Uint8Array(ev.data);
      if (bytes.length < 5) return;
      const flags = bytes[0];
      const payload = bytes.slice(1);
      const isConfig = (flags & 0x01) !== 0;
      const isKey = (flags & 0x02) !== 0;

      if (isConfig) {
        configureDecoder(payload);
        return;
      }

      if (waitForKeyframe && !isKey) {
        return;
      }

      if ((!isConfigured || decoderRef.current?.state !== "configured") && isKey && avccDescription) {
        configureDecoder(avccDescription);
      }

      const activeDecoder = decoderRef.current;
      if (!isConfigured || !activeDecoder || activeDecoder.state !== "configured") {
        return;
      }

      if (activeDecoder.decodeQueueSize > 8 && !isKey) {
        waitForKeyframe = true;
        return;
      }

      try {
        timestampUs += 16_667;
        activeDecoder.decode(
          new EncodedVideoChunk({
            type: isKey ? "key" : "delta",
            timestamp: timestampUs,
            data: payload,
          })
        );
        waitForKeyframe = false;
      } catch (err) {
        console.warn("decode error:", err);
        waitForKeyframe = true;
      }
    };

    return () => {
      cancelled = true;
      ws.onmessage = null;
      ws.onerror = null;
      ws.onopen = null;
      ws.close();
      wsRef.current = null;
      if (decoderRef.current && decoderRef.current.state !== "closed") {
        try {
          decoderRef.current.close();
        } catch {
          /* ignore */
        }
      }
      decoderRef.current = null;
    };
  }, [stream.streaming, stream.videoPort]);

  const showPlaceholder = !connected;

  return (
    <div
      className="stage"
      style={{
        display: "flex",
        flexDirection: "column",
        height: "100%",
        position: "relative",
      }}
    >
      <div
        className="stage-frame"
        style={{
          flex: 1,
          position: "relative",
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          minHeight: 0,
        }}
        onClick={(e) => {
          if (!connected || !canvasRef.current) return;
          const canvas = canvasRef.current;
          const rect = canvas.getBoundingClientRect();
          const x = e.clientX - rect.left;
          const y = e.clientY - rect.top;
          if (x < 0 || y < 0 || x > rect.width || y > rect.height) return;
          onTap(x, y, rect.width, rect.height);
        }}
      >
        <canvas
          ref={canvasRef}
          width={1280}
          height={720}
          style={{
            maxWidth: "100%",
            maxHeight: "100%",
            objectFit: "contain",
            display: connected ? "block" : "none",
          }}
        />

        {showPlaceholder && (
          <div
            className="stage-placeholder"
            style={{ position: "absolute", zIndex: 10 }}
          >
            <h3>Connect an Android TV</h3>
            <p>
              Open Devices (⋮) to connect over Wi-Fi or USB ADB.
            </p>
          </div>
        )}
      </div>

      {recording.recording ? (
        <div className="rec-hud glass glass-capsule">
          <span className="rec-dot" />
          {formatElapsed(recording.elapsedMs)} · {formatSize(recording.bytes)}
        </div>
      ) : null}
    </div>
  );
}
