@echo off
setlocal enableDelayedExpansion

rem Build and stage the isolated Tauri prototype with the native Moonlight helper.
rem Run from the repository root. Set SKIP_NATIVE_BUILD=1 to reuse an existing native build.

set SOURCE_ROOT=%cd%
set BUILD_CONFIG=release
set TAURI_ROOT=%SOURCE_ROOT%\prototypes\gui-tauri
set TAURI_EXE=%TAURI_ROOT%\src-tauri\target\release\moonlight-gui-tauri-prototype.exe
set PACKAGE_DIR=%SOURCE_ROOT%\build\tauri-prototype

where npm >nul 2>nul
if !ERRORLEVEL! NEQ 0 (
    echo Unable to find npm. Install Node.js and npm first.
    exit /b 1
)

where cargo >nul 2>nul
if !ERRORLEVEL! NEQ 0 (
    echo Unable to find cargo. Install Rust with rustup first.
    exit /b 1
)

if not "%SKIP_NATIVE_BUILD%"=="1" (
    call "%SOURCE_ROOT%\scripts\build-arch.bat" %BUILD_CONFIG%
    if !ERRORLEVEL! NEQ 0 exit /b !ERRORLEVEL!
)

set HELPER_EXE=
for %%A in (x64 arm64 x86) do (
    if not defined HELPER_EXE (
        if exist "%SOURCE_ROOT%\build\deploy-%%A-%BUILD_CONFIG%\Moonlight.exe" (
            set HELPER_EXE=%SOURCE_ROOT%\build\deploy-%%A-%BUILD_CONFIG%\Moonlight.exe
            set HELPER_DIR=%SOURCE_ROOT%\build\deploy-%%A-%BUILD_CONFIG%
        )
    )
)

if not defined HELPER_EXE (
    echo Unable to find a native helper build under build\deploy-*-release\Moonlight.exe.
    echo Run scripts\build-arch.bat release first or unset SKIP_NATIVE_BUILD.
    exit /b 1
)

pushd "%TAURI_ROOT%"
if not exist node_modules (
    call npm install
    if !ERRORLEVEL! NEQ 0 exit /b !ERRORLEVEL!
)

call npm run tauri -- build --no-bundle
if !ERRORLEVEL! NEQ 0 exit /b !ERRORLEVEL!
popd

if not exist "%TAURI_EXE%" (
    echo Tauri executable was not produced: %TAURI_EXE%
    exit /b 1
)

rmdir /s /q "%PACKAGE_DIR%" >nul 2>nul
mkdir "%PACKAGE_DIR%\native"
if !ERRORLEVEL! NEQ 0 exit /b !ERRORLEVEL!

copy "%TAURI_EXE%" "%PACKAGE_DIR%\MoonlightTauri.exe" >nul
if !ERRORLEVEL! NEQ 0 exit /b !ERRORLEVEL!

xcopy "%HELPER_DIR%\*" "%PACKAGE_DIR%\native\" /E /I /Y >nul
if !ERRORLEVEL! NEQ 0 exit /b !ERRORLEVEL!

(
    echo @echo off
    echo set "MOONLIGHT_TAURI_BACKEND=ipc"
    echo set "MOONLIGHT_TAURI_HELPER=%%~dp0native\Moonlight.exe"
    echo start "" "%%~dp0MoonlightTauri.exe"
) > "%PACKAGE_DIR%\Launch-Moonlight-Tauri.bat"

echo Tauri prototype package staged at:
echo %PACKAGE_DIR%
echo Run Launch-Moonlight-Tauri.bat to start the Tauri shell with the native helper.
exit /b 0
