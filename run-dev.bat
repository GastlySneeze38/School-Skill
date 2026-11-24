@echo off
setlocal
echo Demarrage de l'app Revisions (Tauri dev + Vite a chaud)...
echo.

set "APP=%~dp0app"

where cargo >nul 2>nul
if errorlevel 1 (
    echo Rust ^(cargo^) est introuvable. Installe-le depuis https://rustup.rs
    pause
    exit /b 1
)

if not exist "%APP%\Frontend\node_modules" (
    echo Installation des dependances du Frontend...
    pushd "%APP%\Frontend"
    call npm install
    popd
    if errorlevel 1 (
        echo Echec de npm install.
        pause
        exit /b 1
    )
)

rem `tauri dev` fait les deux en meme temps :
rem   - lance Vite (npm --prefix Frontend run dev) : l'interface se recharge a chaud
rem     a chaque fichier enregistre dans Frontend\src ;
rem   - surveille Backend\src : chaque modif Rust recompile et relance l'app.
rem La premiere compilation Rust prend quelques minutes, les suivantes quelques secondes.
cd /d "%APP%\Backend"
call "..\Frontend\node_modules\.bin\tauri.cmd" dev
if errorlevel 1 pause
