# Android TV Remote & Developer Suite - Comprehensive Implementation Plan

This document outlines the full multi-phase development plan for expanding the **Android TV Remote** application with advanced developer tools, app management, file transfers, and enhanced remote controls.

---

## Architecture Overview

- **Frontend**: React 19 + TypeScript + Vite + Custom CSS
- **Desktop Runtime**: Tauri v2
- **Backend / Communication**: Rust (`src-tauri/src/`) + ADB sidecar (`adb.rs`) + Scrcpy streaming sidecar (`scrcpy.rs`)

---

## Phase 1: App Management & Sideloading (App Launcher Grid)

- [x] **ADB Integration**:
  - `list_apps`: Enumerate installed applications (`pm list packages -3` and `-s` with app labels).
  - `launch_app`: Launch package via `monkey` or `am start`.
  - `force_stop_app`: Terminate package with `am force-stop`.
  - `install_apk`: Sideload `.apk` files via `adb install -r`.
  - `uninstall_app`: Remove package via `adb uninstall`.
- [x] **Frontend UI**:
  - App Drawer modal / tab with app search & filtering.
  - One-click app launch, force-stop, and uninstall buttons.
  - Drag-and-drop APK installer overlay.

---

## Phase 2: Enhanced Media & Input Controls

- [x] **Trackpad & Mouse Pointer Mode**:
  - Direct pointer drag and scroll injection on screen stage.
  - Mouse mode toggle for non-TV-optimized apps.
- [x] **Clipboard Synchronization**:
  - `get_clipboard`: Fetch Android device clipboard text.
  - `set_clipboard`: Send desktop clipboard text directly to TV.
- [x] **Custom Key Macros & Shortcuts**:
  - User-configurable macro buttons (e.g. Netflix, YouTube, Custom ADB Keycode combos).

---

## Phase 3: File Manager & Media Options

- [x] **ADB File Explorer**:
  - `list_files`: Directory listing on `/sdcard/` and device filesystem.
  - `upload_file` (`adb push`) & `download_file` (`adb pull`).
  - File deletion and directory creation.
- [x] **Audio & Video Streaming Configuration**:
  - Configurable Scrcpy max resolution (720p, 1080p, Native), bitrate (2M, 8M, 16M), max FPS, and audio forwarding toggles.

---

## Phase 4: UX & Performance Enhancements

- [x] **Multi-Device Quick Switcher**:
  - Header bar device switcher dropdown for seamless device toggling.
- [x] **Quality Presets**:
  - Fast Low-Latency preset (720p @ 60fps) vs High Quality preset (1080p+ @ high bitrate).
