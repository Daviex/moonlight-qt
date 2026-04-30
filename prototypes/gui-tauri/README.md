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

To build and stage the prototype together with the native helper from the repository root:

```powershell
scripts\build-tauri-prototype.bat
```

This keeps the production Widgets package unchanged. The script first checks that it is running from the repository root, that the prototype package/manifests exist, and that npm, Cargo, rustc, and the local Tauri CLI dependency are available. On Windows it also warns if the WebView2 runtime is not detected in the standard registry locations, because the staged Tauri shell needs WebView2 at launch time. It then builds the native Moonlight helper, builds the Tauri shell with `--no-bundle`, stages both under `build\tauri-prototype`, and writes `Launch-Moonlight-Tauri.bat` with the required `MOONLIGHT_TAURI_BACKEND=ipc` and `MOONLIGHT_TAURI_HELPER` environment variables. It also writes `Launch-Moonlight-Tauri-Debug.bat`, which sets `MOONLIGHT_TAURI_DEBUG=1` and writes `MoonlightTauri.log` beside the staged executable. If a fresh native build already exists, set `SKIP_NATIVE_BUILD=1` before running the script to reuse `build\deploy-*-release\Moonlight.exe`. Set `TAURI_PACKAGE_ZIP=1` to also produce `build\installer-tauri-prototype-release\MoonlightTauriPrototype-<arch>-<version>.zip` from the staged package.

## Migration notes

The production Tauri migration should keep Moonlight's C++ backend and streaming engine native. The web UI should communicate through a narrow native command/event bridge for:

1. Host discovery, pairing, wake, rename, delete, details, and network tests.
2. App listing, launch, resume, quit, hide/unhide, direct launch, and box art.
3. Streaming settings snapshots, validation, saves, and the persisted native language preference.
4. Session launch lifecycle, warnings, errors, UI hide/show, and stream-window ownership.
5. Controller navigation actions translated into web focus operations while streaming owns SDL input during active sessions.

The streaming video path should remain a native SDL/window path. Do not attempt to render decoded frames inside the webview unless a separate renderer redesign is explicitly chosen.

## Current bridge scaffold

`src\bridge.ts` is the TypeScript-side contract for the UI. `src-tauri\src\main.rs` currently implements the same commands with an in-memory mock backend:

1. Hosts: list, add, pair, wake, rename, delete, expanded native details, wakeability/server-support metadata, network test, and action guards that block invalid pair/wake requests.
2. Apps: list, launch, resume running session, quit running app, hide/unhide, direct-launch toggle, app collector metadata, cached box-art URLs, empty-state guidance when a host is unpaired or returns no visible apps, and native validation for app command toggles.
3. Settings: load and save the current streaming settings snapshot, including core display, bitrate, audio/video, language, input, warning, network, and stream-behavior preferences, plus native default bitrate calculation and frontend/native validation for numeric, select, and boolean stream settings.
4. System: load native version, architecture, display/HDR, hardware acceleration, browser, desktop session, unmapped gamepad information, show gamepad mapping guidance, and open HTTP/HTTPS documentation or update URLs through the native browser integration.

The bridge also emits `moonlight-bridge-event` events for native-side host, app, session, settings, status, and controller navigation changes. The React shell subscribes to those events, refreshes the affected state, translates controller actions into web focus/activation/back/settings behavior, mirrors the same back/settings/refresh paths through keyboard shortcuts, tracks an active native stream panel for forwarded lifecycle state, hides the Tauri shell when the native stream requests it, shows/focuses the shell when the native lifecycle allows it again, and shows recent native events in the UI. This mirrors the production shape needed for host discovery updates, app list changes, stream lifecycle events, errors, warnings, and SDL-derived controller navigation events. `src-tauri\capabilities\default.json` grants the Tauri v2 `core:event:default` permission required for frontend event subscriptions and `core:window:default` for stream lifecycle window show/hide/focus calls.

The React shell is split into focused modules so the UI can grow without returning to a single large component. `src\components` contains page/card/panel components such as `HostsPage`, `AppsPage`, `HostCard`, `AppCard`, and `StreamPanel`. `src\ui` contains shared UI types, constants, artwork helpers, settings validation, controller-focus helpers, stream-state helpers, host predicates, and theme utilities. `src\styles.css` uses CSS variables for the current theme foundation. The toolbar theme picker persists a local webview theme (`Moonlight`, `Steam Deck`, or `High contrast`), and those tokens can later be consumed by Tailwind or a component library if the visual system moves in that direction.

Host management flows use in-app React dialogs instead of browser prompts for add, rename, delete confirmation, pairing PIN display, host details, and help. The pairing dialog follows native completion events, closing on pairing success and showing pairing errors without losing focus. The Help/About dialog also reads native system information through the bridge so diagnostics like HDR support, hardware acceleration, display mode, and unmapped gamepads are visible in the web UI. This keeps the prototype on the same focus/navigation path as the rest of the shell so keyboard, touch, and controller input can exercise these core dialogs.

The helper starts the native update checker during IPC startup and forwards `updateAvailable` bridge events with the available version and download URL. The React shell shows those events as a dismissible update banner and opens the download URL through the native `open_url` bridge command, matching the Widgets UI's update notification path without implementing update-check networking in TypeScript or navigating the webview itself.

Help/About documentation buttons also use the native `open_url` bridge command. The helper restricts this command to `http://` and `https://` URLs and reports a native error when no browser is available.

App entries include `boxArtUrl` from the native `AppListFacade`. The native helper returns cached local artwork as `data:image/png;base64,...` URLs so the webview does not depend on Flatpak or portable-install file path scopes. The React shell still accepts HTTP(S) and `file:` artwork URLs as fallbacks. The native helper continues to emit `appChanged` events when asynchronous box-art loading completes so the React app grid refreshes without duplicating artwork fetching logic in TypeScript.

For mock-backend runs, the prototype exposes a small "Controller event test" toolbar that asks the Rust side to emit controller actions. IPC-backend runs now also receive real controller events from Moonlight's existing SDL controller navigation source in the native helper. The helper disables its GUI controller polling while a native stream owns SDL input, then re-enables it when session lifecycle events return control to the UI.

The Rust side now separates the production command surface from the in-memory mock implementation:

1. `src-tauri\src\backend.rs` defines the DTOs and `MoonlightBackend` trait used by all Tauri commands.
2. `src-tauri\src\mock_backend.rs` contains the current in-memory mock implementation.
3. `src-tauri\src\ipc_backend.rs` contains the selected production bridge scaffold. It can spawn a native helper process and forward commands over a line-delimited JSON protocol.
4. `src-tauri\src\main.rs` owns the Tauri commands, event emission, and backend registration.

The default dev backend remains the in-memory mock so the prototype launches without extra native processes. A staged package launched directly from `build\tauri-prototype\MoonlightTauri.exe` auto-selects the IPC backend when `native\Moonlight.exe` is present beside it. To force the IPC bridge from an unstaged dev build, run the Tauri shell with:

```powershell
$env:MOONLIGHT_TAURI_BACKEND = 'ipc'
$env:MOONLIGHT_TAURI_HELPER = 'C:\Users\david\Desktop\Work\moonlight-qt\build\deploy-x64-release\Moonlight.exe'
npm run tauri dev
```

To capture startup and IPC diagnostics for freezes or empty-list regressions, enable debug logging before launch:

```powershell
$env:MOONLIGHT_TAURI_DEBUG = '1'
$env:MOONLIGHT_TAURI_LOG = 'C:\Users\david\Desktop\Work\moonlight-qt\build\tauri-prototype\MoonlightTauri.log'
```

When debug logging is enabled, the Rust/Tauri layer records startup, backend selection, helper spawn, frontend lifecycle markers, command begin/end records, IPC request/response IDs, forwarded helper events, helper stderr, and timeout/failure messages. `MOONLIGHT_TAURI_IPC_TIMEOUT_SECS` can override the default 15-second helper response timeout while debugging.

The native Moonlight executable now has a hidden `--tauri-bridge-helper` mode for this protocol. The helper reserves stdout for one JSON response per line and writes diagnostics to stderr. Requests include an ID and tagged command payload:

```json
{"id":1,"command":{"name":"list_hosts","payload":{}}}
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

The native helper validates request envelopes before dispatching commands. Malformed JSON, missing or non-integer request IDs, missing command objects, missing command names, and non-object payloads return explicit error frames instead of being coerced into request ID `0` or an empty command. The Rust IPC backend and native helper use the same command envelope shape: `command.name` contains the snake-case command name and `command.payload` contains the command object. Host/app command IDs and string payloads such as host addresses, host names, and URLs are also validated before facade lookup or native URL handling.

From a Windows checkout, direct helper IPC can be smoke-tested without launching the webview:

```powershell
scripts\test-tauri-helper-ipc.ps1
```

Pass `-HelperPath build\tauri-prototype\native\Moonlight.exe` or another `Moonlight.exe` path to test a specific staged helper. The smoke check sends the current `name`/`payload` `list_hosts` request and verifies that invalid `default_bitrate` input returns a validation error.

Inside the Tauri Flatpak, use the same protocol directly against the bundled helper:

```sh
flatpak run --user --command=sh com.moonlight_stream.Moonlight//master -c '
  export LD_LIBRARY_PATH=/app/lib/x86_64-linux-gnu:/app/lib:/app/lib64${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}
  export QT_PLUGIN_PATH=/app/lib/plugins${QT_PLUGIN_PATH:+:$QT_PLUGIN_PATH}
  export QT_QPA_PLATFORM=offscreen
  printf "%s\n" "{\"id\":1,\"command\":{\"name\":\"list_hosts\",\"payload\":{}}}" | /app/bin/native/Moonlight --tauri-bridge-helper
'
```

Current helper coverage is intentionally incremental: it can return real settings and host snapshots through the existing frontend facades, start `ComputerManager` polling/discovery when helper mode starts, update the subset of settings used by the prototype, route basic host/app mutations that already have facade methods, start network tests, resume a currently running native session through `ComputerListFacade::createSessionForCurrentGame()`, and emit native event frames for those mutations. Host snapshots collapse exact duplicate bridge records and same-name placeholder records so a discovered host does not appear twice when one backend entry has only empty placeholder address data. It also forwards `ComputerListFacade` discovery/state, pairing, manual-add completion/failure, and connection-test signals as host/status events. On Linux, the helper pumps the Qt event loop while waiting for IPC input so mDNS, polling, manual-add, box-art, and session signals can be delivered even when the process is otherwise idle on stdin. The latest `list_apps` request is kept as the observed app list, so later app-list resets, app changes, box-art changes, and selected-host loss are forwarded as app/host events without requiring another explicit command. `FrontendSessionCoordinator` lifecycle signals are forwarded as session/status events, including stage text, launch warnings, asynchronous errors, UI hide/show requests, quit segue requests, session finish, and cleanup readiness. Controller navigation events come from `SdlControllerNavigation` and are forwarded as `controllerAction` events with the same action vocabulary used by the TypeScript bridge. The Rust IPC backend reads helper stdout on a dedicated thread, correlates response frames by request ID, and forwards helper event frames to the existing `moonlight-bridge-event` Tauri channel. Command handlers still synthesize events for the mock backend, but skip those synthetic events when the active backend already forwards native helper events. With `MOONLIGHT_TAURI_DEBUG=1`, Rust/Tauri logs are written both to the configured log file and stderr, which makes `flatpak run ... 2>&1 | tee ...` useful for Steam Deck diagnostics.

`launch_app` and `resume_session` now route through the native helper's `AppListFacade`/`ComputerListFacade`, `FrontendSessionCoordinator`, `QtWidgetWindowContext`, and `Session` path. `quit_running_app` interrupts any active helper-owned native `Session` before asking Sunshine to quit the running app. The helper pumps the Qt event loop while waiting for IPC so native session startup and shutdown can progress after command responses. This keeps stream rendering in the native SDL/window path instead of the webview. The React prototype now turns forwarded session events into active-stream state for launch/resume, active/hide-requested, warning/error, quitting, finished, and cleanup states, and it uses Tauri window APIs to hide/show/focus the webview shell around native stream lifecycle events. The remaining production work is real-world validation on Windows, Linux, and Steam Deck.
