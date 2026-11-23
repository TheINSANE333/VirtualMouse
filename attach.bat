@echo off
set /p SENDER_IP="Enter Sender PC IP address (default 127.0.0.1): "
if "%SENDER_IP%"=="" set SENDER_IP=127.0.0.1
python client.py %SENDER_IP%
pause
