# Copilot instructions for moonlight-qt

## Build, test, and lint commands

- Initialize dependencies before building: `git submodule update --init --recursive`.
- Development build on Linux/macOS: `qmake6 moonlight-qt.pro` then `make debug` or `make release`. Use `qmake` instead of `qmake6` for Qt 5 builds.
- Embedded/device build variants are qmake configs, for example `qmake6 "CONFIG+=embedded" moonlight-qt.pro`; add `"CONFIG+=gpuslow"` for platforms that should prefer direct KMSDRM rendering over GL/Vulkan renderers.
- Windows package builds run from the repo root in a Qt command prompt: `scripts\build-arch.bat Release x64`, `scripts\build-arch.bat Release arm64`, then `scripts\generate-bundle.bat Release`.
- macOS package build: `scripts/generate-dmg.sh Release`.
- Linux AppImage package build: `scripts/build-appimage.sh`.
- Steam Link package build: set `STEAMLINK_SDK_PATH` and run `scripts/build-steamlink-app.sh`.
- Existing automated tests are in the vendored qmdnsengine CMake project:
  - Full qmdnsengine tests: `cmake -S qmdnsengine/qmdnsengine -B build/qmdnsengine-tests -DBUILD_TESTS=ON && cmake --build build/qmdnsengine-tests && ctest --test-dir build/qmdnsengine-tests --output-on-failure`
  - Single qmdnsengine test: `ctest --test-dir build/qmdnsengine-tests -R TestDns --output-on-failure`

## High-level architecture

- `moonlight-qt.pro` is a qmake `subdirs` project. It builds `moonlight-common-c`, `qmdnsengine`, `h264bitstream`, `app`, and Windows-only `AntiHooking`; `app` depends on the libraries.
- `app/app.pro` is the main Qt Quick application. It uses C++17, compiles QML/resources/translations, links platform dependencies, and conditionally enables renderers/decoders via qmake configs such as `ffmpeg`, `libplacebo`, `config_SL`, `embedded`, `glslow`, `vkslow`, and `gpuslow`.
- `app/main.cpp` owns process-wide setup: logging, SDL/Qt platform hints, signal handling, command-line dispatch, QML type registration, Material theme setup, and loading `qrc:/gui/main.qml`.
- `app/backend` contains host discovery, pairing, GameStream/Sunshine HTTP interaction, box art, updates, and rich presence. `ComputerManager` is the central host manager and uses qmdnsengine for mDNS discovery.
- `app/gui` contains QML screens and C++ `QAbstractListModel` adapters (`ComputerModel`, `AppModel`) that expose backend state to QML.
- `app/streaming` contains session startup/runtime, Limelight integration, input handling, audio renderers, video decoders/renderers, bandwidth, and overlays.
- `app/settings` contains persistent streaming preferences, compatibility/mapping fetchers, and gamepad mapping management. Preferences are exposed to QML as a singleton.
- `app/cli` implements non-GUI actions (`list`, `quit`, `stream`, `pair`) that are selected in `main.cpp` before the QML UI is loaded.

## Key conventions

- Add new C++ source/header/resource/translation files to the relevant `.pro` file; qmake file lists are explicit and are not auto-discovered.
- Expose C++ to QML through `qmlRegisterType`, `qmlRegisterSingletonType`, `Q_PROPERTY`, `Q_ENUM`, and `Q_INVOKABLE` patterns in existing QML-facing classes. Update QML imports and resource files when adding UI-visible types or QML files.
- Keep new `StreamingPreferences::Language` enum values at the end to avoid renumbering stored user preferences, and update language resources/translations consistently.
- `NvComputer` separates ephemeral and persisted traits. When adding persisted fields, update serialization and `isEqualSerialized()` together.
- Respect existing object ownership and threading patterns in `ComputerManager`, `Session`, and singleton-style managers such as `StreamingPreferences` and `IdentityManager`; avoid ad-hoc cross-thread ownership.
