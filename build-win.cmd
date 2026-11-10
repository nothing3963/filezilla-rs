@ECHO OFF

cd /d "%~dp0"

rem The version lives in src-tauri\tauri.conf.json only (see scripts\get-version.mjs).
rem Read it before touching NODE_OPTIONS so the shim can never interfere.
set "VER="
for /f "usebackq delims=" %%v in (`node scripts\get-version.mjs`) do set "VER=%%v"
if not defined VER (
  echo [ERROR] Could not read a version from src-tauri\tauri.conf.json.
  goto :error
)

set "TMP=%~dp0.appdata\tmp"
set "TEMP=%TMP%"
if not exist "%TMP%" mkdir "%TMP%"

set "CARGO_HOME=%~dp0.cache\cargo-home"
set "NODE_OPTIONS=--import ./scripts/vite-sandbox-shim.mjs"

echo [build] project : %~dp0
echo [build] TMP     : %TMP%
echo [build] CARGO_HOME : %CARGO_HOME%
echo [build] version : %VER%

call pnpm install --reporter=append-only --config.confirmModulesPurge=false <nul
if errorlevel 1 goto :error

call pnpm build
if errorlevel 1 goto :error

call pnpm tauri build --bundles nsis
if errorlevel 1 goto :error

set "OUT=release\%VER%"
if not exist "%OUT%" mkdir "%OUT%"
copy /Y "src-tauri\target\release\filezilla-rs.exe" "%OUT%\filezilla-rs.exe" >nul
if errorlevel 1 goto :error

rem Exact file name on purpose: a wildcard would also pick up older bundles that
rem still sit in target\release\bundle\nsis, and an exact name fails loudly if the
rem bundle version did not end up matching Cargo.toml.
copy /Y "src-tauri\target\release\bundle\nsis\FileZilla Modern_%VER%_x64-setup.exe" "%OUT%\" >nul
if errorlevel 1 goto :error

echo Build and copy succeeded!
echo   %OUT%\filezilla-rs.exe
echo   %OUT%\FileZilla Modern_%VER%_x64-setup.exe
exit /b 0

:error
echo [ERROR] A command failed. Aborting script.
exit /b 1