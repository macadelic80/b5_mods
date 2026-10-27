@echo off
rem Build the Rust mods (wasm32-unknown-unknown, see .cargo\config.toml) and install them into
rem <game dir>\mods\<name>\ (<name>.wasm + mod.ini).
rem Usage: install_mods.bat [game dir]   default: N:\refs\BP-Decomp_Workflow\build\game
setlocal
cd /d "%~dp0"
set "GAME=%~1"
if "%GAME%"=="" set "GAME=N:\refs\BP-Decomp_Workflow\build\game"
cargo build --release || exit /b 1
for %%M in (debug_menu) do (
  if not exist "%GAME%\mods\%%M\data" mkdir "%GAME%\mods\%%M\data"
  copy /Y "target\wasm32-unknown-unknown\release\%%M.wasm" "%GAME%\mods\%%M\" >nul || exit /b 1
  copy /Y "%%M\mod.ini" "%GAME%\mods\%%M\" >nul || exit /b 1
  rem The DLL version kept its files next to mod.ini; the sandbox only reaches data\.
  for %%F in (positions.txt favourites.txt) do (
    if exist "%GAME%\mods\%%M\%%F" if not exist "%GAME%\mods\%%M\data\%%F" copy /Y "%GAME%\mods\%%M\%%F" "%GAME%\mods\%%M\data\" >nul
  )
  echo installed %%M -^> %GAME%\mods\%%M
)
