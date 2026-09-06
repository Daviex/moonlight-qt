# Moonlight Qt Codebase Knowledge

Generated: 2026-06-04

This document is based on direct inspection of the repository rooted at `C:\Users\david\Desktop\Work\moonlight-qt`. It focuses on the runtime-critical paths, build system, feature boundaries, security-sensitive flows, and modules that future changes are most likely to touch.

Supplemental Mermaid diagrams are stored in `codebase-analysis-docs/assets/`:

- `assets/architecture.mmd`
- `assets/stream_lifecycle.mmd`
- `assets/host_discovery_pairing.mmd`
- `assets/build_release.mmd`

Follow-up studies:

- [Per-game streaming settings feasibility and work plan](PER_GAME_STREAMING_SETTINGS_FEASIBILITY.md) (2026-09-06): active-profile inheritance, game overrides, shared editor, Session ownership, CLI precedence, reset and validation plan. Includes the navigation fix baseline and its regression-test results.

## Phase 1: Explore

### Repository Shape

This is the Qt desktop client for Moonlight, an open source NVIDIA GameStream and Sunshine streaming client. The repository is organized as a qmake subdir project rather than CMake:

- `moonlight-qt.pro`: top-level qmake `subdirs` project.
- `app/`: the main Qt Quick application, QML UI, streaming implementation, settings, platform code, packaging metadata, assets, and translations.
- `moonlight-common-c/`: vendored Limelight/GameStream protocol library with modified ENet dependency.
- `qmdnsengine/`: vendored Qt mDNS library used for host discovery.
- `h264bitstream/`: vendored H.264 bitstream parser used by video handling.
- `AntiHooking/`: Windows-only DLL used to force anti-hooking behavior into the process.
- `config.tests/`: qmake configure tests for Steam Link and EGL.
- `scripts/`: platform build and packaging scripts.
- `.github/workflows/`: CI entry points for AppImage, Steam Link, Windows, macOS, and prerelease publishing.
- `wix/`: Windows MSI and bootstrapper projects.

The source tree is dominated by C++/C headers because it includes large vendored dependencies, especially `moonlight-common-c`, `qmdnsengine`, and compatibility headers. The hand-maintained application logic is concentrated under `app/`.

### File Classification

| Category | Primary locations | Notes |
|---|---|---|
| Application C++ | `app/*.cpp`, `app/backend/`, `app/gui/`, `app/settings/`, `app/streaming/` | Main runtime code, QML models, host management, streaming, input, audio, video, platform helpers. |
| QML UI | `app/gui/*.qml` | Qt Quick shell, host/app views, settings, CLI helper screens, dialogs, navigable controls. |
| Vendored protocol libraries | `moonlight-common-c/`, `qmdnsengine/`, `h264bitstream/` | Static libraries linked into `app`. These are runtime-critical but should be treated as upstream-style code. |
| Build configs | `moonlight-qt.pro`, `app/app.pro`, `globaldefs.pri`, `config.tests/` | qmake controls feature macros and platform integration. |
| CI and packaging | `.github/workflows/`, `scripts/`, `wix/`, `app/deploy/` | Builds AppImage, Steam Link package, Windows MSI/portable ZIP/bootstrapper, macOS DMG. |
| Assets and resources | `app/resources.qrc`, `app/qml.qrc`, `app/gui/assets/`, `app/languages/`, `app/SDL_GameControllerDB/`, `app/shaders/` | Embedded QML, icons, translations, controller mappings, shader resources, fonts. |
| Documentation | `README.md`, `SECURITY.md`, module README files | User/build docs and vendored-library notes. |
| Generated or local outputs | `build/`, `libs/` | Ignored for analysis except where build scripts refer to them. |

### Importance Scoring

Score meaning: `5` is an entry point, global contract, security boundary, or session-critical module; `4` is core feature logic or platform integration; `3` is supporting runtime infrastructure; `2` is packaging, assets, or focused helpers; `1` is low-risk static data.

| Score | File or area | Why it matters |
|---:|---|---|
| 5 | `moonlight-qt.pro` | Defines subprojects and build dependency order. |
| 5 | `app/app.pro` | Defines Qt modules, platform libraries, feature macros, resource files, and install behavior. |
| 5 | `app/main.cpp` | Process bootstrap, logging, QML type registration, CLI modes, settings paths, SDL/Qt environment. |
| 5 | `app/backend/computermanager.cpp` | Central host discovery, polling, persistence, pairing, app quit, and connectivity orchestration. |
| 5 | `app/backend/nvhttp.cpp` | HTTP/HTTPS API boundary to GameStream/Sunshine hosts, pinned-cert handling, launch/resume/quit/app list. |
| 5 | `app/backend/nvpairingmanager.cpp` | Pairing cryptography and certificate trust establishment. |
| 5 | `app/streaming/session.cpp` | Stream lifecycle, decoder selection, launch validation, Limelight connection, SDL event loop, cleanup. |
| 5 | `app/streaming/input/input.cpp` and siblings | User input capture, hotkeys, gamepads, mouse/keyboard forwarding. |
| 5 | `moonlight-common-c/moonlight-common-c/` | Core protocol implementation for RTSP, control, audio, video, input, STUN, and connection tests. |
| 4 | `app/gui/main.qml`, `PcView.qml`, `AppView.qml`, `StreamSegue.qml`, `SettingsView.qml` | User-visible navigation and QML control flow into models and sessions. |
| 4 | `app/gui/computermodel.cpp`, `app/gui/appmodel.cpp` | QML model bridge for hosts and apps. |
| 4 | `app/backend/nvcomputer.cpp`, `app/backend/nvapp.cpp` | Persistent host/app model and runtime state merge rules. |
| 4 | `app/backend/identitymanager.cpp` | Client certificate/key creation and persistence. |
| 4 | `app/settings/streamingpreferences.cpp` | Persistent user settings, defaults, migrations, language reload. |
| 4 | `app/backend/systemproperties.cpp` | Async capability probing used by settings, warnings, and stream validation. |
| 4 | `app/streaming/video/ffmpeg.cpp` and renderers | Decoder and renderer selection across hardware/software/platform paths. |
| 4 | `app/streaming/audio/audio.cpp` | Opus decoding and audio renderer selection/recovery. |
| 3 | `app/path.cpp` | Portable mode, cache/data/log path resolution. |
| 3 | `app/wm.cpp` | Linux/window manager/GPU environment detection. |
| 3 | `app/settings/mappingmanager.cpp`, `mappingfetcher.cpp`, `compatfetcher.cpp` | Controller DB and compatibility metadata refresh. |
| 3 | `qmdnsengine/qmdnsengine/` | mDNS discovery implementation. |
| 3 | `.github/workflows/*.yml`, `scripts/*`, `wix/*` | Operational release pipeline. |

### Build and Runtime Entry Points

- Build entry: `moonlight-qt.pro`.
- Main binary project: `app/app.pro`.
- Application process entry: `main()` in `app/main.cpp`.
- Primary QML entry: `app/gui/main.qml`.
- Host browser view: `app/gui/PcView.qml`.
- App browser view: `app/gui/AppView.qml`.
- Stream launch view/controller: `app/gui/StreamSegue.qml`.
- Main stream engine: `Session` in `app/streaming/session.cpp`.
- Limelight protocol entry: `LiStartConnection()`, `LiStopConnection()`, and callback registration through `moonlight-common-c`.

### STATE BLOCK - Phase 1

- Completed: repository map, file classification, importance scoring, entry point identification.
- Highest-confidence critical path: `main.cpp` -> QML shell -> `ComputerModel`/`AppModel` -> `ComputerManager`/`NvHTTP` -> `Session` -> `moonlight-common-c`.
- Assets created: `architecture.mmd`, `stream_lifecycle.mmd`, `host_discovery_pairing.mmd`, `build_release.mmd`.
- Remaining for later phases at this point: describe features, data flows, hidden dependencies, and change risks.

## Phase 2: High-Level Overview

### Purpose and Domain

Moonlight Qt is a cross-platform PC client for streaming games and desktops from NVIDIA GameStream-compatible hosts and Sunshine servers. It discovers hosts on the local network, pairs with them using client/server certificates, shows launchable apps, starts or resumes a remote session, and forwards local input while decoding audio/video from the host.

Target users are people who want low-latency game or desktop streaming from a PC/server to another desktop-class device. The app supports Windows, macOS, Linux, Steam Link, and AppImage-style Linux deployment.

### Main Features

- Local mDNS host discovery and manual host add.
- Host polling, wake-on-LAN, rename, delete, network testing, and compatibility checking.
- PIN-based pairing with certificate persistence.
- App list retrieval, app hiding, direct launch selection, app quit, resume running app.
- Streaming launch/resume/quit lifecycle.
- H.264, HEVC, AV1, HDR, and YUV 4:4:4 negotiation where supported.
- Platform hardware decode/rendering through FFmpeg and renderer backends such as D3D11VA, DXVA2, VideoToolbox/Metal, VAAPI, VDPAU, DRM, EGL, CUDA, Vulkan/libplacebo, SDL, and Steam Link video.
- Opus audio decode and audio renderer recovery.
- Keyboard, mouse, touch, gamepad, sensors, rumble, adaptive triggers, LED, and gamepad mouse emulation.
- Settings UI for streaming quality, audio, input, gamepad, UI language, display mode, decoder choice, codec choice, HDR/YUV444, warnings, and advanced behavior.
- Auto-update, controller DB update, compatibility metadata fetch, Discord rich presence where compiled.
- CI packaging for Windows, macOS, Linux AppImage, and Steam Link.

### Tech Stack

- UI framework: Qt Quick/QML with Qt Quick Controls.
- Application language: C++17.
- Build system: qmake `.pro` files with platform-specific qmake feature flags.
- Core protocol: vendored `moonlight-common-c`.
- Network stack: Qt Network plus raw sockets inside Limelight/ENet.
- Discovery: vendored `qmdnsengine`.
- Media: FFmpeg, Opus, SDL2/SDL3 compatibility depending on platform packaging, SDL_ttf.
- Crypto/TLS: OpenSSL through Qt/OpenSSL integration and direct OpenSSL APIs in pairing/identity code.
- Packaging: WiX Toolset 7 on Windows, `macdeployqt` and `create-dmg` on macOS, `linuxdeploy` for AppImage, Steam Link SDK for Steam Link.

### Architecture Type

The app is a native desktop client with a Qt/QML presentation layer, C++ service/model layer, and a media-streaming engine. It is not a client/server repository; the remote server is external GameStream/Sunshine software.

The dominant runtime layering is:

1. `main.cpp` initializes global paths, logging, settings, QML types, identity, and startup mode.
2. QML views create model objects and call C++ invokables.
3. `ComputerManager` owns host discovery, host state, persistence, and network tasks.
4. `NvHTTP`, `NvPairingManager`, and `IdentityManager` handle host API calls and trust.
5. `Session` owns launch validation, stream start, SDL window/input/audio/video lifetime, Limelight callbacks, and cleanup.
6. Vendored protocol/media libraries handle the packet-level streaming details.

See `assets/architecture.mmd` for the component map.

### STATE BLOCK - Phase 2

- Completed: purpose, domain, target users, feature inventory, stack, and architecture summary.
- Key architecture decision: QML is intentionally thin; runtime state and side effects live in C++ models/services.
- Important external boundary: host API calls are split between Qt HTTP/XML helpers and `moonlight-common-c` stream transport.
- Remaining for later phases at this point: map detailed data flows, feature internals, and risks.

## Phase 3: Mid-Level Technical Notes

### Component Map

| Component | Files | Responsibility |
|---|---|---|
| Bootstrap | `app/main.cpp`, `app/path.cpp` | App metadata, portable mode, paths, logging, QML registration, environment hints, CLI startup modes. |
| QML shell | `app/gui/main.qml`, `PcView.qml`, `AppView.qml`, `StreamSegue.qml`, `SettingsView.qml` | User navigation, dialogs, launch flow, settings editing, lifecycle-triggered polling. |
| QML models | `app/gui/computermodel.cpp`, `app/gui/appmodel.cpp` | Adapt C++ host/app data to QML list roles and invokable actions. |
| Host manager | `app/backend/computermanager.cpp` | mDNS/manual discovery, polling threads, host persistence, pairing tasks, app quit tasks, delayed QSettings writes. |
| Host data | `app/backend/nvcomputer.cpp`, `app/backend/nvapp.cpp` | Host/app XML parsing, state merge, persisted fields, Wake-on-LAN, address ordering, app metadata. |
| Host HTTP API | `app/backend/nvhttp.cpp` | Server info, app list, box art, launch/resume/cancel, pinned TLS errors, request timeouts. |
| Pairing and identity | `app/backend/nvpairingmanager.cpp`, `app/backend/identitymanager.cpp` | Client certificate generation, pairing challenge/response, server certificate trust. |
| Settings and system info | `app/settings/streamingpreferences.cpp`, `app/backend/systemproperties.cpp` | User preference persistence, migrations, translations, decoder/display/system capability probes. |
| Streaming session | `app/streaming/session.cpp` | Stream validation, launch request, Limelight connection, SDL window/event loop, decoder/audio/input lifetime, cleanup. |
| Video | `app/streaming/video/ffmpeg.cpp`, `app/streaming/video/ffmpeg-renderers/` | Hardware/software decoder and renderer selection, frame pacing, overlays, HDR, device reset handling. |
| Audio | `app/streaming/audio/audio.cpp` | Opus decoder, renderer choice, channel mapping, renderer recovery after submit failure. |
| Input | `app/streaming/input/*.cpp` | Keyboard/mouse/gamepad/touch forwarding, capture, shortcuts, rumble/sensors/adaptive triggers. |
| External metadata | `app/settings/mappingfetcher.cpp`, `app/settings/compatfetcher.cpp`, `app/backend/autoupdatechecker.cpp` | HTTPS fetches for controller mappings, server compatibility, and updates. |
| Release | `.github/workflows/*.yml`, `scripts/*`, `wix/*` | Cross-platform CI and package generation. |

### Primary Data Flows

#### Host Discovery and Persistence

1. `main.qml` starts polling via `ComputerManager.startPolling()` when the UI is active.
2. `ComputerManager::startPolling()` starts qmdnsengine browsing for `_nvstream._tcp.local.` if `StreamingPreferences::enableMdns` is true.
3. `MdnsPendingComputer` resolves hostnames and ports, then `PendingAddTask` calls `NvHTTP::getServerInfo()`.
4. `NvComputer` parses server XML and merges into `m_KnownHosts`.
5. `ComputerManager::saveHosts()` schedules delayed persistence through `DelayedFlushThread`.
6. `ComputerModel` receives `computerStateChanged()` and updates QML roles.

See `assets/host_discovery_pairing.mmd`.

#### Pairing

1. `PcView.qml` generates a PIN with `ComputerModel::generatePinString()`.
2. `ComputerModel::pairComputer()` calls `ComputerManager::pairHost()`.
3. `PendingPairingTask` creates `NvPairingManager`.
4. `NvPairingManager::pair()` retrieves the server certificate, derives an AES key from the PIN and salt, verifies challenge responses and signatures, and stores the server certificate only on success.
5. `ComputerManager` persists the paired host and emits `pairCompleted()` or `pairFailed()`.

Security-sensitive files: `app/backend/nvpairingmanager.cpp`, `app/backend/identitymanager.cpp`, `app/backend/nvhttp.cpp`.

#### App Browsing and Launch

1. `AppView.qml` creates `AppModel` and initializes it with host index and `ComputerManager`.
2. `AppModel::updateAppList()` reflects `NvComputer::appList` plus hidden/directLaunch local attributes.
3. Selecting an app creates `Session` through `AppModel::createSessionForApp()`.
4. `StreamSegue.qml` calls `Session::initialize()` and then `Session::start()`.
5. `Session::startConnectionAsync()` calls `NvHTTP::startApp()` to launch or resume the host app and then calls `LiStartConnection()`.

See `assets/stream_lifecycle.mmd`.

#### Stream Runtime

Once `LiStartConnection()` succeeds, `Session::exec()` creates the SDL streaming window and enters the SDL event loop. Runtime ownership is concentrated in the active `Session`:

- `SdlInputHandler` sends local input to the host using `LiSend*` APIs.
- Audio callbacks decode Opus and submit samples through the selected audio renderer.
- Video callbacks submit decode units to `IVideoDecoder` implementations.
- Window and device events can recreate the renderer/decoder or update capture/fullscreen state.
- Termination routes through `LiStopConnection()` and optional `NvHTTP::quitApp()`.

### Third-Party Integrations

| Integration | Files | Notes |
|---|---|---|
| Sunshine/GameStream host HTTP API | `app/backend/nvhttp.cpp` | Server info, app list, app assets, launch, resume, cancel. |
| Limelight protocol | `moonlight-common-c/moonlight-common-c/`, `app/streaming/session.cpp` | RTSP/control/audio/video/input streaming transport. |
| mDNS | `qmdnsengine/qmdnsengine/`, `ComputerManager` | Local host discovery. |
| STUN | `ComputerManager::PendingAddTask`, `LiFindExternalAddressIP4()` | Uses `stun.moonlight-stream.org:3478` for external IPv4 discovery. |
| Updates | `app/backend/autoupdatechecker.cpp` | Fetches `https://moonlight-stream.org/updates/qt.json`. |
| Compatibility metadata | `app/settings/compatfetcher.cpp` | Fetches `https://moonlight-stream.org/compatibility/v1`. Fails open when unavailable. |
| Controller DB | `app/settings/mappingfetcher.cpp`, `mappingmanager.cpp` | Fetches controller mappings and loads bundled/cached mappings. |
| Discord rich presence | `app/backend/richpresencemanager.cpp` | Compiled behind `HAVE_DISCORD`; uses application ID `594668102021677159`. |
| WiX | `wix/Moonlight/*`, `wix/MoonlightSetup/*` | Windows MSI and bootstrapper with firewall exception and VC redist packages. |

### Cross-Cutting Concerns

#### Security

- Client identity is a generated RSA key and self-signed X509 certificate stored in QSettings by `IdentityManager`.
- Server trust is host-specific. `NvHTTP::handleSslErrors()` only ignores TLS errors if every certificate in the error list matches the stored pinned server certificate.
- Pairing verifies server challenge signatures in `NvPairingManager::verifySignature()`, reducing MITM risk during PIN pairing.
- Launch requests include RI AES key material in query parameters. `main.cpp` redacts `rikey` and `rikeyid` in logs, but any new secret query parameter must be explicitly reviewed for logging.
- Remote host commands such as launch, resume, app list, box art, and cancel are HTTPS-only after pairing.
- Windows installer adds a firewall exception for `Moonlight.exe` in `wix/Moonlight/Product.wxs`.

#### Logging

- `main.cpp` installs `messageHandler()` and writes logs to `Path::getLogDir()`.
- Log messages redact `rikey` and `rikeyid`.
- Logs are pruned by size/count and async logging is enabled during streaming through `StreamUtils::enterAsyncLoggingMode()` and disabled with `exitAsyncLoggingMode()`.
- Crash/signal handling exists in platform-specific code paths inside `main.cpp`.

#### Persistence

- `StreamingPreferences` stores user settings in QSettings and has migration logic controlled by `CURRENT_DEFAULT_VER`.
- `ComputerManager` stores hosts in QSettings under `hosts`; it writes `hostsbackup` first, writes primary data, then removes the backup.
- `NvComputer::serialize()` and `NvComputer::isEqualSerialized()` must stay in sync when persisted host fields change.
- `NvApp` stores app-local attributes such as hidden/directLaunch in host app lists.
- `Path::initialize()` controls standard versus portable locations via `portable.dat`.

#### Concurrency

- Host polling uses `PcMonitorThread` objects and a shared `QNetworkAccessManager` per polling thread.
- Host add/pair/quit operations are QRunnable-style asynchronous tasks under `ComputerManager`.
- `ComputerManager` uses read/write locks around `NvComputer` state and a separate delayed flush mutex. The comment in `computermanager.cpp` warns that `m_DelayedFlushMutex` must not be acquired while holding an `NvComputer` lock.
- `Session` serializes active streaming with `s_ActiveSession` and `s_ActiveSessionSemaphore`.
- Decoder destruction and Limelight stop ordering are intentionally strict in `Session::exec()` and `DeferredSessionCleanupTask`.

### STATE BLOCK - Phase 3

- Completed: component map, discovery/pairing/app/stream data flows, external integrations, security/logging/persistence/concurrency concerns.
- Primary data-flow confidence: high for normal GUI streaming path and pairing path.
- Areas intentionally summarized rather than exhaustively expanded: every individual FFmpeg renderer backend and every vendored C protocol file.
- Remaining for later phases at this point: deep feature reference and non-obvious change risks.

## Phase 4: Deep Reference Section

### Application Bootstrap and Startup Modes

Business purpose: initialize a desktop streaming client with consistent paths, logs, settings, QML types, SDL/Qt behavior, and CLI-compatible startup modes.

Implementation:

- Entry point: `main()` in `app/main.cpp`.
- Global path setup: `Path::initialize()` in `app/path.cpp`.
- Settings identity: `QCoreApplication::setOrganizationName()`, `setApplicationName()`, and `setApplicationVersion()` in `main.cpp`.
- Portable mode: `main.cpp` checks for `portable.dat`, then `Path::initialize(portable)` chooses log/cache/boxart/QML cache locations.
- Logging: `messageHandler()` in `main.cpp`.
- QML registration: `qmlRegisterType<ComputerModel>()`, `qmlRegisterType<AppModel>()`, `qmlRegisterUncreatableType<Session>()`, singleton registrations for `ComputerManager`, `AutoUpdateChecker`, `SystemProperties`, `SdlGamepadKeyNavigation`, and `StreamingPreferences`.
- CLI modes: `GlobalCommandLineParser` selects `PcView.qml`, `CliStartStreamSegue.qml`, `CliQuitStreamSegue.qml`, `CliPair.qml`, or app listing.

Interactions:

- `IdentityManager` is initialized before pairing or HTTPS client certificate use.
- `SystemProperties::startAsyncLoad()` is triggered so warnings and decoder capability checks are available before streaming.
- SDL hints are configured before both QML gamepad navigation and streaming input use SDL subsystems.

Edge cases and hidden dependencies:

- QML disk cache is redirected through `QML_DISK_CACHE_PATH` to avoid unwritable or undesirable default locations.
- `QT_NO_USE_NATIVE_WINDOWS` is set on Unix to prevent native child windows from affecting QML.
- Windows AntiHooking is referenced by `AntiHookingDummyImport()` so the DLL is loaded.
- Log redaction must be updated if new sensitive launch parameters are added.

### QML Shell, Navigation, and Views

Business purpose: provide the first-run and daily user workflows: browse PCs, pair, browse apps, launch/resume, adjust settings, and view update/help/status dialogs.

Implementation:

- Main shell: `app/gui/main.qml` creates `ApplicationWindow`, `StackView`, toolbar actions, polling lifecycle, warning dialogs, and update notifications.
- Host view: `app/gui/PcView.qml` owns a `ComputerModel`, handles add/pair/wake/rename/delete/network-test workflows.
- App view: `app/gui/AppView.qml` owns an `AppModel`, handles launch/resume/quit/hide/direct-launch workflows.
- Stream segue: `app/gui/StreamSegue.qml` owns a `Session`, disables QML gamepad navigation during streaming, and reconnects UI state when the stream ends.
- Settings view: `app/gui/SettingsView.qml` edits `StreamingPreferences`.

Interactions:

- `main.qml` starts/stops `ComputerManager` polling based on app visibility and activity, with a delayed inactive stop.
- `PcView.qml` calls `ComputerModel::pairComputer()`, `wakeComputer()`, `testConnectionForComputer()`, `renameComputer()`, and `deleteComputer()`.
- `AppView.qml` uses `AppModel::createSessionForApp()` and `AppModel::quitRunningApp()`.
- `StreamSegue.qml` calls `Session::initialize()`, `Session::start()`, `Session::interrupt()`, and reacts to `sessionFinished`, `connectionStarted`, and `readyForDeletion`.

Edge cases and hidden dependencies:

- `StreamSegue.qml` waits for `SystemProperties.waitForAsyncLoad()` before `Session::initialize()`.
- QML gamepad navigation must be disabled before streaming because `Session` owns SDL gamecontroller state during a stream.
- If a CLI stream ends, `StreamSegue.qml` quits the app rather than returning to the PC list.
- App launch can be direct-launch-index based, so `AppModel::getDirectLaunchAppIndex()` matters for CLI and direct launch flows.

### Host Discovery, Polling, and Persistence

Business purpose: keep an accurate list of available hosts, discover new hosts, preserve host identity, and avoid losing user pairing data.

Implementation:

- Service: `ComputerManager` in `app/backend/computermanager.cpp`.
- Discovery: `ComputerManager::startPolling()`, `MdnsPendingComputer`, `handleMdnsServiceResolved()`, qmdnsengine browser/resolver.
- Manual add: `ComputerManager::addNewHostManually()` and `PendingAddTask`.
- Polling: `PcMonitorThread::run()` calls `NvHTTP::getServerInfo()` and periodically `NvHTTP::getAppList()`.
- Persistence: constructor loads `hostsbackup` or `hosts`; `ComputerManager::saveHosts()` triggers `DelayedFlushThread`.
- Host model: `NvComputer` in `app/backend/nvcomputer.cpp`.

Interactions:

- `ComputerModel` listens for `ComputerManager::computerStateChanged()`.
- `NvComputer::uniqueAddresses()` drives polling order across active, local, remote, IPv6, and manual addresses.
- `CompatFetcher` influences `NvComputer::isSupportedServerVersion`.
- `PendingAddTask` can run `LiTestClientConnectivity()` when manual add fails and network-blocking detection is enabled.
- `PendingAddTask` uses `LiFindExternalAddressIP4("stun.moonlight-stream.org", 3478)` for WAN address discovery.

Edge cases and hidden dependencies:

- `ComputerManager` restores from `hostsbackup` first to recover interrupted writes.
- `PcMonitorThread` does not immediately mark a host offline; it requires repeated failures.
- Host identity is UUID-based. `NvComputer::update()` asserts UUID consistency.
- Manual, local, remote, and IPv6 addresses have different update rules. A VPN reachability heuristic can intentionally avoid overwriting WAN address data.
- The delayed flush lock ordering rule is important: do not acquire `m_DelayedFlushMutex` while holding an `NvComputer` lock.

### Pairing and Certificate Trust

Business purpose: bind the client identity to a host using a PIN workflow, then use pinned host certificates for authenticated control-plane HTTPS.

Implementation:

- Client identity: `IdentityManager` in `app/backend/identitymanager.cpp`.
- Pairing service: `NvPairingManager` in `app/backend/nvpairingmanager.cpp`.
- Pairing entry: `ComputerManager::pairHost()` creates `PendingPairingTask`.
- HTTP helper: `NvHTTP::openConnection()` and SSL handling in `app/backend/nvhttp.cpp`.

Technical details:

- `IdentityManager` loads or generates a 2048-bit RSA key and self-signed X509 certificate.
- `NvPairingManager::pair()` chooses SHA-256 for newer host generations and SHA-1 for older ones, derives an AES-128-ECB key from PIN material, validates challenge responses, and verifies signatures.
- On success, the server certificate is stored on the `NvComputer`.
- `NvHTTP::handleSslErrors()` only ignores errors for the pinned server certificate.

Edge cases and hidden dependencies:

- Pairing failure triggers an `unpair` request to clean host state.
- Error handling distinguishes wrong PIN, pairing already in progress, failed pairing, and game currently running.
- The pairing process temporarily trusts the unverified server certificate only to complete the challenge flow, then verifies challenge material before persistence.
- macOS certificate/key serialization has special handling for SecureTransport expectations.

### App List, Box Art, and Local App Attributes

Business purpose: show launchable host apps, preserve local UI preferences, and support resume/quit/direct-launch flows.

Implementation:

- App data: `NvApp` in `app/backend/nvapp.cpp`.
- App list retrieval: `NvHTTP::getAppList()`.
- Box art retrieval: `NvHTTP::getBoxArt()`.
- Host app state: `NvComputer::appList` and `NvComputer::updateAppList()`.
- QML model: `AppModel` in `app/gui/appmodel.cpp`.
- Box art manager: `BoxArtManager` used by `AppModel`.

Interactions:

- `PcMonitorThread` periodically refreshes app lists during host polling.
- `AppModel::updateAppList()` filters hidden apps unless `showHiddenGames` is enabled.
- `AppModel::setAppHidden()` and `setAppDirectLaunch()` persist local attributes through `ComputerManager::clientSideAttributeUpdated()`.
- `AppView.qml` uses app running state to choose launch, resume, or quit behavior.

Edge cases and hidden dependencies:

- Local hidden/directLaunch attributes must be propagated when a fresh app list arrives from the host.
- `AppModel::getDirectLaunchAppIndex()` returns an index into the current filtered model, so hidden/direct-launch interaction matters.
- The app list can disappear if the host goes offline or becomes unpaired; `AppModel` emits `computerLost`.

### Stream Launch, Validation, and Lifecycle

Business purpose: transform a chosen host app plus user preferences into a validated, running low-latency stream, then cleanly return to the UI.

Implementation:

- QML flow: `app/gui/StreamSegue.qml`.
- Session engine: `Session` in `app/streaming/session.cpp`.
- Launch API: `NvHTTP::startApp()`.
- Protocol start/stop: `LiStartConnection()`, `LiStopConnection()`.
- Cleanup: `DeferredSessionCleanupTask`.

Technical details:

- `Session::initialize()` creates a test SDL window, initializes config, probes decoders/audio/displays, builds an ordered supported video format list, and calls `validateLaunch()`.
- `Session::validateLaunch()` checks server support, client decoder support, audio availability, HDR/YUV444/codec compatibility, forced hardware/software settings, 4K limitations, and warning conditions.
- `Session::start()` serializes active sessions with `s_ActiveSessionSemaphore`, creates `SdlInputHandler`, and starts `AsyncConnectionStartThread`.
- `Session::startConnectionAsync()` checks current game state, sets SOPS/game optimization flags, calls `NvHTTP::startApp()`, chooses packet size/reachability, then calls `LiStartConnection()`.
- `Session::exec()` creates the SDL stream window, decoder, renderer, overlay, and event loop.
- `DeferredSessionCleanupTask::run()` stops Limelight and optionally quits the host app.

Interactions:

- `StreamingPreferences` controls resolution, FPS, bitrate, audio config, window mode, codec, decoder selection, HDR, YUV444, packet size, frame pacing, and quit-after behavior.
- `SystemProperties` provides hardware decoder and display warnings before session start.
- `NvComputer` contributes host display modes, app version, GFE version, codec support, HDR support, active address, and reachability.

Edge cases and hidden dependencies:

- Only one active `Session` is allowed globally.
- Decoder destruction before fullscreen toggles or renderer recreation is deliberate on several platforms.
- `LiStopConnection()` is ordered after decoder/window cleanup to avoid callbacks racing destroyed objects.
- For remote/VPN reachability, default packet size may be reduced to 1024.
- If YUV444 is requested but unsupported and bitrate was not customized, bitrate is adjusted downward.
- Sunshine and NVIDIA hosts have different behavior around SOPS and supported display modes.

### Video Decode, Rendering, HDR, and Overlays

Business purpose: decode the remote video stream with the best available backend and present frames with low latency and correct color/HDR behavior.

Implementation:

- Decoder interface: `app/streaming/video/decoder.h`.
- FFmpeg decoder: `app/streaming/video/ffmpeg.cpp`.
- Renderer interface: `app/streaming/video/ffmpeg-renderers/renderer.h`.
- Renderer backends: `app/streaming/video/ffmpeg-renderers/`.
- Frame pacing: `app/streaming/video/ffmpeg-renderers/pacer/`.
- Overlay: `app/streaming/video/overlaymanager.cpp`.
- Decoder choice: `Session::chooseDecoder()`.

Technical details:

- `Session::initialize()` builds a codec priority list including AV1, HEVC, H.264, 10-bit, 8-bit, and 4:4:4 variants.
- `FFmpegVideoDecoder::initialize()` tries environment hints first, then hardware decoders unless force software is set, then software unless force hardware is set.
- Hardware renderer selection considers platform-specific direct renderers and frontend renderers. Examples include D3D11VA, DXVA2, VideoToolbox/Metal, VAAPI, VDPAU, DRM, EGL, CUDA, Vulkan/libplacebo, SDL, and GenericHwAccel.
- `IFFmpegRenderer` advertises capabilities such as HDR support, fullscreen-only behavior, max resolution, buffering, pacing, and external texture/export mechanisms.
- `OverlayManager` renders status/debug overlays using `ModeSeven.ttf` loaded via `Path::readDataFile()`.

Edge cases and hidden dependencies:

- AV1 and HEVC are deprioritized or masked based on host and client capabilities.
- HDR support requires both stream format and renderer capability.
- 4:4:4 support changes both codec list and bitrate expectations.
- Renderers can trigger device resets; `Session::exec()` handles recreation and IDR requests.
- On slow GPUs or KMSDRM, display mode selection favors matching stream timing rather than desktop native mode.

### Audio Decode and Playback

Business purpose: decode Opus audio from the host and play it through a platform-appropriate renderer with channel mapping and recovery behavior.

Implementation:

- Main audio file: `app/streaming/audio/audio.cpp`.
- Audio init: `initializeAudioRenderer()`.
- Audio test: `Session::testAudio()` and audio renderer test path.
- Sample decode/play: `arDecodeAndPlaySample()`.

Technical details:

- Renderer selection can be forced with `ML_AUDIO=sdl` or `ML_AUDIO=slaudio`.
- Steam Link prefers SLAudio where compiled; normal desktop uses SDL audio.
- Opus multistream decoding is configured using the selected audio configuration.
- Renderer submit failures trigger renderer/decoder teardown and periodic reinitialization attempts while dropping audio to avoid runaway latency.

Edge cases and hidden dependencies:

- `validateLaunch()` can downgrade surround audio to stereo if audio testing fails.
- Mute-on-focus-loss and play-audio-on-host preferences are enforced during runtime.
- Audio thread priority is raised except on Steam Link.

### Input, Capture, Gamepads, and UI Navigation

Business purpose: send local keyboard, mouse, touch, and controller input to the host while supporting capture hotkeys and UI gamepad navigation outside streams.

Implementation:

- Streaming input owner: `SdlInputHandler` in `app/streaming/input/input.cpp`.
- Keyboard: `app/streaming/input/keyboard.cpp`.
- Mouse: `app/streaming/input/mouse.cpp`.
- Gamepad: `app/streaming/input/gamepad.cpp`.
- Touch: `app/streaming/input/touch.cpp`.
- UI gamepad navigation: `app/gui/sdlgamepadkeynavigation.cpp`.

Technical details:

- Streaming shortcuts use Ctrl+Alt+Shift combinations, including quit, ungrab, fullscreen, stats, mouse mode, cursor hide, minimize, paste text, pointer region lock, quit-and-exit, and keyboard grab.
- Keyboard events are translated to Windows virtual key codes and sent with `LiSendKeyboardEvent2()`.
- Mouse supports relative mode, absolute mode, pointer-region lock, wheel handling, touch mouse filtering, and video-region scaling.
- Gamepad supports up to 16 controllers, multi-controller merge when disabled, battery, motion sensors, touchpad, rumble, triggers, LED, adaptive triggers, and gamepad mouse emulation.
- `SdlGamepadKeyNavigation` polls SDL gamepad events and injects Qt key events only when the Qt window is focused and no stream owns SDL gamecontroller state.

Edge cases and hidden dependencies:

- UI gamepad navigation must be disabled before streaming to avoid SDL subsystem conflicts.
- Background gamepad behavior is controlled by user settings and SDL hints.
- `NO_GAMEPAD_QUIT=1` disables the gamepad quit combo.
- Device ignore behavior can be configured through `STREAM_GAMECONTROLLER_IGNORE_DEVICES`, `STREAM_GAMECONTROLLER_IGNORE_DEVICES_EXCEPT`, and `STREAM_IGNORE_DEVICE_GUIDS`.
- `raiseAllKeys()` sends release events for tracked keys during cleanup.

### Settings, Preferences, System Capabilities, and Localization

Business purpose: persist user choices, provide safe defaults, migrate old settings, expose system capability warnings, and support runtime translation changes.

Implementation:

- Preferences singleton: `StreamingPreferences` in `app/settings/streamingpreferences.cpp`.
- Settings UI: `app/gui/SettingsView.qml`.
- System singleton: `SystemProperties` in `app/backend/systemproperties.cpp`.
- Path resolution: `Path` in `app/path.cpp`.

Technical details:

- `StreamingPreferences` exposes QML properties for resolution/FPS/bitrate, audio, UI mode, window mode, input behavior, gamepad behavior, decoder/codec, HDR, YUV444, network detection, and warning toggles.
- Defaults use `getDefaultBitrate(width, height, fps, yuv444)`, interpolating a Shield-like bitrate table and increasing for YUV444.
- Migrations are tied to `CURRENT_DEFAULT_VER`.
- Language enum entries must be appended rather than inserted because stored integer values represent user preferences.
- `StreamingPreferences::retranslate()` loads `:/languages/qml_<suffix>` and asks the QML engine to retranslate.
- `SystemProperties::startAsyncLoad()` probes displays, SDL video, decoder support, HDR support, unmapped gamepads, desktop session, XWayland, WOW64, browser availability, and Discord support.

Edge cases and hidden dependencies:

- `SettingsView.qml` saves settings on deactivation/destruction.
- Toggling mDNS in settings restarts `ComputerManager` polling.
- `SystemProperties` uses a test window for decoder/display probes, so this must remain coordinated with SDL video lifetime.
- Wayland, KMSDRM, slow GPU, and platform-specific fullscreen behavior are reflected in both settings defaults and session validation.

### External Metadata and Update Checks

Business purpose: keep runtime metadata current without shipping a new binary.

Implementation:

- Controller mappings: `MappingManager` and `MappingFetcher` in `app/settings/mappingmanager.cpp` and `mappingfetcher.cpp`.
- Compatibility metadata: `CompatFetcher` in `app/settings/compatfetcher.cpp`.
- Auto updates: `AutoUpdateChecker` in `app/backend/autoupdatechecker.cpp`.

Technical details:

- Controller DB is loaded from bundled data, cached data, and user mappings. Corrupt cached data is deleted.
- Controller DB fetch uses HTTPS, HSTS, safe redirects, and `If-Modified-Since` when possible.
- Compatibility metadata fetches a latest-supported server version and fails open on missing or malformed data.
- Auto-update fetches platform/architecture update metadata and emits `onUpdateAvailable(version, url)`.

Edge cases and hidden dependencies:

- Network failures must not block normal streaming.
- Compatibility fail-open is intentional to avoid breaking users when metadata cannot be fetched.
- Auto-update is platform-gated at compile time.

### Build, Packaging, and CI

Business purpose: produce release artifacts across supported platforms with consistent dependency and packaging behavior.

Implementation:

- CI root: `.github/workflows/build.yml`.
- Linux AppImage: `.github/workflows/build-appimage.yml` and `scripts/build-appimage.sh`.
- Steam Link: `.github/workflows/build-steamlink.yml` and `scripts/build-steamlink-app.sh`.
- Windows/macOS: `.github/workflows/build-win-mac.yml`, `scripts/build-arch.bat`, `scripts/generate-bundle.bat`, `scripts/generate-dmg.sh`.
- Prerelease publication: `.github/workflows/prerelease-builds.yml`.
- Windows installers: `wix/Moonlight/Product.wxs` and `wix/MoonlightSetup/Bundle.wxs`.

Technical details:

- CI sets `CI_VERSION` to the first six characters of the commit SHA.
- Windows builds both x64 and ARM64 binaries, collects PDBs, runs `windeployqt`, builds MSI packages, creates a portable ZIP, then bundles architecture-specific MSIs with WiX.
- macOS builds a universal app with `QMAKE_APPLE_DEVICE_ARCHS="x86_64 arm64"`, runs `macdeployqt`, optionally codesigns/notarizes, and renames the DMG with the version.
- AppImage builds custom dependencies including SDL3, sdl2-compat, SDL_ttf, libva, libplacebo, dav1d, and FFmpeg, then disables Wayland and DRM support for AppImage packaging.
- Steam Link builds with Valve's SDK and zips the app bundle.
- Master branch pushes publish prerelease artifacts through GitHub releases.

See `assets/build_release.mmd`.

Edge cases and hidden dependencies:

- Windows signed-release builds require a clean worktree and symbol deployment environment variables.
- AppImage intentionally disables Wayland and DRM to avoid bundling conflicts with host EGL/Wayland libraries.
- `moonlight-common-c` warns that it requires the bundled modified ENet; replacing it with another libenet can crash.

### Vendored Libraries

Business purpose: provide stable protocol/discovery/bitstream behavior without relying on system versions.

Implementation:

- `moonlight-common-c/moonlight-common-c/`: core GameStream client library, including RTSP, control, audio, video, input, connection testing, STUN, crypto/socket helpers, and modified ENet.
- `qmdnsengine/qmdnsengine/`: Qt mDNS implementation used by host discovery.
- `h264bitstream/`: static H.264 NAL/SEI/stream parser used by video code.

Edge cases and hidden dependencies:

- Treat `moonlight-common-c` as an external-protocol contract. Changes can affect packet-level compatibility, latency, and stream stability.
- The ENet warning in `moonlight-common-c/moonlight-common-c/README.md` is operationally important.
- Vendored qmdnsengine is part of the host discovery contract; replacing it affects polling/discovery behavior.

### STATE BLOCK - Phase 4

- Completed: feature-by-feature reference for bootstrap, UI, discovery, pairing, app list, streaming, video, audio, input, settings, metadata, CI, and vendored libraries.
- Strongest implementation anchors: `ComputerManager`, `NvHTTP`, `NvPairingManager`, `StreamingPreferences`, `SystemProperties`, `Session`, `FFmpegVideoDecoder`, `SdlInputHandler`.
- Remaining for later phases at this point: summarize non-obvious decisions, bottlenecks, security implications, and change guidance.

## Phase 5: Critical Insights

### Non-Obvious Design Decisions

- The application keeps the UI thin. Most mutable runtime behavior lives in C++ services and models, not QML.
- Host persistence is intentionally two-phase with `hostsbackup` because losing pairing certificates would be a severe user-facing regression.
- TLS errors are not globally ignored. `NvHTTP::handleSslErrors()` only accepts the stored pinned certificate.
- `ComputerManager` deliberately avoids blocking UI writes by using `DelayedFlushThread`, partly because macOS QSettings writes can stall.
- `Session` uses a global semaphore to prevent multiple simultaneous streams inside one process.
- Stream cleanup ordering is strict. Input is deleted before the UI resumes, the decoder is destroyed before `LiStopConnection()`, and SDL window teardown is coordinated with Limelight callbacks.
- UI gamepad navigation and streaming gamepad input are separate owners of SDL controller state and must be handed off cleanly.
- AppImage disables Wayland/DRM in the package build because bundling those libraries can break host EGL behavior.
- Compatibility metadata fails open because stale metadata should not prevent streaming.

### Performance Bottlenecks and Sensitive Paths

- `Session::exec()` is the highest-risk runtime loop. It handles SDL events, decoder lifecycle, window state, overlays, input, and cleanup while streaming.
- FFmpeg decoder/renderer selection in `FFmpegVideoDecoder::initialize()` and `createHwAccelRenderer()` has many platform-specific branches. Small changes can regress entire hardware classes.
- Frame pacing under `app/streaming/video/ffmpeg-renderers/pacer/` is latency-sensitive.
- `PcMonitorThread::run()` performs repeated network calls and app-list refreshes; aggressive changes can increase host load or UI churn.
- QSettings writes and host locks can deadlock or stall if lock ordering is changed.
- Audio renderer recovery intentionally drops samples after failures to avoid accumulating latency.
- Overlay rendering uses SDL_ttf surfaces and renderer notification; expensive overlay updates can affect frame timing.

### Security Implications

- Pairing, certificate storage, and pinned TLS are the main trust boundary. Changes in `NvPairingManager`, `IdentityManager`, or `NvHTTP::handleSslErrors()` need security review.
- Launch/resume URL construction includes sensitive key material. `main.cpp` currently redacts `rikey` and `rikeyid`; new secrets must be added to the redaction logic.
- `NvHTTP::openConnection()` disables HTTP/2 and persistent connection caching on newer Qt versions for compatibility/control. Re-enabling those behaviors should be tested against both Sunshine and NVIDIA hosts.
- User-supplied/manual host addresses feed network calls and persistent host records. Address canonicalization and UUID matching protect against accidental host confusion.
- The Windows installer firewall exception is deliberate because inbound/outbound behavior matters for streaming and discovery.

### Things You Must Know Before Changing Code

- If adding a persisted `NvComputer` field, update both `NvComputer::serialize()` and `NvComputer::isEqualSerialized()`.
- If adding a `StreamingPreferences::Language` enum value, append it at the end rather than inserting it.
- If touching host save logic, preserve the lock ordering rule involving `m_DelayedFlushMutex` and `NvComputer` locks.
- If touching stream cleanup, preserve the order around input deletion, decoder destruction, SDL window destruction, `LiStopConnection()`, and `DeferredSessionCleanupTask`.
- If adding a new codec or video format, update the supported format list, host capability checks, client decoder checks, renderer capability checks, and validation warnings together.
- If adding a sensitive host query parameter, update log redaction in `main.cpp`.
- If modifying gamepad behavior, validate both `SdlGamepadKeyNavigation` outside streams and `SdlInputHandler` during streams.
- If modifying build flags in `app/app.pro`, audit all platform macros because many source paths are compiled conditionally.
- If updating vendored `moonlight-common-c`, keep the modified ENet dependency aligned.
- If changing AppImage dependencies, re-evaluate the Wayland/DRM bundling comments in `scripts/build-appimage.sh`.

### Known Risks and Open Questions

- There is no single automated test suite visible from the inspected root. Much of the confidence likely comes from manual/platform testing and CI builds.
- The code has substantial platform-specific branching, especially in video, windowing, input capture, and packaging.
- Decoder and renderer behavior depends on runtime drivers, window systems, FFmpeg build configuration, and environment variables.
- Network and host behavior differs between NVIDIA GameStream and Sunshine; validation logic often contains host-specific handling.
- QML UI behavior depends on asynchronous signals from C++ worker tasks, so race conditions are possible around host disappearance, stream teardown, and view navigation.

### STATE BLOCK - Phase 5

- Completed: non-obvious decisions, bottlenecks, security implications, must-know change rules, and residual risks.
- Final artifact status: this master document and Mermaid supplemental assets are stored under `codebase-analysis-docs/`.
- Recommended first files for future maintainers: `app/main.cpp`, `app/backend/computermanager.cpp`, `app/backend/nvhttp.cpp`, `app/backend/nvpairingmanager.cpp`, `app/streaming/session.cpp`, `app/settings/streamingpreferences.cpp`, and `app/app.pro`.
