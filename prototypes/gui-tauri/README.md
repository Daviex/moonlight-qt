# Moonlight Tauri UI prototype

This prototype is the new target direction for replacing the GUI with a Tauri webview shell and a React + TypeScript frontend.

The first milestones are intentionally isolated from the production qmake build. They prove the frontend structure, accessible host/app/settings screens, and the Tauri command boundary without disturbing the current no-QML Qt Widgets bridge.

## Local commands

```powershell
npm install
npm run build
npm run tauri -- build --no-bundle
npm run tauri dev
```

This prototype has been validated on Windows with Node.js, npm, Rust/Cargo installed by rustup, and the existing MSVC toolchain available to Cargo.

## Migration notes

The production Tauri migration should keep Moonlight's C++ backend and streaming engine native. The web UI should communicate through a narrow native command/event bridge for:

1. Host discovery, pairing, wake, rename, delete, details, and network tests.
2. App listing, launch, resume, quit, hide/unhide, direct launch, and box art.
3. Streaming settings snapshots, validation, saves, and localization.
4. Session launch lifecycle, warnings, errors, UI hide/show, and stream-window ownership.
5. Controller navigation actions translated into web focus operations while streaming owns SDL input during active sessions.

The streaming video path should remain a native SDL/window path. Do not attempt to render decoded frames inside the webview unless a separate renderer redesign is explicitly chosen.

## Current bridge scaffold

`src\bridge.ts` is the TypeScript-side contract for the UI. `src-tauri\src\main.rs` currently implements the same commands with an in-memory mock backend:

1. Hosts: list, add, pair, wake, rename, delete, details, and network test.
2. Apps: list, launch, quit running app, hide/unhide, and direct-launch toggle.
3. Settings: load and save the current streaming settings snapshot.

The next production step is replacing the mock Rust state with a thin bridge into the existing C++ facades without moving backend behavior into TypeScript.
