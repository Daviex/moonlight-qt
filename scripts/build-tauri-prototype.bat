@echo off
setlocal enableDelayedExpansion

rem Build and stage the isolated Tauri prototype with the native Moonlight helper.
rem Run from the repository root. Set SKIP_NATIVE_BUILD=1 to reuse an existing native build.

set SOURCE_ROOT=%cd%
set BUILD_CONFIG=release
set TAURI_ROOT=%SOURCE_ROOT%\prototypes\gui-tauri
set TAURI_EXE=%TAURI_ROOT%\src-tauri\target\release\moonlight-gui-tauri-prototype.exe
set PACKAGE_DIR=%SOURCE_ROOT%\build\tauri-prototype

if not exist "%SOURCE_ROOT%\moonlight-qt.pro" (
    echo This script must be run from the moonlight-qt repository root.
    echo Current directory: %SOURCE_ROOT%
    exit /b 1
)

if not exist "%TAURI_ROOT%\package.json" (
    echo Unable to find the Tauri prototype package:
    echo %TAURI_ROOT%\package.json
    exit /b 1
)

if not exist "%TAURI_ROOT%\src-tauri\Cargo.toml" (
    echo Unable to find the Tauri Cargo manifest:
    echo %TAURI_ROOT%\src-tauri\Cargo.toml
    exit /b 1
)

call :RequireCommand npm "Unable to find npm. Install Node.js and npm first."
if !ERRORLEVEL! NEQ 0 exit /b !ERRORLEVEL!

call :RequireCommand cargo "Unable to find cargo. Install Rust with rustup first."
if !ERRORLEVEL! NEQ 0 exit /b !ERRORLEVEL!

call :RequireCommand rustc "Unable to find rustc. Install Rust with rustup first."
if !ERRORLEVEL! NEQ 0 exit /b !ERRORLEVEL!

for /F "delims=" %%V in ('npm --version') do set NPM_VERSION=%%V
for /F "delims=" %%V in ('cargo --version') do set CARGO_VERSION=%%V
for /F "delims=" %%V in ('rustc --version') do set RUSTC_VERSION=%%V
echo Using npm !NPM_VERSION!, !CARGO_VERSION!, !RUSTC_VERSION!

set WEBVIEW2_FOUND=
reg query "HKCU\Software\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}" /v pv >nul 2>nul
if !ERRORLEVEL! EQU 0 set WEBVIEW2_FOUND=1
if not defined WEBVIEW2_FOUND (
    reg query "HKLM\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}" /v pv >nul 2>nul
    if !ERRORLEVEL! EQU 0 set WEBVIEW2_FOUND=1
)
if not defined WEBVIEW2_FOUND (
    echo Warning: Microsoft Edge WebView2 Runtime was not detected in the standard registry locations.
    echo The Tauri shell may fail to launch on systems without WebView2 installed.
)

if not "%SKIP_NATIVE_BUILD%"=="1" (
    if not exist "%SOURCE_ROOT%\scripts\build-arch.bat" (
        echo Unable to find scripts\build-arch.bat. Run this script from the repository root.
        exit /b 1
    )
)

pushd "%TAURI_ROOT%"
if !ERRORLEVEL! NEQ 0 (
    echo Unable to enter Tauri prototype directory:
    echo %TAURI_ROOT%
    exit /b 1
)

if not exist node_modules (
    call npm install
    if !ERRORLEVEL! NEQ 0 exit /b !ERRORLEVEL!
)

if not exist node_modules\.bin\tauri.cmd (
    echo Tauri CLI dependency is missing from node_modules. Refreshing npm dependencies.
    call npm install
    if !ERRORLEVEL! NEQ 0 exit /b !ERRORLEVEL!
)

if not exist node_modules\.bin\tauri.cmd (
    echo Unable to find the local Tauri CLI after npm install:
    echo %TAURI_ROOT%\node_modules\.bin\tauri.cmd
    popd
    exit /b 1
)
popd

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

(
    echo @echo off
    echo set "MOONLIGHT_TAURI_BACKEND=ipc"
    echo set "MOONLIGHT_TAURI_HELPER=%%~dp0native\Moonlight.exe"
    echo set "MOONLIGHT_TAURI_DEBUG=1"
    echo set "MOONLIGHT_TAURI_LOG=%%~dp0MoonlightTauri.log"
    echo start "" "%%~dp0MoonlightTauri.exe"
) > "%PACKAGE_DIR%\Launch-Moonlight-Tauri-Debug.bat"

echo Tauri prototype package staged at:
echo %PACKAGE_DIR%
echo Run Launch-Moonlight-Tauri.bat to start the Tauri shell with the native helper.
echo Run Launch-Moonlight-Tauri-Debug.bat to capture MoonlightTauri.log beside the staged executable.
exit /b 0

:RequireCommand
where %~1 >nul 2>nul
if !ERRORLEVEL! NEQ 0 (
    echo %~2
    exit /b 1
)
exit /b 0
