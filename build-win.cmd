@ECHO OFF

cd /d "%~dp0"

set "TMP=%~dp0.appdata\tmp"
set "TEMP=%TMP%"
if not exist "%TMP%" mkdir "%TMP%"

set "CARGO_HOME=%~dp0.cache\cargo-home"
set "NODE_OPTIONS=--import ./scripts/vite-sandbox-shim.mjs"

echo [build] project : %~dp0
echo [build] TMP     : %TMP%
echo [build] CARGO_HOME : %CARGO_HOME%

call pnpm install --reporter=append-only --config.confirmModulesPurge=false <nul
if errorlevel 1 goto :error

call pnpm build
if errorlevel 1 goto :error

call pnpm tauri build --bundles nsis
if errorlevel 1 goto :error

if not exist "release\0.1.0" mkdir "release\0.1.0"
copy /Y "src-tauri\target\release\filezilla-rs.exe" "release\0.1.0\filezilla-rs.exe" >nul
if errorlevel 1 goto :error

xcopy "src-tauri\target\release\bundle\nsis\FileZilla Modern_*_x64-setup.exe" "release\0.1.0\" /Y
if errorlevel 1 goto :error

echo Build and copy succeeded!
echo   release\0.1.0\filezilla-rs.exe
echo   release\0.1.0\FileZilla Modern_0.1.0_x64-setup.exe
exit /b 0

:error
echo [ERROR] A command failed. Aborting script.
exit /b 1