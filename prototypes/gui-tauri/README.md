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

To build and stage the prototype with the default in-process Rust backend from the repository root:

```powershell
scripts\build-tauri-prototype.bat
```

This keeps the production Widgets package unchanged. The script first checks that it is running from the repository root, that the prototype package/manifests exist, and that npm, Cargo, rustc, and the local Tauri CLI dependency are available. On Windows it also warns if the WebView2 runtime is not detected in the standard registry locations, because the staged Tauri shell needs WebView2 at launch time. It then builds the Tauri shell with `--no-bundle`, stages `MoonlightTauri.exe` under `build\tauri-prototype`, and writes `Launch-Moonlight-Tauri.bat` for the default in-process Rust backend. It also writes `Launch-Moonlight-Tauri-Debug.bat`, which sets `MOONLIGHT_TAURI_DEBUG=1` and writes `MoonlightTauri.log` beside the staged executable. Set `TAURI_PACKAGE_ZIP=1` to also produce `build\installer-tauri-prototype-release\MoonlightTauriPrototype-<arch>-<version>.zip` from the staged package.

## Migration notes

The production Tauri migration now targets an in-process Rust backend with Moonlight's GameStream C library linked through Rust FFI. The web UI communicates through the Tauri command/event bridge for:

1. Host discovery, pairing, wake, rename, delete, details, and network tests.
2. App listing, launch, resume, quit, hide/unhide, direct launch, and box art.
3. Streaming settings snapshots, validation, saves, and persisted preferences.
4. Session launch lifecycle, warnings, errors, UI hide/show, and stream-window ownership.
5. Controller navigation actions translated into web focus operations while streaming owns native input during active sessions.

The streaming video path remains native and outside the webview. The Rust backend owns session setup, audio output, FFmpeg decode, native video presentation, and input forwarding while `moonlight-common-c` remains the C GameStream transport/session library.

## Current bridge scaffold

`src\bridge.ts` is the TypeScript-side contract for the UI. `src-tauri\src\main.rs` implements the commands through the Rust backend, with an explicit mock backend still available for UI-only development:

1. Hosts: list, mDNS discovery, add, pair, wake, rename, delete, expanded details, wakeability/server-support metadata, network test, and action guards that block invalid pair/wake requests.
2. Apps: authenticated Sunshine app-list requests, launch, resume running session, quit running app, hide/unhide, direct-launch toggle, app collector metadata, cached box-art URLs, empty-state guidance when a host is unpaired or returns no visible apps, and Rust validation for app command toggles.
3. Settings: load and save the current streaming settings snapshot, including core display, bitrate, audio/video, language, input, warning, network, and stream-behavior preferences, plus native-compatible default bitrate calculation and frontend/Rust validation for numeric, select, and boolean stream settings.
4. System: load version, architecture, display/HDR, hardware acceleration, browser, desktop session, unmapped gamepad information, show gamepad mapping guidance, and open HTTP/HTTPS documentation or update URLs through the Rust browser integration.

The bridge also emits `moonlight-bridge-event` events for Rust-side host, app, session, settings, status, and controller navigation changes. The React shell subscribes to those events, refreshes the affected state, translates controller actions into web focus/activation/back/settings behavior, mirrors the same back/settings/refresh paths through keyboard shortcuts, tracks an active native stream panel for forwarded lifecycle state, hides the Tauri shell when the native stream requests it, shows/focuses the shell when the session lifecycle allows it again, and shows recent Rust events in the UI. `src-tauri\capabilities\default.json` grants the Tauri v2 `core:event:default` permission required for frontend event subscriptions and `core:window:default` for stream lifecycle window show/hide/focus calls.

The React shell is split into focused modules so the UI can grow without returning to a single large component. `src\components` contains page/card/panel components such as `HostsPage`, `AppsPage`, `HostCard`, `AppCard`, and `StreamPanel`. `src\ui` contains shared UI types, constants, artwork helpers, settings validation, controller-focus helpers, stream-state helpers, host predicates, and theme utilities. `src\styles.css` uses CSS variables for the current theme foundation. The toolbar theme picker persists a local webview theme (`Moonlight`, `Steam Deck`, or `High contrast`), and those tokens can later be consumed by Tailwind or a component library if the visual system moves in that direction.

Host management flows use in-app React dialogs instead of browser prompts for add, rename, delete confirmation, pairing PIN display, host details, and help. The pairing dialog follows native completion events, closing on pairing success and showing pairing errors without losing focus. The Help/About dialog also reads native system information through the bridge so diagnostics like HDR support, hardware acceleration, display mode, and unmapped gamepads are visible in the web UI. This keeps the prototype on the same focus/navigation path as the rest of the shell so keyboard, touch, and controller input can exercise these core dialogs.

The Rust backend opens documentation and update links through the `open_url` bridge command so the webview never navigates itself.

Help/About documentation buttons also use the Rust `open_url` bridge command. The backend restricts this command to `http://` and `https://` URLs and reports an error when no browser is available.

App entries include `boxArtUrl` from the Rust backend. The backend fetches Sunshine artwork with the same authenticated client identity used for paired host requests, stores it in the Tauri app-data box-art cache, and emits `appChanged` events when asynchronous box-art loading completes.

For mock-backend runs, the prototype exposes a small "Controller event test" toolbar that asks the Rust side to emit controller actions. Production streaming input is forwarded from both the Tauri stream capture surface and the native Rust video window into the GameStream input FFI.

The Rust side now separates the production command surface from the in-process Rust backend and mock backend:

1. `src-tauri\src\backend.rs` keeps compatibility re-exports for DTOs and the `MoonlightBackend` trait.
2. `src-tauri\src\core\rust_backend.rs` contains the default in-process Rust backend. It uses Rust-owned host storage, settings validation, app/session state, and system-info DTOs without launching a helper process.
3. `src-tauri\src\mock_backend.rs` contains the explicit in-memory mock implementation for UI-only testing.
4. `src-tauri\src\main.rs` owns the Tauri commands, event emission, and backend registration.

The default dev and packaged backend is the in-process Rust backend, so the prototype launches without extra native processes. Use `MOONLIGHT_TAURI_BACKEND=mock` for UI-only mock testing.

To capture startup diagnostics for freezes or empty-list regressions, enable debug logging before launch:

```powershell
$env:MOONLIGHT_TAURI_DEBUG = '1'
$env:MOONLIGHT_TAURI_LOG = 'C:\Users\david\Desktop\Work\moonlight-qt\build\tauri-prototype\MoonlightTauri.log'
```

When debug logging is enabled, the Rust/Tauri layer records startup, backend selection, frontend lifecycle markers, command begin/end records, Rust backend events, and request failures. With `MOONLIGHT_TAURI_DEBUG=1`, Rust/Tauri logs are written both to the configured log file and stderr, which makes packaged diagnostics easier.

`launch_app` and `resume_session` now route through the Rust-owned Sunshine request path and C GameStream runner. `quit_running_app` interrupts the active Rust-owned stream and asks Sunshine to stop the running app. Video, audio, and input stay native and outside the webview while the React prototype tracks forwarded lifecycle state for launch/resume, active, warning/error, quitting, finished, and cleanup states. The remaining production work is real-world validation on Windows, Linux, and Steam Deck plus lower-latency platform renderer work where the temporary software presenter is not sufficient.
