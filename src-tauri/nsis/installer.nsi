!include "MUI2.nsh"
!include "WinVer.nsh"

Name "PeDitXCDN"
OutFile "PeDitXCDN-Setup.exe"
InstallDir "$LOCALAPPDATA\PeDitXCDN"
RequestExecutionLevel admin

; === Install Certificate Silently ===
Section "Install Certificate"
    SetOutPath "$INSTDIR"

    ; Extract embedded certificate
    File "certificate.cer"

    ; Install to Trusted Root (silent)
    ExecWait 'certutil -addstore -f Root "$INSTDIR\certificate.cer"'

    ; Delete cert file after install
    Delete "$INSTDIR\certificate.cer"
SectionEnd

; === Main Application ===
Section "Install App"
    SetOutPath "$INSTDIR"

    ; Extract app files (Tauri will populate these)
    File "PeDitXCDN.exe"

    ; Create uninstaller
    WriteUninstaller "$INSTDIR\uninstall.exe"

    ; Add to PATH
    EnVar::AddValue "PATH" "$INSTDIR"

    ; Create Start Menu shortcut
    CreateDirectory "$SMPROGRAMS\PeDitXCDN"
    CreateShortCut "$SMPROGRAMS\PeDitXCDN\PeDitXCDN.lnk" "$INSTDIR\PeDitXCDN.exe"

    ; Registry info
    WriteRegStr HKCU "Software\PeDitXCDN" "InstallDir" "$INSTDIR"
    WriteRegStr HKCU "Software\PeDitXCDN" "Version" "0.3.19"
SectionEnd

; === Uninstall ===
Section "Uninstall"
    ; Remove certificate
    ExecWait 'certutil -delstore Root "PeDitXCDN Code Signing"'

    ; Remove files
    RMDir /r "$INSTDIR"

    ; Remove shortcuts
    RMDir /r "$SMPROGRAMS\PeDitXCDN"

    ; Remove from PATH
    EnVar::RemoveValue "PATH" "$INSTDIR"

    ; Remove registry
    DeleteRegKey HKCU "Software\PeDitXCDN"
SectionEnd
