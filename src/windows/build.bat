cl.exe /LD ble_server.cpp /Fe:ble_server.dll
if %errorlevel% neq 0 (
    echo Compilation failed.
    exit /b %errorlevel%
)
echo Compilation succeeded.