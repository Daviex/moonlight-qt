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
- Successful Windows x64 release artifacts are produced at `build\deploy-x64-release\Moonlight.exe`, `build\build-x64-release\Moonlight.msi`, `build\installer-x64-release\MoonlightPortable-x64-<version>.zip`, and `build\symbols-x64-release\MoonlightDebuggingSymbols-x64-<version>.zip`. The default build is the no-QML `gui-next` Widgets UI.
- The legacy QML UI can still be built with the same script by setting `$env:QMAKE_EXTRA_ARGS = 'CONFIG+=legacy-qml'` before `cmd /c "scripts\build-arch.bat release"`.
- If the Windows build compiles but fails during symbol or portable packaging with `7z` not recognized, add 7-Zip to `PATH` and rerun the same script. Do not use MSYS2/MinGW for the normal Windows package build; the bundled Windows libraries and `AntiHooking`/Detours dependency are MSVC-built.
- Tauri prototype Windows package: install Node.js/npm and Rust/Cargo, keep the MSVC Qt/7-Zip paths above on `PATH`, then run from the repo root:
  ```powershell
  $env:PATH = "$env:USERPROFILE\.cargo\bin;C:\Qt\6.8.3\msvc2022_64\bin;C:\Program Files\7-Zip;" + $env:PATH
  $env:TAURI_PACKAGE_ZIP = '1'
  cmd /c "scripts\build-tauri-prototype.bat"
  exit $LASTEXITCODE
  ```
  This builds the native `Moonlight.exe` helper, builds the Tauri shell with `npm run tauri -- build --no-bundle`, stages `build\tauri-prototype\MoonlightTauri.exe` plus `build\tauri-prototype\native\Moonlight.exe`, and writes `Launch-Moonlight-Tauri.bat`/`Launch-Moonlight-Tauri-Debug.bat`. Set `SKIP_NATIVE_BUILD=1` only when a fresh `build\deploy-*-release\Moonlight.exe` already exists. With `TAURI_PACKAGE_ZIP=1`, the portable test artifact is `build\installer-tauri-prototype-release\MoonlightTauriPrototype-<arch>-<version>.zip`.
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

The `PKG_CONFIG_PATH` override in the generated local manifest is required because libplacebo installs `libplacebo.pc` under `/app/lib64/pkgconfig` in the KDE SDK. Without it, qmake may miss libplacebo and build Moonlight without the Vulkan renderer, which disables the HDR toggle on Steam Deck. The `QMAKE_RPATHDIR+=/app/lib64` option is also required because the libplacebo shared library is installed under `/app/lib64`; without it, the Flatpak can fail at launch with `libplacebo.so.360: cannot open shared object file`. Force `SDL_AUDIODRIVER=pipewire` and expose `xdg-run/pipewire-0` for Steam Deck audio; the PulseAudio socket alone was not sufficient on SteamOS.

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
        module.setdefault("config-opts", []).append("QMAKE_RPATHDIR+=/app/lib64")
        module["sources"] = [{"type": "dir", "path": "/mnt/c/Users/david/Desktop/Work/moonlight-qt"}]
        module.setdefault("build-options", {}).setdefault("env", {})["PKG_CONFIG_PATH"] = "/app/lib64/pkgconfig:/app/lib/pkgconfig:/app/share/pkgconfig:/usr/lib/pkgconfig:/usr/share/pkgconfig"
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
        --env=SDL_AUDIODRIVER=pipewire \
        --env=QT_QUICK_CONTROLS_STYLE=Material \
        --env=LIBVA_DRIVER_NAME= \
        --unset-env=LIBVA_DRIVER_NAME \
        --env=LIBVA_DRIVERS_PATH= \
        --unset-env=LIBVA_DRIVERS_PATH \
        --filesystem=xdg-run/gamescope-0 \
        --filesystem=xdg-run/pipewire-0 \
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

### Tauri prototype Flatpak build from Debian WSL

Do not use the native Flathub/KDE Flatpak above when the requested artifact is the new Tauri GUI. The Tauri Flatpak is a hybrid package:

- Build the native Moonlight helper in the KDE SDK/Flathub manifest path above so FFmpeg, Qt, libplacebo, Vulkan/HDR, SDL, and Sunshine/GameStream backend behavior match the normal native Flatpak.
- Build the Linux Tauri shell inside `org.gnome.Sdk//49` because KDE SDK 6.10 does not provide WebKitGTK 4.1 (`webkit2gtk-4.1`/`javascriptcoregtk-4.1`), which Tauri on Linux requires.
- Finalize/export the final app as `org.gnome.Platform//49` with command `moonlight-tauri`; copy the native helper to `/app/bin/native/Moonlight`; copy Qt/KF runtime libraries/plugins and native helper libraries from the KDE build/runtime into `/app`; keep `/app/lib64/libplacebo.so.360` available for HDR.
- The wrapper must set `MOONLIGHT_TAURI_BACKEND=ipc`, `MOONLIGHT_TAURI_HELPER=/app/bin/native/Moonlight`, `MOONLIGHT_TAURI_DEBUG=1`, `LD_LIBRARY_PATH=/app/lib/x86_64-linux-gnu:/app/lib:/app/lib64`, `QT_PLUGIN_PATH=/app/lib/plugins`, `QT_QPA_PLATFORM=offscreen`, and `WEBKIT_DISABLE_DMABUF_RENDERER=1`, then exec `/app/bin/moonlight-tauri-bin`.
- Keep `SDL_AUDIODRIVER=pipewire`, expose `xdg-run/pipewire-0`, and keep `xdg-run/gamescope-0`/`host-os:ro` for Steam Deck.

Install/update the extra WSL user runtimes/toolchains used for the Tauri Flatpak:

```sh
flatpak remote-add --user --if-not-exists flathub https://flathub.org/repo/flathub.flatpakrepo
flatpak install --user -y flathub org.gnome.Sdk//49 org.gnome.Platform//49

# If system Node/Rust are unavailable in WSL, install them under $HOME and use this PATH in GNOME SDK:
export PATH=$HOME/.cargo/bin:$HOME/node-v20.19.5-linux-x64/bin:/usr/bin:/bin
```

Build the Linux Tauri shell in GNOME SDK:

```sh
export REPO=/mnt/c/Users/david/Desktop/Work/moonlight-qt
export TAURI_BUILD=$HOME/moonlight-tauri-linux-build/gui-tauri

rm -rf "$TAURI_BUILD"
mkdir -p "$(dirname "$TAURI_BUILD")"
cp -a "$REPO/prototypes/gui-tauri" "$TAURI_BUILD"
rm -rf "$TAURI_BUILD/node_modules" "$TAURI_BUILD/dist" "$TAURI_BUILD/src-tauri/target"

flatpak run --share=network \
    --filesystem="$HOME" \
    --filesystem="$REPO" \
    --command=sh org.gnome.Sdk//49 -lc '
        set -euo pipefail
        export PATH=$HOME/.cargo/bin:$HOME/node-v20.19.5-linux-x64/bin:/usr/bin:/bin
        export PKG_CONFIG_PATH=/usr/lib/x86_64-linux-gnu/pkgconfig:/usr/lib/pkgconfig:/usr/share/pkgconfig
        cd $HOME/moonlight-tauri-linux-build/gui-tauri
        npm ci --silent
        npm run tauri -- build --no-bundle
    '
```

Before packaging the Tauri Flatpak, rebuild the native helper using the KDE/Flathub local manifest above, but remove all stale Moonlight module build dirs, not only `moonlight`:

```sh
cd "$HOME/moonlight-flatpak/com.moonlight_stream.Moonlight"
rm -rf build-dir repo .flatpak-builder/build/moonlight .flatpak-builder/build/moonlight-*
flatpak-builder --user --install-deps-from=flathub --disable-cache --force-clean --build-only build-dir com.moonlight_stream.Moonlight.local.json
```

The final Tauri app dir is created manually from those two builds. Use `flatpak build-init <final>/build-dir com.moonlight_stream.Moonlight org.gnome.Sdk org.gnome.Platform 49`, copy:

- native helper: `$HOME/moonlight-flatpak/com.moonlight_stream.Moonlight/build-dir/files/bin/moonlight` to `<final>/build-dir/files/bin/native/Moonlight`
- Tauri shell: `$HOME/moonlight-tauri-linux-build/gui-tauri/src-tauri/target/release/moonlight-gui-tauri-prototype` to `<final>/build-dir/files/bin/moonlight-tauri-bin`
- native build libraries/data: `files/lib`, `files/lib64`, and `files/share`
- Qt/KF plugin directories from the KDE runtime (`org.kde.Platform//6.10`) under `/app/lib/plugins`, plus any missing Qt/KF `.so` dependencies reported by `ldd`

Then finish/export/bundle:

```sh
flatpak build-finish "$FINAL/build-dir" \
    --command=moonlight-tauri \
    --share=network \
    --socket=fallback-x11 \
    --socket=wayland \
    --share=ipc \
    --socket=pulseaudio \
    --device=all \
    --talk-name=org.freedesktop.ScreenSaver \
    --env=IGNORE_RFI_LATENCY_BUG=1 \
    --env=SDL_AUDIODRIVER=pipewire \
    --env=QT_QUICK_CONTROLS_STYLE=Material \
    --env=LANG=C.UTF-8 \
    --env=LC_ALL=C.UTF-8 \
    --env=WEBKIT_DISABLE_DMABUF_RENDERER=1 \
    --filesystem=xdg-run/gamescope-0 \
    --filesystem=xdg-run/pipewire-0 \
    --filesystem=host-os:ro

rm -rf "$FINAL/repo"
mkdir -p "$FINAL/repo" "$OUTDIR"
flatpak build-export --disable-sandbox "$FINAL/repo" "$FINAL/build-dir"
flatpak build-bundle "$FINAL/repo" "$FINAL/moonlight-current-tauri.flatpak" com.moonlight_stream.Moonlight
cp -f "$FINAL/moonlight-current-tauri.flatpak" "$OUTDIR/moonlight-current.flatpak"
cp -f "$FINAL/moonlight-current-tauri.flatpak" "$REPO/build/Moonlight-feature-steamdeck.flatpak"
```

Validate the packaged Tauri Flatpak before handing it off:

```sh
flatpak install --user -y "$REPO/build/Moonlight-feature-steamdeck.flatpak"
timeout 12s flatpak run --user com.moonlight_stream.Moonlight//master 2>&1 | tee "$REPO/build/tauri-flatpak-smoke.log"
flatpak run --user --command=sh com.moonlight_stream.Moonlight//master -c '
  export LD_LIBRARY_PATH=/app/lib/x86_64-linux-gnu:/app/lib:/app/lib64${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}
  export QT_PLUGIN_PATH=/app/lib/plugins${QT_PLUGIN_PATH:+:$QT_PLUGIN_PATH}
  export QT_QPA_PLATFORM=offscreen
  printf "%s\n" "{\"id\":1,\"command\":{\"name\":\"list_hosts\",\"payload\":{}}}" | /app/bin/native/Moonlight --tauri-bridge-helper
'
```

Successful smoke logs should show `creating IPC backend`, `native helper spawned`, `ipc request ... command=list_hosts`, and `refreshHosts success`. Direct helper IPC should return a JSON frame with `result`.

Steam Deck install/test commands for the Tauri Flatpak:

```sh
flatpak remote-add --user --if-not-exists flathub https://flathub.org/repo/flathub.flatpakrepo
flatpak install --user -y flathub org.gnome.Platform//49
flatpak uninstall --user -y com.moonlight_stream.Moonlight || true
flatpak install --user -y ~/Downloads/Moonlight-feature-steamdeck.flatpak
flatpak run com.moonlight_stream.Moonlight 2>&1 | tee ~/moonlight-tauri-start.log
```

If the Deck says `application requires runtime org.gnome.Platform which was not found`, install `org.gnome.Platform//49` from Flathub as shown above. The final Tauri Flatpak should still expose PipeWire audio and keep the native helper linked to `/app/lib64/libplacebo.so.360` for HDR.

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
