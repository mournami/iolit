!macro NSIS_HOOK_PREINSTALL
  DetailPrint "Остановка службы Zapret Desktop…"
  nsExec::ExecToStack '"$SYSDIR\sc.exe" stop ZapretDesktop'
  Pop $0
  Pop $1
  ; Wait only when a running service was asked to stop; fresh installs skip it.
  ${If} $0 == 0
    Sleep 1500
  ${EndIf}
!macroend

!macro NSIS_HOOK_POSTINSTALL
  SetShellVarContext all
  CreateDirectory "$APPDATA\ZapretDesktop"
  ; nsExec prints nothing, so name each step: otherwise the page shows the
  ; folder creation line while the service is registered and started.
  DetailPrint "Защита каталога службы…"
  nsExec::ExecToStack '"$SYSDIR\icacls.exe" "$APPDATA\ZapretDesktop" /inheritance:r /grant:r "*S-1-5-18:(OI)(CI)F" "*S-1-5-32-544:(OI)(CI)F"'
  Pop $0
  Pop $1
  ${If} $0 != 0
    MessageBox MB_ICONSTOP "Не удалось защитить каталог службы. Установка прервана."
    Abort
  ${EndIf}
  DetailPrint "Регистрация системной службы…"
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
  DetailPrint "Запуск службы (первый запуск может занять до минуты)…"
  nsExec::ExecToStack '"$SYSDIR\sc.exe" start ZapretDesktop'
  Pop $0
  Pop $1
  ${If} $0 != 0
    MessageBox MB_ICONSTOP "Служба установлена, но не запущена. $1"
    Abort
  ${EndIf}
  DetailPrint "Служба запущена."
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  DetailPrint "Остановка службы Zapret Desktop…"
  nsExec::ExecToStack '"$SYSDIR\sc.exe" stop ZapretDesktop'
  Pop $0
  Pop $1
  ${If} $0 == 0
    Sleep 1500
  ${EndIf}
  nsExec::ExecToStack '"$SYSDIR\sc.exe" delete ZapretDesktop'
  Pop $0
  Pop $1
!macroend
