!macro NSIS_HOOK_PREINSTALL
  nsExec::ExecToStack '"$SYSDIR\sc.exe" stop ZapretDesktop'
  Pop $0
  Pop $1
  Sleep 1500
!macroend

!macro NSIS_HOOK_POSTINSTALL
  SetShellVarContext all
  CreateDirectory "$APPDATA\ZapretDesktop"
  nsExec::ExecToStack '"$SYSDIR\icacls.exe" "$APPDATA\ZapretDesktop" /inheritance:r /grant:r "*S-1-5-18:(OI)(CI)F" "*S-1-5-32-544:(OI)(CI)F"'
  Pop $0
  Pop $1
  ${If} $0 != 0
    MessageBox MB_ICONSTOP "Не удалось защитить каталог службы. Установка прервана."
    Abort
  ${EndIf}
  nsExec::ExecToStack '"$SYSDIR\sc.exe" delete ZapretDesktop'
  Pop $0
  Pop $1
  nsExec::ExecToStack '"$SYSDIR\sc.exe" create ZapretDesktop binPath= "\"$INSTDIR\runtime\zapretd.exe\"" start= auto DisplayName= "Zapret Desktop Service"'
  Pop $0
  Pop $1
  ${If} $0 != 0
    MessageBox MB_ICONSTOP "Не удалось зарегистрировать системную службу. $1"
    Abort
  ${EndIf}
  nsExec::ExecToStack '"$SYSDIR\sc.exe" description ZapretDesktop "Управление локальным обходом DPI. Приложение работает без прав администратора."'
  Pop $0
  Pop $1
  nsExec::ExecToStack '"$SYSDIR\sc.exe" failure ZapretDesktop reset= 86400 actions= restart/5000/restart/15000/none/0'
  Pop $0
  Pop $1
  nsExec::ExecToStack '"$SYSDIR\sc.exe" start ZapretDesktop'
  Pop $0
  Pop $1
  ${If} $0 != 0
    MessageBox MB_ICONSTOP "Служба установлена, но не запущена. $1"
    Abort
  ${EndIf}
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  nsExec::ExecToStack '"$SYSDIR\sc.exe" stop ZapretDesktop'
  Pop $0
  Pop $1
  Sleep 1500
  nsExec::ExecToStack '"$SYSDIR\sc.exe" delete ZapretDesktop'
  Pop $0
  Pop $1
!macroend
