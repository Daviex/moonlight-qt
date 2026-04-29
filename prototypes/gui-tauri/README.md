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

The bridge also emits `moonlight-bridge-event` events for native-side host, app, session, settings, status, and controller navigation changes. The React shell subscribes to those events, refreshes the affected state, translates controller actions into web focus/activation/back/settings behavior, tracks an active native stream panel for forwarded lifecycle state, hides the Tauri shell when the native stream requests it, shows/focuses the shell when the native lifecycle allows it again, and shows recent native events in the UI. This mirrors the production shape needed for host discovery updates, app list changes, stream lifecycle events, errors, warnings, and SDL-derived controller navigation events. `src-tauri\capabilities\default.json` grants the Tauri v2 `core:event:default` permission required for frontend event subscriptions and `core:window:default` for stream lifecycle window show/hide/focus calls.

For mock-backend runs, the prototype exposes a small "Controller event test" toolbar that asks the Rust side to emit controller actions. IPC-backend runs now also receive real controller events from Moonlight's existing SDL controller navigation source in the native helper. The helper disables its GUI controller polling while a native stream owns SDL input, then re-enables it when session lifecycle events return control to the UI.

The Rust side now separates the production command surface from the in-memory mock implementation:

1. `src-tauri\src\backend.rs` defines the DTOs and `MoonlightBackend` trait used by all Tauri commands.
2. `src-tauri\src\mock_backend.rs` contains the current in-memory mock implementation.
3. `src-tauri\src\ipc_backend.rs` contains the selected production bridge scaffold. It can spawn a native helper process and forward commands over a line-delimited JSON protocol.
4. `src-tauri\src\main.rs` owns the Tauri commands, event emission, and backend registration.

The default backend remains the in-memory mock so the prototype launches without extra native processes. To exercise the IPC bridge scaffold later, run the Tauri shell with:

```powershell
$env:MOONLIGHT_TAURI_BACKEND = 'ipc'
$env:MOONLIGHT_TAURI_HELPER = 'C:\Users\david\Desktop\Work\moonlight-qt\build\deploy-x64-release\Moonlight.exe'
npm run tauri dev
```

The native Moonlight executable now has a hidden `--tauri-bridge-helper` mode for this protocol. The helper reserves stdout for one JSON response per line and writes diagnostics to stderr. Requests include an ID and tagged command payload:

```json
{"id":1,"command":{"command":"list_hosts"}}
```

Responses must echo the request ID and include either `result` or `error`:

```json
{"id":1,"result":[{"id":"gaming-pc","name":"Gaming PC","address":"192.168.1.20","status":"Online","paired":true,"running":false}]}
```

The helper can also write event frames at any point on stdout:

```json
{"event":{"kind":"settingsChanged","message":"Settings saved."}}
```

This process boundary is the chosen production direction because it avoids mixing Tauri/Rust and Qt/C++ event loops in one process, keeps the existing C++ backend and SDL streaming ownership intact, and prevents TypeScript from reimplementing Moonlight backend logic.

Current helper coverage is intentionally incremental: it can return real settings and host snapshots through the existing frontend facades, update the subset of settings used by the prototype, route basic host/app mutations that already have facade methods, and emit native event frames for those mutations. It also forwards `ComputerListFacade` discovery/state, pairing, and connection-test signals as host/status events. The latest `list_apps` request is kept as the observed app list, so later app-list resets, app changes, box-art changes, and selected-host loss are forwarded as app/host events without requiring another explicit command. `FrontendSessionCoordinator` lifecycle signals are forwarded as session/status events, including stage text, launch warnings, asynchronous errors, UI hide/show requests, quit segue requests, session finish, and cleanup readiness. Controller navigation events come from `SdlControllerNavigation` and are forwarded as `controllerAction` events with the same action vocabulary used by the TypeScript bridge. The Rust IPC backend reads helper stdout on a dedicated thread, correlates response frames by request ID, and forwards helper event frames to the existing `moonlight-bridge-event` Tauri channel. Command handlers still synthesize events for the mock backend, but skip those synthetic events when the active backend already forwards native helper events.

`launch_app` now routes through the native helper's `AppListFacade`, `FrontendSessionCoordinator`, `QtWidgetWindowContext`, and `Session` path. The helper pumps the Qt event loop while waiting for IPC so native session startup can progress after the command response. This keeps stream rendering in the native SDL/window path instead of the webview. The React prototype now turns forwarded session events into active-stream state for launch, active/hide-requested, warning/error, quitting, finished, and cleanup states, and it uses Tauri window APIs to hide/show/focus the webview shell around native stream lifecycle events. The remaining production work is a polished quit/resume flow around an active native stream and real-world validation on Windows, Linux, and Steam Deck.
