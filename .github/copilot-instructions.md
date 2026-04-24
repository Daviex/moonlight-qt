# Moonlight Qt Copilot Instructions

## Build and test commands

Run submodule setup before any build that depends on vendored code or prebuilts:

```sh
git submodule update --init --recursive
```

For day-to-day development on Linux or macOS, the root project is qmake-based:

```sh
qmake6 moonlight-qt.pro
make debug
# or
make release
```

The built desktop binary ends up at `app/moonlight`. Root configuration also runs the compile checks in `config.tests/SL` and `config.tests/EGL` during `qmake`.

For embedded builds, pass the qmake config flag up front:

```sh
qmake6 "CONFIG+=embedded" moonlight-qt.pro
make release
```

Windows packaging scripts are intended to run from the repository root inside a Qt command prompt with the desired MSVC Qt kit already on `PATH`:

```bat
scripts\build-arch.bat Release
scripts\generate-bundle.bat Release
```

CI also uses the platform packaging scripts directly:

```sh
scripts/build-appimage.sh
STEAMLINK_SDK_PATH=/path/to/steamlink-sdk scripts/build-steamlink-app.sh
```

There is no repo-wide test target in the root qmake project. The explicit automated tests that ship in-tree are the vendored `qmdnsengine` tests, which use CMake instead of qmake:

```sh
cmake -S qmdnsengine/qmdnsengine -B build/qmdnsengine-tests -DBUILD_TESTS=ON -DBUILD_SHARED_LIBS=OFF
cmake --build build/qmdnsengine-tests
ctest --test-dir build/qmdnsengine-tests --output-on-failure
ctest --test-dir build/qmdnsengine-tests -R TestDns --output-on-failure
```

## High-level architecture

The root `moonlight-qt.pro` project is a qmake `subdirs` build. It builds `moonlight-common-c`, `qmdnsengine`, `h264bitstream`, and on Windows `AntiHooking`, then links them into `app`. `moonlight-common-c` is the low-level streaming stack (`Limelight.h` and the C transport/control/audio/video pipeline), `qmdnsengine` provides multicast DNS discovery, and `h264bitstream` supplies bitstream parsing helpers used by the app.

`app/main.cpp` is the real composition root for both GUI and CLI usage. The same executable handles `list`, `pair`, `stream`, and `quit` actions via `GlobalCommandLineParser`, then either runs headless CLI code or loads QML. When running the GUI path, `main.cpp` registers the QML-facing C++ types and singletons (`ComputerManager`, `StreamingPreferences`, `SystemProperties`, `AutoUpdateChecker`, `SdlGamepadKeyNavigation`, models) before loading `qrc:/gui/main.qml`.

The QML UI in `app/gui/` is a stack-based shell around those registered services. `main.qml` owns the top-level `ApplicationWindow`, pushes the initial view from the `initialView` context property, starts and stops host polling based on focus/visibility, and delegates stateful work to the registered singletons instead of doing network or persistence work directly. Every QML file used at runtime is listed in `app/qml.qrc`.

Most host/discovery/business logic lives under `app/backend/`. `ComputerManager` is the central coordinator for known hosts, mDNS discovery, polling, pairing, host persistence, and quit requests. It uses `QMdnsEngine` for discovery and `NvHTTP` for Sunshine/GameStream HTTP/XML traffic. `StreamingPreferences` and related classes under `app/settings/` hold persistent user configuration and translation state that is shared by both CLI and GUI flows.

The streaming runtime is centered on `app/streaming/session.*`. `Session` bridges the Qt/QML shell to `moonlight-common-c`, configures audio/video/input callbacks, manages the SDL window/input path, and selects the platform decoder/renderer implementation. The renderer/decoder matrix is split across `app/streaming/video/ffmpeg-renderers/`, Steam Link-specific files, and platform gates in `app/app.pro`.

## Key conventions

- Prefer simplifying the existing Qt Quick / QML GUI over replacing it with another frontend stack. For GUI-specific structure, constraints, and rewrite boundaries, read `GUI_EDITING.md`.
- Treat the vendored directories in `.gitmodules` as part of the build contract. The repo expects checked-out submodules for `moonlight-common-c/moonlight-common-c`, `qmdnsengine/qmdnsengine`, `h264bitstream/h264bitstream`, `app/SDL_GameControllerDB`, and `libs`.
- The root build is qmake, but not every dependency follows qmake conventions. `qmdnsengine` keeps its own CMake-based tests, so discovery changes may need validation there even though the main app is built from `moonlight-qt.pro`.
- Keep new application code in the existing buckets: `backend/` for host/network orchestration, `streaming/` for runtime streaming/audio/video/input, `settings/` for persisted preferences and compatibility data, `gui/` for QML, and `cli/` for action-specific command-line flows.
- When exposing new C++ functionality to QML, wire it in `app/main.cpp` with the existing `qmlRegisterType` / `qmlRegisterSingletonType` pattern and add any new runtime QML files to `app/qml.qrc`. QML loads from `qrc:/gui/...`, not from source-relative filesystem paths.
- `StreamingPreferences` enums are persistence-sensitive. New enum members should be appended rather than inserted because stored user settings depend on stable numeric values.
- Platform feature selection is compile-time first, not purely runtime. If you add a new renderer/decoder or optional dependency, update `app/app.pro` and the relevant `CONFIG` / `packagesExist(...)` / compile-test gates alongside the implementation.
- Windows and macOS rely on the checked-in prebuilts under `libs` unless `disable-prebuilts` is set; Linux/Unix prefers `pkg-config`. Do not assume dependency discovery works the same way on all platforms.
- The single binary serves both GUI and CLI flows. Changes in startup, argument parsing, or singleton initialization should be checked against both `app/cli/*` and the normal QML startup path in `main.cpp`.

## Working style merged from `CLAUDE.MD`

- Make assumptions explicit when repo behavior is ambiguous, especially around platform-specific build flags, CLI vs GUI startup, and optional dependencies.
- Prefer the smallest change that solves the requested problem. Avoid speculative abstractions, opportunistic refactors, or cleanup outside the requested scope.
- Keep edits surgical: match the surrounding style and only remove code that your own change makes obsolete.
- For multi-step work, define concrete success checks up front instead of stopping at a plausible implementation.
