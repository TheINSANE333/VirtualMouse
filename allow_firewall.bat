@echo off
:: BatchGotAdmin
:-------------------------------------
REM --> Check for permissions
IF '%PROCESSOR_ARCHITECTURE%' EQU 'amd64' (
   >nul 2>&1 "%SYSTEMROOT%\SysWOW64\cacls.exe" "%SYSTEMROOT%\SysWOW64\config\system"
) ELSE (
   >nul 2>&1 "%SYSTEMROOT%\system32\cacls.exe" "%SYSTEMROOT%\system32\config\system"
)

REM --> If error flag set, we do not have admin.
if '%errorlevel%' NEQ '0' (
    echo [!] Requesting administrative privileges...
    goto UACPrompt
) else ( goto gotAdmin )

:UACPrompt
    echo Set UAC = CreateObject^("Shell.Application"^) > "%temp%\getadmin.vbs"
    set params = %*:"=""
    echo UAC.ShellExecute "cmd.exe", "/c ""%~s0"" %params%", "", "runas", 1 >> "%temp%\getadmin.vbs"

    "%temp%\getadmin.vbs"
    del "%temp%\getadmin.vbs"
    exit /B

:gotAdmin
    pushd "%CD%"
    CD /D "%~dp0"
:--------------------------------------

echo ==================================================================
echo   Configuring Windows Firewall for VirtualMouse (Ethernet & Wi-Fi)
echo ==================================================================
echo.
echo [*] Adding Inbound TCP Port 3240 (All Network Profiles)...
netsh advfirewall firewall delete rule name="VirtualMouse USBIP (Port 3240)" >nul 2>&1
netsh advfirewall firewall add rule name="VirtualMouse USBIP (Port 3240)" dir=in action=allow protocol=TCP localport=3240 profile=any >nul

echo [*] Adding Inbound UDP Port 3240 (Auto-Discovery)...
netsh advfirewall firewall delete rule name="VirtualMouse UDP Discovery (Port 3240)" >nul 2>&1
netsh advfirewall firewall add rule name="VirtualMouse UDP Discovery (Port 3240)" dir=in action=allow protocol=UDP localport=3240 profile=any >nul

echo [*] Adding VirtualMouseServer application exception...
netsh advfirewall firewall delete rule name="VirtualMouseServer" >nul 2>&1
netsh advfirewall firewall add rule name="VirtualMouseServer" dir=in action=allow program="%~dp0VirtualMouseServer.exe" profile=any >nul

echo.
echo ==================================================================
echo   [+] SUCCESS! Windows Firewall rules configured successfully!
echo   Both Ethernet and Wi-Fi connections are now fully permitted.
echo ==================================================================
echo.
pause
