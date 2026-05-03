; Maolan DAW Installer
; Run with: makensis.exe installer.nsi

!include "MUI2.nsh"
!include "LogicLib.nsh"

;--------------------------------
; General
;--------------------------------
Name "Maolan"
OutFile "maolan-setup.exe"
InstallDir "$LOCALAPPDATA\Maolan"
InstallDirRegKey HKCU "Software\Maolan" "InstallDir"
RequestExecutionLevel user

;--------------------------------
; Version Info
;--------------------------------
VIProductVersion "0.0.8.0"
VIAddVersionKey "ProductName" "Maolan"
VIAddVersionKey "ProductVersion" "0.0.8"
VIAddVersionKey "FileVersion" "0.0.8"
VIAddVersionKey "FileDescription" "Maolan Digital Audio Workstation"
VIAddVersionKey "LegalCopyright" "BSD-2-Clause"

;--------------------------------
; Interface Settings
;--------------------------------
!define MUI_ABORTWARNING
!define MUI_ICON "${NSISDIR}\Contrib\Graphics\Icons\modern-install.ico"
!define MUI_UNICON "${NSISDIR}\Contrib\Graphics\Icons\modern-uninstall.ico"

;--------------------------------
; Pages
;--------------------------------
!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_LICENSE "LICENSE"
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH

!insertmacro MUI_UNPAGE_WELCOME
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_UNPAGE_FINISH

;--------------------------------
; Languages
;--------------------------------
!insertmacro MUI_LANGUAGE "English"

;--------------------------------
; Installer Sections
;--------------------------------
Section "Install"
    SetOutPath "$INSTDIR"

    ; Main binaries
    File "C:\cargo-target\x86_64-pc-windows-msvc\release\maolan.exe"
    File "C:\cargo-target\x86_64-pc-windows-msvc\release\maolan-cli.exe"

    ; FFmpeg DLLs from vcpkg
    File "C:\vcpkg\installed\x64-windows\bin\avcodec-62.dll"
    File "C:\vcpkg\installed\x64-windows\bin\avdevice-62.dll"
    File "C:\vcpkg\installed\x64-windows\bin\avfilter-11.dll"
    File "C:\vcpkg\installed\x64-windows\bin\avformat-62.dll"
    File "C:\vcpkg\installed\x64-windows\bin\avutil-60.dll"
    File "C:\vcpkg\installed\x64-windows\bin\swresample-6.dll"

    ; VC++ Redistributable installer (bundled)
    File "..\vc_redist.x64.exe"
    ExecWait '"$INSTDIR\vc_redist.x64.exe" /install /quiet /norestart' $0
    Delete "$INSTDIR\vc_redist.x64.exe"

    ; Store installation folder
    WriteRegStr HKCU "Software\Maolan" "InstallDir" $INSTDIR

    ; Create uninstaller
    WriteUninstaller "$INSTDIR\Uninstall.exe"

    ; Add to Add/Remove Programs
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Maolan" \
        "DisplayName" "Maolan"
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Maolan" \
        "UninstallString" "$\"$INSTDIR\Uninstall.exe$\""
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Maolan" \
        "DisplayVersion" "0.0.8"
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Maolan" \
        "Publisher" "Maolan Team"
    WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Maolan" \
        "NoModify" 1
    WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Maolan" \
        "NoRepair" 1

    ; Create Start Menu shortcuts
    CreateDirectory "$SMPROGRAMS\Maolan"
    CreateShortcut "$SMPROGRAMS\Maolan\Maolan.lnk" "$INSTDIR\maolan.exe"
    CreateShortcut "$SMPROGRAMS\Maolan\Maolan CLI.lnk" "$INSTDIR\maolan-cli.exe"
    CreateShortcut "$SMPROGRAMS\Maolan\Uninstall.lnk" "$INSTDIR\Uninstall.exe"

    ; Create desktop shortcut
    CreateShortcut "$DESKTOP\Maolan.lnk" "$INSTDIR\maolan.exe"
SectionEnd

;--------------------------------
; Uninstaller Section
;--------------------------------
Section "Uninstall"
    Delete "$INSTDIR\maolan.exe"
    Delete "$INSTDIR\maolan-cli.exe"

    Delete "$INSTDIR\avcodec-62.dll"
    Delete "$INSTDIR\avdevice-62.dll"
    Delete "$INSTDIR\avfilter-11.dll"
    Delete "$INSTDIR\avformat-62.dll"
    Delete "$INSTDIR\avutil-60.dll"
    Delete "$INSTDIR\swresample-6.dll"

    ; VC++ Redistributable is uninstalled separately via Windows Add/Remove Programs

    Delete "$INSTDIR\Uninstall.exe"

    Delete "$SMPROGRAMS\Maolan\Maolan.lnk"
    Delete "$SMPROGRAMS\Maolan\Maolan CLI.lnk"
    Delete "$SMPROGRAMS\Maolan\Uninstall.lnk"
    RMDir "$SMPROGRAMS\Maolan"

    Delete "$DESKTOP\Maolan.lnk"

    DeleteRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Maolan"
    DeleteRegKey HKCU "Software\Maolan"

    RMDir "$INSTDIR"
SectionEnd
