@echo off
setlocal enableDelayedExpansion

rem Build the external C GameStream library for the Rust Tauri backend.
rem Run from the repository root. Requires MSYS2 CMake and MSVC.

set SOURCE_ROOT=%cd%
set CMAKE_EXE=C:\msys64\ucrt64\bin\cmake.exe
set BUILD_DIR=%SOURCE_ROOT%\build\moonlight-common-c
set SOURCE_DIR=%SOURCE_ROOT%\moonlight-common-c\moonlight-common-c
set OPENSSL_INCLUDE=%SOURCE_ROOT%\libs\windows\include\x64
set OPENSSL_CRYPTO=%SOURCE_ROOT%\libs\windows\lib\x64\libcrypto.lib

if not exist "%SOURCE_ROOT%\moonlight-qt.pro" (
    echo This script must be run from the moonlight-qt repository root.
    echo Current directory: %SOURCE_ROOT%
    exit /b 1
)

if not exist "%CMAKE_EXE%" (
    echo Unable to find MSYS2 CMake:
    echo %CMAKE_EXE%
    echo Install it from MSYS2 with: pacman -S --needed mingw-w64-ucrt-x86_64-cmake
    exit /b 1
)

if not exist "%SOURCE_DIR%\CMakeLists.txt" (
    echo Unable to find moonlight-common-c sources:
    echo %SOURCE_DIR%
    exit /b 1
)

if not exist "%OPENSSL_CRYPTO%" (
    echo Unable to find bundled Windows OpenSSL import library:
    echo %OPENSSL_CRYPTO%
    exit /b 1
)

for /F "usebackq delims=" %%V in (`scripts\vswhere.exe -latest -property installationPath`) do set VS_PATH=%%V
if not defined VS_PATH (
    echo Unable to find Visual Studio with scripts\vswhere.exe.
    exit /b 1
)

set VCVARS=%VS_PATH%\VC\Auxiliary\Build\vcvarsall.bat
if not exist "%VCVARS%" (
    echo Unable to find vcvarsall.bat:
    echo %VCVARS%
    exit /b 1
)

call "%VCVARS%" x64
if !ERRORLEVEL! NEQ 0 exit /b !ERRORLEVEL!

rmdir /s /q "%BUILD_DIR%" >nul 2>nul

"%CMAKE_EXE%" ^
    -S "%SOURCE_DIR%" ^
    -B "%BUILD_DIR%" ^
    -G "NMake Makefiles" ^
    -DCMAKE_BUILD_TYPE=Release ^
    -DBUILD_SHARED_LIBS=ON ^
    -DCMAKE_WINDOWS_EXPORT_ALL_SYMBOLS=ON ^
    -DOPENSSL_INCLUDE_DIR:PATH="%OPENSSL_INCLUDE%" ^
    -DOPENSSL_CRYPTO_LIBRARY:FILEPATH="%OPENSSL_CRYPTO%" ^
    -DLIB_EAY_RELEASE:FILEPATH="%OPENSSL_CRYPTO%" ^
    -DLIB_EAY_DEBUG:FILEPATH="%OPENSSL_CRYPTO%" ^
    -DCMAKE_IGNORE_PREFIX_PATH=C:/msys64/ucrt64
if !ERRORLEVEL! NEQ 0 exit /b !ERRORLEVEL!

"%CMAKE_EXE%" --build "%BUILD_DIR%" --config Release
if !ERRORLEVEL! NEQ 0 exit /b !ERRORLEVEL!

if not exist "%BUILD_DIR%\moonlight-common-c.lib" (
    echo Build finished but moonlight-common-c.lib was not produced.
    exit /b 1
)

if not exist "%BUILD_DIR%\moonlight-common-c.dll" (
    echo Build finished but moonlight-common-c.dll was not produced.
    exit /b 1
)

echo Built moonlight-common-c for the Rust Tauri backend:
echo %BUILD_DIR%\moonlight-common-c.lib
echo %BUILD_DIR%\moonlight-common-c.dll
exit /b 0
