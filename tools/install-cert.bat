@echo off
echo ================================================
echo   PeDitXCDN - Install Security Certificate
echo ================================================
echo.
echo This will install the PeDitXCDN certificate
echo to your Trusted Root store so Windows will
echo trust the application.
echo.
echo Administrator privileges required!
echo.

net session >nul 2>&1
if %errorLevel% neq 0 (
    echo ERROR: Run this as Administrator!
    echo Right-click - Run as administrator
    pause
    exit /b 1
)

set CERT_PATH=%~dp0certificate.cer

if not exist "%CERT_PATH%" (
    echo ERROR: certificate.cer not found!
    echo Expected at: %CERT_PATH%
    pause
    exit /b 1
)

echo Installing certificate...
certutil -addstore -f "Root" "%CERT_PATH%"

if %errorLevel% equ 0 (
    echo.
    echo SUCCESS! Certificate installed.
    echo PeDitXCDN should now open without warnings.
) else (
    echo.
    echo FAILED to install certificate.
)

echo.
pause
