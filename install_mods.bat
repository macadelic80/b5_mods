@echo off
rem Build the Rust mods and install them into <game dir>\mods\<name>\ (dll + mod.ini).
rem Usage: install_mods.bat [game dir]   default: N:\refs\BP-Decomp_Workflow\build\game
setlocal
cd /d "%~dp0"
set "GAME=%~1"
if "%GAME%"=="" set "GAME=N:\refs\BP-Decomp_Workflow\build\game"
cargo build --release || exit /b 1
for %%M in (debug_menu) do (
  if not exist "%GAME%\mods\%%M" mkdir "%GAME%\mods\%%M"
  copy /Y "target\release\%%M.dll" "%GAME%\mods\%%M\" >nul || exit /b 1
  copy /Y "target\release\%%M.pdb" "%GAME%\mods\%%M\" >nul 2>nul
  copy /Y "%%M\mod.ini" "%GAME%\mods\%%M\" >nul || exit /b 1
  echo installed %%M -^> %GAME%\mods\%%M
)
