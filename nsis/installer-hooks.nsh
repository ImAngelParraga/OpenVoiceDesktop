!macro StopOpenVoiceDesktop
  nsExec::ExecToLog '"$INSTDIR\openvoice-desktop.exe" shutdown'
  Sleep 1000
  nsExec::ExecToLog 'taskkill.exe /IM openvoice-desktop.exe /T /F'
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
  nsExec::ExecToLog 'schtasks.exe /Delete /TN OpenVoiceDesktop /F'
  !insertmacro RemoveLegacyStartupShortcut
!macroend
