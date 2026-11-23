!macro NSIS_HOOK_POSTINSTALL
  CopyFiles /SILENT "$INSTDIR\uninstall.exe" "$INSTDIR\uninstall_easynote.exe"
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  Delete /REBOOTOK "$INSTDIR\uninstall_easynote.exe"
!macroend
