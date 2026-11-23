@echo off
setlocal
cd /d "%~dp0"
set PATH=%USERPROFILE%\.cargo\bin;%PATH%

if exist "VirtualMouseServer.exe" (
    VirtualMouseServer.exe %*
) else if exist "target\release\virtual_mouse_server.exe" (
    target\release\virtual_mouse_server.exe %*
) else (
    echo [!] Server executable not found. Building now...
    cargo build --release
    if exist "target\release\virtual_mouse_server.exe" (
        copy target\release\virtual_mouse_server.exe VirtualMouseServer.exe /Y
        VirtualMouseServer.exe %*
    ) else (
        echo [!] Build failed.
        pause
    )
)
