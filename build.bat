@echo off
setlocal
echo ==================================================================
echo   Building VirtualMouse Native Rust Suite (Server & Host)
echo ==================================================================

set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"
cargo build --release
if %ERRORLEVEL% equ 0 (
    copy /Y target\release\VirtualMouseServer.exe VirtualMouseServer.exe >nul
    copy /Y target\release\VirtualMouseHost.exe VirtualMouseHost.exe >nul
    if not exist dist mkdir dist
    copy /Y target\release\VirtualMouseServer.exe dist\VirtualMouseServer.exe >nul
    copy /Y target\release\VirtualMouseHost.exe dist\VirtualMouseHost.exe >nul
    echo.
    echo [+] Build successful! Executables created:
    echo     - VirtualMouseServer.exe (Run on Server PC)
    echo     - VirtualMouseHost.exe   (Run on Host PC)
    echo.
) else (
    echo.
    echo [!] Build failed.
)
pause
