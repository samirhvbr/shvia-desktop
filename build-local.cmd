@echo off
REM Lancador do build-local.ps1 — pode dar DUPLO-CLIQUE ou rodar pelo cmd/Explorer.
REM Contorna duas pegadinhas do Windows: a associacao .ps1->Notepad e a ExecutionPolicy.
REM Passa argumentos adiante:  build-local.cmd -SkipNpmCi
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0build-local.ps1" %*
echo.
pause
