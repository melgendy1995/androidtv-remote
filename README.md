# Android TV Remote

Desktop remote for Android TV: live screen, D-pad, keyboard, apps, files, and a network inspector.

## Windows installer (what to send users)

This Mac cannot compile a Windows `.exe`. Build the NSIS installer on a Windows PC or GitHub Actions:

```bat
npm ci
npm run sidecars
npm run tauri build -- --bundles nsis
```

The installer lands at:

`src-tauri\target\release\bundle\nsis\Android TV Remote_*_x64-setup.exe`

GitHub Actions builds the installer on every push to `main`, on `v*` tags, and from **Actions → Windows installer → Run workflow**. Download the `AndroidTVRemote-windows` artifact (`Android TV Remote_*_x64-setup.exe`). Tags also attach that installer to a GitHub Release.

The Windows build needs **WebView2** (bundled as an offline installer) and ships `adb.exe` + `AdbWinApi.dll` + `AdbWinUsbApi.dll` next to the app.

## macOS development

```bash
npm ci
npm run sidecars
npm run tauri dev
```
