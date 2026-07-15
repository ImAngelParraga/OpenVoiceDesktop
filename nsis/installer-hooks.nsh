!macro StopOpenVoiceDesktop
  ; Never let a damaged or busy daemon hold the installer forever.
  nsExec::ExecToLog /TIMEOUT=3000 '"$INSTDIR\openvoice-desktop.exe" shutdown'
  Pop $0
  Sleep 500
  nsExec::ExecToLog /TIMEOUT=5000 'taskkill.exe /IM openvoice-desktop.exe /T /F'
  Pop $0
!macroend

!macro RemoveLegacyStartupShortcut
  Delete "$APPDATA\Microsoft\Windows\Start Menu\Programs\Startup\OpenVoiceDesktop.lnk"
!macroend

!macro NSIS_HOOK_PREINSTALL
  !insertmacro StopOpenVoiceDesktop
  !insertmacro RemoveLegacyStartupShortcut
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  !insertmacro StopOpenVoiceDesktop
  nsExec::ExecToLog /TIMEOUT=5000 'schtasks.exe /Delete /TN OpenVoiceDesktop /F'
  Pop $0
  !insertmacro RemoveLegacyStartupShortcut
!macroend
