Unicode true
ManifestDPIAware true
RequestExecutionLevel user
SetCompressor /SOLID lzma

!ifndef APP_SOURCE
  !error "APP_SOURCE define is required"
!endif
!ifndef OUT_FILE
  !error "OUT_FILE define is required"
!endif

Name "LedgerKit Avalonia Spike"
OutFile "${OUT_FILE}"
InstallDir "$LOCALAPPDATA\Programs\LedgerKit Avalonia Spike"
InstallDirRegKey HKCU "Software\LedgerKit\AvaloniaSpike" "InstallLocation"

Page directory
Page instfiles
UninstPage uninstConfirm
UninstPage instfiles

Section "LedgerKit Avalonia Spike" SEC_APP
  SetShellVarContext current
  SetOutPath "$INSTDIR"
  File /r "${APP_SOURCE}\*"
  WriteUninstaller "$INSTDIR\Uninstall.exe"
  WriteRegStr HKCU "Software\LedgerKit\AvaloniaSpike" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\LedgerKitAvaloniaSpike" "DisplayName" "LedgerKit Avalonia Spike"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\LedgerKitAvaloniaSpike" "DisplayVersion" "0.1.0"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\LedgerKitAvaloniaSpike" "Publisher" "LedgerKit contributors"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\LedgerKitAvaloniaSpike" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\LedgerKitAvaloniaSpike" "UninstallString" '"$INSTDIR\Uninstall.exe"'
  WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\LedgerKitAvaloniaSpike" "NoModify" 1
  WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\LedgerKitAvaloniaSpike" "NoRepair" 1
  CreateDirectory "$SMPROGRAMS\LedgerKit"
  CreateShortcut "$SMPROGRAMS\LedgerKit\LedgerKit Avalonia Spike.lnk" "$INSTDIR\ledgerkit-avalonia-spike.exe"
SectionEnd

Section "Uninstall"
  SetShellVarContext current
  Delete "$SMPROGRAMS\LedgerKit\LedgerKit Avalonia Spike.lnk"
  RMDir "$SMPROGRAMS\LedgerKit"
  DeleteRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\LedgerKitAvaloniaSpike"
  DeleteRegKey HKCU "Software\LedgerKit\AvaloniaSpike"
  RMDir /r "$INSTDIR"
SectionEnd
