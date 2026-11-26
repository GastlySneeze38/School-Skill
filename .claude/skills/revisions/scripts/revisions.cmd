@echo off
rem Lance l'app Revisions en ligne de commande (meme programme, meme base que l'interface).
rem Exemples : revisions.cmd sync   /   revisions.cmd priorites   /   revisions.cmd carte --matiere SVT
setlocal
set "CIBLE=%~dp0..\..\..\..\app\Backend\target"
set "EXE=%CIBLE%\release\revisions-backend.exe"
if not exist "%EXE%" set "EXE=%CIBLE%\debug\revisions-backend.exe"
if not exist "%EXE%" (
    echo L'app Revisions n'est pas encore compilee : lance run-dev.bat une fois. 1>&2
    exit /b 2
)
"%EXE%" %*
exit /b %ERRORLEVEL%
