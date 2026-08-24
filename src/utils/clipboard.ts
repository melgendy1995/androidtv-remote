import { writeText } from "@tauri-apps/plugin-clipboard-manager";

/**
 * Copy text to the OS clipboard, working around Tauri webview quirks.
 * Order: Tauri plugin (most reliable in the webview) → async Clipboard API
 * → hidden-textarea + execCommand fallback. Returns true when a path
 * reported success.
 */
export async function copyText(text: string): Promise<boolean> {
  if (!text) return false;
  try {
    await writeText(text);
    return true;
  } catch {
    // fall through
  }
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    // fall through
  }
  try {
    const ta = document.createElement("textarea");
    ta.value = text;
    ta.style.position = "fixed";
    ta.style.opacity = "0";
    document.body.appendChild(ta);
    ta.focus();
    ta.select();
    const ok = document.execCommand("copy");
    ta.remove();
    return ok;
  } catch {
    return false;
  }
}
