@echo off
echo Building VirtualMouseServer.exe...
pyinstaller --onefile --name VirtualMouseServer server.py
echo.
echo Build finished! The executable is located in: dist\VirtualMouseServer.exe
pause
