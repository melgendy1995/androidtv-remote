# Android TV Remote

Desktop remote for Android TV on **macOS** and **Windows**. Live screen, D-pad, keyboard, app/APK management, file transfer, logcat, crash capture, and HTTP inspection — all over ADB.

The window is split into a **live TV stage** (left) and a **remote sidebar** (right). Inspect tools (Logs / Network / Crashes) slide up from the bottom of the stage.

---

## Contents

- [Setup](#setup)
  - [Install a team build](#install-a-team-build)
  - [Run from source](#run-from-source)
  - [Build locally](#build-locally)
- [Connect a TV](#connect-a-tv)
- [How to use](#how-to-use)
- [Keyboard shortcuts](#keyboard-shortcuts)
- [Settings](#settings)
- [Files on disk](#files-on-disk)
- [What it does not do](#what-it-does-not-do)

---

## Setup

### Install a team build

GitHub Actions builds on every push to `main`, on `v*` tags, and from **Actions → Run workflow**.

| Platform | Workflow | Artifact | What you get |
|---|---|---|---|
| macOS (Apple Silicon + Intel) | **macOS app** | `AndroidTVRemote-macos` | `Android TV Remote.app` |
| Windows x64 | **Windows installer** | `AndroidTVRemote-windows` | `Android TV Remote_*_x64-setup.exe` |

Tags also attach those files to the GitHub Release.

**macOS first launch.** The app is unsigned (no Apple Developer ID). Right-click → **Open**, or:

```bash
xattr -cr "/Applications/Android TV Remote.app"
```

**Windows.** The installer needs **WebView2**. An offline WebView2 installer is bundled. `adb.exe`, `AdbWinApi.dll`, and `AdbWinUsbApi.dll` ship next to the app.

Download artifacts from the run page (GitHub login required).

### Run from source

Needs **Node 20**, **Rust stable**, and **npm**. CI uses those versions.

```bash
npm ci
npm run sidecars
npm run tauri dev
```

`npm run sidecars` downloads platform-tools ADB and **scrcpy 3.3.1** into `src-tauri/resources/` (gitignored).

macOS bundle target is **14.0+** (`src-tauri/tauri.conf.json`).

### Build locally

```bash
# macOS universal .app
npm ci
npm run build:macos
# → src-tauri/target/universal-apple-darwin/release/bundle/macos/Android TV Remote.app

# Windows NSIS installer (run on Windows)
npm ci
npm run build:windows
# → src-tauri\target\release\bundle\nsis\Android TV Remote_*_x64-setup.exe
```

A Mac cannot produce the Windows `.exe`. Use a Windows PC or the **Windows installer** Action.

---

## Connect a TV

The TV and this computer must be on the same network (wireless) or linked by USB.

1. On the TV: **Developer options** → enable **USB debugging** and/or **Wireless debugging**.
2. Open the app. Click the device pill at the top of the sidebar, or **⋮**.
3. Pick a listed device → **Connect**. The live screen starts by itself.

**Wireless, already paired** (typical `IP:5555`):

- Type `192.168.1.50` or `192.168.1.50:5555` → **Add**.

**Android 11+ first pairing** (pairing port is random, not 5555):

1. TV → Developer options → Wireless debugging → **Pair device with pairing code**.
2. In the app: **Pair wireless…**
3. Enter the **IP:pairing-port** shown on the TV and the **6-digit code**.
4. After pairing, the app looks up the connect port via `adb mdns services` / `adb devices`, then falls back to `:5555`.

If pairing succeeds but connect fails, open Wireless debugging again and **Add** the IP:port shown there.

**Keep the port stable.** On a connected device, **Lock Port 5555** runs `adb tcpip 5555` so the TV stays on 5555 after reboot.

**Unauthorized.** Accept the RSA fingerprint prompt on the TV. The red banner says “Accept the RSA prompt on the TV.”

**Switch / forget.** **⋮** → **Connect** on another row, **Disconnect**, or **⋯** to forget a saved host.

---

## How to use

Walk through this in order the first time. After that, use it as a feature map.

### 1. Layout

| Area | Where | What it is |
|---|---|---|
| **Stage** | Left | Live H.264 screen from scrcpy. Click the picture to tap the TV. |
| **Sidebar** | Right | Device, tools, now-playing, D-pad, snap/record. |
| **Inspect drawer** | Bottom of the stage | Logs, Network, Crashes. Closed until you open it. |

Hide the sidebar with **»**. Restore with **« Show Controls & Remote** on the stage.

Toolstrip under the device pill (needs a connected TV except Inspect and Settings):

| Button | Opens |
|---|---|
| 🚀 | Apps & Sideloading |
| 📁 | ADB File Explorer |
| ⌨ | Keyboard & Clipboard |
| ☰ | Inspect drawer (Logs / Network / Crashes) |
| ↻ | Reconnect ADB and restart the stream |
| ⚙ | Settings |

### 2. Live screen

On connect, scrcpy starts automatically (`control=false`, `audio=false`, `stay_awake=true`). Default quality is **1080p, 8 Mbps, 60 fps** (change in Settings).

- **Click** the video to send `input tap` at that point (letterbox bars are ignored).
- After a click, the app waits 280 ms and reads the focused field. If the TV reports a text box, **Keyboard & Clipboard** opens. The same sheet also opens after two consecutive focused polls (every 600 ms) while you use the D-pad.
- A red **REC** overlay shows elapsed time and file size while recording.

If the picture stalls, press **↻**.

### 3. Remote (D-pad and media)

Same keys as a physical remote. Disabled until a TV is connected.

| Control | Sends |
|---|---|
| ▲ ▼ ◀ ▶ | D-pad |
| ● center (click) | Select / OK |
| ● center (hold ~600 ms) | Select long-press |
| ↩ Back | Back (`KEYCODE_BACK`) |
| ⏯ / ⏮ / ⏭ | Play-pause / previous / next |
| 🔉 🔊 🔇 | Volume down / up / mute |
| ⏻ Power | Power |
| ⌂ TV (single) | Home |
| ⌂ TV (double-tap within 250 ms) | Recents / app switch |
| ⌂⌂ Apps | Home long-press |

The computer keyboard drives the same commands when no sheet is open — see [Keyboard shortcuts](#keyboard-shortcuts).

**Now on device** (card above the D-pad) is `dumpsys media_session`: title, artist, package. Polled every 2 seconds.

### 4. Keyboard & clipboard

Opens from **⌨**, or automatically when a TV text field is focused.

- The field **mirrors the TV text box**. Type here, or on the hardware remote — not both at once.
- Changes are sent as a **delta** (suffix / backspace), not a full rewrite.
- Non-ASCII goes through the TV clipboard + paste (`input text` drops it).
- **Enter** / **Submit ⏎** commits and closes.
- **Escape** or **Close** dismisses; it stays closed until the TV field blurs.
- **Fetch from TV** / **Push to TV** copy the device clipboard in or out.
- **Clear field** wipes the TV field.

If no field is focused, you can still type; text goes to whatever opens next.

### 5. Apps & sideloading (🚀)

Lists user and system packages. Search by label or package name.

Filters: **All**, **TV & Media**, **User Apps**, **System**. “TV & Media” matches names/packages that contain strings such as `netflix`, `youtube`, `jawwy`, `intigral`.

On each app:

- **Launch**
- **Force stop** (`am force-stop`)
- **Uninstall** (confirms first)

**Sideload:** **Browse APK…** (native file dialog, `.apk` only) or paste a full path → **Sideload APK** (`adb install -r`). Desktop files the dialog cannot see can still be installed by pasting the path.

### 6. File explorer (📁)

Starts at `/sdcard`. Click a folder to enter it, **⬆ Up** or type a path and **Go**.

| Action | How |
|---|---|
| Upload | Paste a local file path → **Upload to TV** (`adb push` into the current folder) |
| Download | **Download** on a file (`adb pull`). Folder = the path field, else Settings capture dir |
| New folder | **+ Folder** |
| Delete | **Delete** (confirms first) |

There is no rename, move, or copy.

### 7. Inspect drawer — Logs, Network, Crashes

Open with **☰** or `` ` `` (backquote). Drag the top edge to resize (200 px–85% of the window). **⤢** maximizes to 75% of the window height.

Badges on the tabs are live counts.

#### Logs

Live `logcat` for the connected TV. Buffer keeps up to 20 000 lines. Switching devices **clears** logs, network, and crashes so sessions do not mix.

| Control | Default | What it does |
|---|---|---|
| **Min** level | Verbose (`V`) | Show this level and above (`V D I W E F`) |
| **Filter Tag** | empty | Tag contains |
| **Search logs** | empty | Tag, message, raw line, PID |
| Package select | All | Includes **STC TV** (`com.intigral.jawwytv`) and **Jawwy** (`net.intigral.jawwytv`) |
| **Auto-scroll** | on | Follow the tail. Scroll up to pause; back to the bottom to resume |
| **Pause / Resume** | running | Stop appending in the UI |
| **Clear** | — | Empty the buffer |
| **Export** | — | Write `logcat-YYYY-MM-DD-HHMMSS.txt` in the capture directory and reveal it |

Click a line for the full message. **Copy Log Line** copies it.

#### Network

HTTP(S) seen by the **built-in MITM proxy** (default) and URLs scraped from logcat.

Default proxy: TV `http_proxy` → `127.0.0.1:<inspect port>` (default **8899**) via `adb reverse`. The proxy terminates TLS and records method, **full URL**, headers, and bodies (gzip/deflate decoded, JSON pretty-printed, 256 KB cap).

| Control | Default | What it does |
|---|---|---|
| **All / Failed only / 2xx Success** | All | Status filter |
| Search | empty | Method, URL, host, path, status |
| **Hide failed tunnels** | on | Hide TLS decrypt failures (system/Google hosts, pinning) |
| **Auto-scroll** | off | Follow new rows |
| **Clear** | — | Empty the list |
| **Export HAR** | — | `network-YYYY-MM-DD-HHMMSS.har` in `~/Downloads/AndroidTV Captures` |

Click a row. The detail pane (drag the left edge to resize, **⤢** to ~82% width) has collapsible sections: Overview, URL + query, Request headers, Request body, Response headers, Response body. Copy buttons per section plus **All**.

Successful `CONNECT` rows are hidden once decrypted requests exist for that host. A **Decrypt Failed** badge means the handshake did not complete (untrusted CA or pinning).

**Charles instead of the built-in tab:** Settings → **TV HTTP proxy** → **Charles**, set this computer’s LAN IP and Charles port (default **8888**). The TV then points at Charles; the Network tab will not see those decrypted calls.

**Don’t change TV proxy:** leaves `http_proxy` / `global_http_proxy_*` alone.

#### Crashes

Crashes and ANRs parsed from logcat (`FATAL EXCEPTION`, `am_crash`, native crash, `ANR in`, …), then enriched from `logcat -b crash`, dropbox, and `/data/anr`. Session cap: 200. Cleared on device switch.

Each row is **collapsed**: kind, package, time, PID, exception. **Show stack trace** opens the full dump; **Hide stack trace** closes it. **Copy** copies the whole entry, **Copy stack** the trace only, **Save** writes `crash-<id>.txt` or `anr-<id>.txt` under `~/Downloads/AndroidTV Captures`. **Clear** wipes the session list.

### 8. Screenshots and recordings

At the bottom of the sidebar (or **S** / **R** while the stream is running):

| Button | File | Notes |
|---|---|---|
| **Snap** | `screenshot-YYYY-MM-DD-HHMMSS.png` | `screencap -p`, then Reveal in Finder/Explorer |
| **Rec / Stop** | `recording-YYYY-MM-DD-HHMMSS.mp4` | Muxes the live H.264 stream |

Both use **Settings → Capture Directory** (default `~/Downloads/AndroidTV Captures`).

### 9. Settings (⚙)

| Field | Default | Effect |
|---|---|---|
| ADB Binary Path | blank | Bundled sidecar, else `PATH` |
| Max Stream Resolution | 1080p (`1920`) | Also **720p** (`1280`) or Native (`0`) |
| Video Bitrate | 8 Mbps | 2 / 8 / 16 Mbps |
| Max Target FPS | 60 | 30 or 60 |
| TV HTTP proxy | Built-in Network tab | `builtin` / `charles` / `off` |
| Inspect Proxy Port | `8899` | Built-in mode only |
| Charles host / port | empty / `8888` | Charles mode only |
| Capture Directory | empty | Screenshots, recordings, logcat export. Empty → `~/Downloads/AndroidTV Captures` |

**Test ADB** prints `adb version` and `adb devices`. **Save Settings** writes `~/.androidtv-remote/settings.json` and re-applies the TV proxy.

Audio forwarding is **not implemented**. A leftover `audioEnabled` flag in an old settings file is ignored so the stream still starts.

---

## Keyboard shortcuts

These fire only when:

- no sheet is open (devices, pair, apps, files, keyboard, settings), and
- focus is not in an `<input>`, `<textarea>`, or content-editable field, and
- you are not holding ⌘ / Ctrl / Alt / Shift.

| Key | Action | Needs |
|---|---|---|
| `` ` `` (backquote) | Toggle Inspect drawer | — |
| **S** | Screenshot | Stream running |
| **R** | Start/stop recording | Stream running |
| **← → ↑ ↓** | D-pad | Connected |
| **Enter** | Select (on key-up) | Connected |
| **Enter** hold (key repeat) | Select long-press | Connected |
| **Esc** | Back | Connected |
| **Space** | Play / pause | Connected |
| **H** | Home | Connected |
| **H** twice within 250 ms | Recents / app switch | Connected |
| **P** | Previous track | Connected |
| **N** | Next track | Connected |

**Inspect Network detail pane** (pane focused):

| Key | Action |
|---|---|
| **⌘C** / **Ctrl+C** | Copy the selected text; if nothing is selected, copy the full request dump |

**Keyboard sheet:**

| Key | Action |
|---|---|
| **Enter** | Submit and close |
| **Esc** | Close without extra submit |

Button tooltips in the UI repeat Snap (**S**), Rec (**R**), and Inspect (`` ` ``).

---

## Settings

Stored at `~/.androidtv-remote/settings.json`. Saved devices at `~/.androidtv-remote/devices.json`.

| JSON key | UI | Default |
|---|---|---|
| `adbPath` | ADB Binary Path | `""` |
| `proxyPort` | Inspect Proxy Port | `8899` |
| `captureDir` | Capture Directory | `""` → `~/Downloads/AndroidTV Captures` |
| `maxSize` | Max Stream Resolution | `1920` |
| `bitRate` | Video Bitrate | `8000000` |
| `maxFps` | Max Target FPS | `60` |
| `deviceProxyMode` | TV HTTP proxy | `"builtin"` |
| `charlesHost` | Charles host | `""` |
| `charlesPort` | Charles port | `8888` |

Built-in Network decrypt in CI installers uses GitHub secrets `MITM_CA_PEM` and `MITM_CA_KEY` (written into `src-tauri/resources/mitm/` at build time, gitignored). Without those secrets the app generates a CA at runtime; production TV apps will fail TLS until that CA is trusted on the device.

---

## Files on disk

| Path | Contents |
|---|---|
| `~/.androidtv-remote/settings.json` | Settings |
| `~/.androidtv-remote/devices.json` | Saved hosts |
| `~/.androidtv-remote/mitm/` | Cached MITM CA (built-in proxy) |
| `~/Downloads/AndroidTV Captures/` | Default screenshots, recordings, logcat export, HAR, saved crash txt |

HAR export and **Save** on a crash always use `~/Downloads/AndroidTV Captures`, even if Capture Directory is customized. Screenshots, recordings, and logcat export follow Capture Directory.

---

## What it does not do

- **Audio** from the TV (scrcpy audio is forced off).
- **Drag / scroll** on the stage — click-to-tap only.
- **Rename / move / copy** in the file explorer.
- **Custom macro buttons** in the UI.
- A signed macOS Developer ID build.

---

## Development extras

```bash
npx tsc --noEmit
cargo test --manifest-path src-tauri/Cargo.toml
```

The **Tests** workflow (`tests.yml`) runs `tsc`, `cargo test`, and `clippy` on `ubuntu-latest` for `main`, `feature/**`, and pull requests.
