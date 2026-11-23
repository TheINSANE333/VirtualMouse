@echo off
echo Starting VirtualMouse Server...
echo Choose Mode:
echo [1] Interactive Capture Window (Default)
echo [2] Test Mode (Auto circle motion)
echo [3] Headless Mode (Background server)
set /p MODE="Select mode [1/2/3]: "

if "%MODE%"=="2" (
    python server.py --test
) else if "%MODE%"=="3" (
    python server.py --headless
) else (
    python server.py
)
pause
