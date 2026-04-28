# Copilot instructions for moonlight-qt

## Build, test, and lint commands

- Initialize dependencies before building: `git submodule update --init --recursive`.
- Development build on Linux/macOS: `qmake6 moonlight-qt.pro` then `make debug` or `make release`. Use `qmake` instead of `qmake6` for Qt 5 builds.
- Embedded/device build variants are qmake configs, for example `qmake6 "CONFIG+=embedded" moonlight-qt.pro`; add `"CONFIG+=gpuslow"` for platforms that should prefer direct KMSDRM rendering over GL/Vulkan renderers.
- Windows package builds run from the repo root in a Qt command prompt: `scripts\build-arch.bat Release x64`, `scripts\build-arch.bat Release arm64`, then `scripts\generate-bundle.bat Release`.
- Validated local Windows x64 build path: use the MSVC Qt qmake on `PATH` (for example `C:\Qt\6.8.3\msvc2022_64\bin`) and ensure 7-Zip is on `PATH` (for example `C:\Program Files\7-Zip`). The build script discovers Visual Studio with `scripts\vswhere.exe` and calls `vcvarsall.bat` automatically.
- Validated PowerShell command for a logged Windows x64 release build:
  ```powershell
  $env:PATH = 'C:\Qt\6.8.3\msvc2022_64\bin;C:\Program Files\7-Zip;' + $env:PATH
  cmd /c "scripts\build-arch.bat release" 2>&1 | Tee-Object -FilePath .\build\build-msvc-release.log
  exit $LASTEXITCODE
  ```
- Successful Windows x64 release artifacts are produced at `build\deploy-x64-release\Moonlight.exe`, `build\build-x64-release\Moonlight.msi`, `build\installer-x64-release\MoonlightPortable-x64-<version>.zip`, and `build\symbols-x64-release\MoonlightDebuggingSymbols-x64-<version>.zip`.
- If the Windows build compiles but fails during symbol or portable packaging with `7z` not recognized, add 7-Zip to `PATH` and rerun the same script. Do not use MSYS2/MinGW for the normal Windows package build; the bundled Windows libraries and `AntiHooking`/Detours dependency are MSVC-built.
- macOS package build: `scripts/generate-dmg.sh Release`.
- Linux AppImage package build: `scripts/build-appimage.sh`.
- Steam Link package build: set `STEAMLINK_SDK_PATH` and run `scripts/build-steamlink-app.sh`.
- Existing automated tests are in the vendored qmdnsengine CMake project:
  - Full qmdnsengine tests: `cmake -S qmdnsengine/qmdnsengine -B build/qmdnsengine-tests -DBUILD_TESTS=ON && cmake --build build/qmdnsengine-tests && ctest --test-dir build/qmdnsengine-tests --output-on-failure`
  - Single qmdnsengine test: `ctest --test-dir build/qmdnsengine-tests -R TestDns --output-on-failure`

### Flatpak package build from Debian WSL

The Flatpak manifest is not stored in this repository. Build it from Debian WSL with the external Flathub manifest repo. Use Flatpak Builder for the build-only step, then finalize/export/bundle with `flatpak` directly to avoid the AppStream compose mismatch seen with older `flatpak-builder`/KDE SDK combinations.

Install/update the WSL tooling:

```sh
sudo apt update
sudo apt install -y flatpak flatpak-builder git python3 appstream-compose appstream-util
flatpak remote-add --user --if-not-exists flathub https://flathub.org/repo/flathub.flatpakrepo
```

Do not replace `/usr/bin/appstream-compose` with `/usr/libexec/appstreamcli-compose`. They are not CLI-compatible: `flatpak-builder` may pass old `appstream-compose` options such as `--basename`, which `appstreamcli-compose` rejects.

Build the current checkout with the Flathub manifest:

```sh
export REPO=/mnt/c/Users/david/Desktop/Work/moonlight-qt
export WORKDIR=$HOME/moonlight-flatpak
export OUTDIR=/mnt/c/Users/david/Desktop/Work/moonlight-flatpak-output
export LOG=$REPO/build/build-wsl-flatpak-final.log

mkdir -p "$REPO/build" "$WORKDIR"
cd "$WORKDIR"
if [ ! -d com.moonlight_stream.Moonlight ]; then
    git clone https://github.com/flathub/com.moonlight_stream.Moonlight.git
fi
cd com.moonlight_stream.Moonlight
git fetch --depth 1 origin
git reset --hard origin/master

python3 - <<'PY'
import json
from pathlib import Path

p = Path("com.moonlight_stream.Moonlight.json")
data = json.loads(p.read_text())
data.pop("rename-icon", None)

for module in data["modules"]:
    if module.get("name") == "moonlight":
        module["sources"] = [{"type": "dir", "path": "/mnt/c/Users/david/Desktop/Work/moonlight-qt"}]
        break
else:
    raise SystemExit("moonlight module not found")

Path("com.moonlight_stream.Moonlight.local.json").write_text(json.dumps(data, indent=2) + "\n")
PY

rm -rf build-dir repo .flatpak-builder/build/moonlight

{
    flatpak-builder --user --install-deps-from=flathub --disable-cache --force-clean --build-only build-dir com.moonlight_stream.Moonlight.local.json

    flatpak build-finish build-dir \
        --command=moonlight \
        --share=network \
        --socket=fallback-x11 \
        --socket=wayland \
        --share=ipc \
        --socket=pulseaudio \
        --device=all \
        --talk-name=org.freedesktop.ScreenSaver \
        --env=IGNORE_RFI_LATENCY_BUG=1 \
        --env=QT_QUICK_CONTROLS_STYLE=Material \
        --env=LIBVA_DRIVER_NAME= \
        --unset-env=LIBVA_DRIVER_NAME \
        --env=LIBVA_DRIVERS_PATH= \
        --unset-env=LIBVA_DRIVERS_PATH \
        --filesystem=xdg-run/gamescope-0 \
        --filesystem=host-os:ro

    mkdir -p "$WORKDIR/repo" "$OUTDIR"
    flatpak build-export --disable-sandbox "$WORKDIR/repo" build-dir
    flatpak build-bundle "$WORKDIR/repo" "$WORKDIR/moonlight-current.flatpak" com.moonlight_stream.Moonlight
    cp -f "$WORKDIR/moonlight-current.flatpak" "$OUTDIR/moonlight-current.flatpak"
    cp -f "$WORKDIR/moonlight-current.flatpak" "$REPO/build/Moonlight-feature-steamdeck.flatpak"
} 2>&1 | tee "$LOG"
```

If the manual export warns about CRLF desktop files or the icon still being named `moonlight.svg`, normalize the finished app dir before rebundling:

```sh
cd "$WORKDIR/com.moonlight_stream.Moonlight"
for f in build-dir/files/share/applications/com.moonlight_stream.Moonlight.desktop build-dir/export/share/applications/com.moonlight_stream.Moonlight.desktop; do
    [ -f "$f" ] && sed -i 's/\r$//' "$f" && sed -i 's/^Icon=moonlight$/Icon=com.moonlight_stream.Moonlight/' "$f"
done
mkdir -p build-dir/files/share/icons/hicolor/scalable/apps build-dir/export/share/icons/hicolor/scalable/apps
if [ -f build-dir/files/share/icons/hicolor/scalable/apps/moonlight.svg ]; then
    cp -f build-dir/files/share/icons/hicolor/scalable/apps/moonlight.svg build-dir/files/share/icons/hicolor/scalable/apps/com.moonlight_stream.Moonlight.svg
    cp -f build-dir/files/share/icons/hicolor/scalable/apps/moonlight.svg build-dir/export/share/icons/hicolor/scalable/apps/com.moonlight_stream.Moonlight.svg
fi
rm -rf "$WORKDIR/repo"
mkdir -p "$WORKDIR/repo"
flatpak build-export --disable-sandbox "$WORKDIR/repo" build-dir
flatpak build-bundle "$WORKDIR/repo" "$WORKDIR/moonlight-current.flatpak" com.moonlight_stream.Moonlight
cp -f "$WORKDIR/moonlight-current.flatpak" "$OUTDIR/moonlight-current.flatpak"
cp -f "$WORKDIR/moonlight-current.flatpak" "$REPO/build/Moonlight-feature-steamdeck.flatpak"
```

Verified output paths from WSL packaging: `build\Moonlight-feature-steamdeck.flatpak`, `C:\Users\david\Desktop\Work\moonlight-flatpak-output\moonlight-current.flatpak`, and logs under `build\build-wsl-flatpak-*.log`.

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
