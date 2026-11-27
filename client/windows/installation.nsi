; SentinAnt Lite för Windows: installationsfilen (NSIS). Byggs av scripts/bygg_windows.sh:
;   makensis -INPUTCHARSET UTF8 -DVERSION=… -DVERSION4=… -DPROGRAM=…/sentinant-lite.exe -DUTFIL=… \
;            -DIKON=…/ikon.ico -DTREDJEPART=…/TREDJEPART.md installation.nsi
;
; Samma installation som SentinAnts (autorobot, client/windows/installation.nsi), med
; SentinAnt Lites namn. Installerar per användare, utan administratörsrätt, i
; %LOCALAPPDATA%\Programs\SentinAnt Lite. Där får användaren skriva, så programmets egen
; uppdatering (knappen på startskärmen) kan byta programfilen utan UAC-fråga.
; Inställningar och sparade robotar ligger i %APPDATA%\robot-control\robotstyrning och
; rörs varken av installationen eller avinstallationen. SentinAnt och SentinAnt Lite kan
; vara installerade samtidigt: egna mappar, egna genvägar, egna poster.

Unicode true
SetCompressor /SOLID lzma
RequestExecutionLevel user

!include "MUI2.nsh"
!include "FileFunc.nsh"

!define NAMN "SentinAnt Lite"
!define AVINST_NYCKEL "Software\Microsoft\Windows\CurrentVersion\Uninstall\SentinAnt Lite"

Name "${NAMN}"
OutFile "${UTFIL}"
InstallDir "$LOCALAPPDATA\Programs\SentinAnt Lite"
BrandingText "Mapro Systems AB"
ShowInstDetails nevershow
ShowUninstDetails nevershow

VIProductVersion "${VERSION4}"
VIAddVersionKey "ProductName" "${NAMN}"
VIAddVersionKey "CompanyName" "Mapro Systems AB"
VIAddVersionKey "LegalCopyright" "Mapro Systems AB"
VIAddVersionKey "FileDescription" "SentinAnt Lite installation"
VIAddVersionKey "FileVersion" "${VERSION}"
VIAddVersionKey "ProductVersion" "${VERSION}"

!define MUI_ICON "${IKON}"
!define MUI_UNICON "${IKON}"
!define MUI_ABORTWARNING
!define MUI_WELCOMEPAGE_TEXT "SentinAnt Lite ${VERSION} installeras för den här användaren. Ingen administratörsrätt behövs.$\r$\n$\r$\nProgrammet når roboten genom WireGuard-tunneln. WireGuard installeras för sig och ska vara igång när du ansluter.$\r$\n$\r$\nTryck Installera."
!define MUI_FINISHPAGE_RUN "$INSTDIR\sentinant-lite.exe"
!define MUI_FINISHPAGE_RUN_TEXT "Starta SentinAnt Lite"
!define MUI_FINISHPAGE_TEXT "SentinAnt Lite är installerat, med genväg på skrivbordet och i startmenyn.$\r$\n$\r$\nNya versioner installeras från programmet: knappen längst ned på startskärmen blir grön när det finns en uppdatering."

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "Swedish"

Section "SentinAnt Lite"
  SetOutPath "$INSTDIR"
  ; Kör programmet redan (ominstallation): en programfil som kör går inte att skriva över,
  ; men den går att byta namn på. Programmet städar själv bort den gamla vid nästa start.
  Delete "$INSTDIR\sentinant-lite-gammal-installation.exe"
  IfFileExists "$INSTDIR\sentinant-lite.exe" 0 +2
    Rename "$INSTDIR\sentinant-lite.exe" "$INSTDIR\sentinant-lite-gammal-installation.exe"
  File "/oname=sentinant-lite.exe" "${PROGRAM}"
  File "/oname=TREDJEPART.txt" "${TREDJEPART}"
  WriteUninstaller "$INSTDIR\avinstallera.exe"

  CreateShortcut "$SMPROGRAMS\SentinAnt Lite.lnk" "$INSTDIR\sentinant-lite.exe"
  CreateShortcut "$DESKTOP\SentinAnt Lite.lnk" "$INSTDIR\sentinant-lite.exe"

  ; Posten under Installerade appar (per användare).
  WriteRegStr HKCU "${AVINST_NYCKEL}" "DisplayName" "${NAMN}"
  WriteRegStr HKCU "${AVINST_NYCKEL}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKCU "${AVINST_NYCKEL}" "Publisher" "Mapro Systems AB"
  WriteRegStr HKCU "${AVINST_NYCKEL}" "DisplayIcon" "$INSTDIR\sentinant-lite.exe"
  WriteRegStr HKCU "${AVINST_NYCKEL}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "${AVINST_NYCKEL}" "UninstallString" '"$INSTDIR\avinstallera.exe"'
  WriteRegStr HKCU "${AVINST_NYCKEL}" "QuietUninstallString" '"$INSTDIR\avinstallera.exe" /S'
  WriteRegDWORD HKCU "${AVINST_NYCKEL}" "NoModify" 1
  WriteRegDWORD HKCU "${AVINST_NYCKEL}" "NoRepair" 1
  ${GetSize} "$INSTDIR" "/S=0K" $0 $1 $2
  IntFmt $0 "0x%08X" $0
  WriteRegDWORD HKCU "${AVINST_NYCKEL}" "EstimatedSize" "$0"
SectionEnd

Section "Uninstall"
  ; Programmet måste vara stängt: annars går programfilen inte att ta bort.
  ClearErrors
  Delete "$INSTDIR\sentinant-lite.exe"
  IfErrors 0 +3
    MessageBox MB_OK|MB_ICONEXCLAMATION "Stäng SentinAnt Lite först, och avinstallera sedan igen." /SD IDOK
    Abort
  ; Rester efter programmets egna uppdateringar.
  Delete "$INSTDIR\sentinant-lite-ny*"
  Delete "$INSTDIR\sentinant-lite-gammal*"
  Delete "$INSTDIR\TREDJEPART.txt"
  Delete "$INSTDIR\avinstallera.exe"
  RMDir "$INSTDIR"
  Delete "$SMPROGRAMS\SentinAnt Lite.lnk"
  Delete "$DESKTOP\SentinAnt Lite.lnk"
  DeleteRegKey HKCU "${AVINST_NYCKEL}"
SectionEnd
