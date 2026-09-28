; Registers "Plugcam Camera" (the DirectShow filter in plugcam_cam.dll) after install and
; removes it before uninstall: the x64 DLL for 64-bit apps, the x86 one for 32-bit apps.
; The installer is a 32-bit process, so 64-bit regsvr32 is reached through Sysnative.

!macro PLUGCAM_REGSVR32 FLAGS
  ${If} ${FileExists} "$WINDIR\Sysnative\regsvr32.exe"
    StrCpy $R9 "$WINDIR\Sysnative\regsvr32.exe"
  ${Else}
    StrCpy $R9 "$WINDIR\System32\regsvr32.exe"
  ${EndIf}
  ExecWait '"$R9" ${FLAGS} "$INSTDIR\resources\plugcam_cam.dll"'
  ${If} ${FileExists} "$WINDIR\SysWOW64\regsvr32.exe"
    ExecWait '"$WINDIR\SysWOW64\regsvr32.exe" ${FLAGS} "$INSTDIR\resources\x86\plugcam_cam.dll"'
  ${EndIf}
!macroend

; A video app that has the camera open keeps plugcam_cam.dll loaded, so it cannot be
; overwritten, but it can be renamed: move the old one aside and delete it now or after a reboot.
!macro PLUGCAM_MOVE_ASIDE DLL
  ${If} ${FileExists} "${DLL}"
    Delete "${DLL}.old"
    Rename "${DLL}" "${DLL}.old"
  ${EndIf}
!macroend

!macro PLUGCAM_DELETE_OLD
  Delete /REBOOTOK "$INSTDIR\resources\plugcam_cam.dll.old"
  Delete /REBOOTOK "$INSTDIR\resources\x86\plugcam_cam.dll.old"
!macroend

!macro NSIS_HOOK_PREINSTALL
  !insertmacro PLUGCAM_MOVE_ASIDE "$INSTDIR\resources\plugcam_cam.dll"
  !insertmacro PLUGCAM_MOVE_ASIDE "$INSTDIR\resources\x86\plugcam_cam.dll"
!macroend

!macro NSIS_HOOK_POSTINSTALL
  !insertmacro PLUGCAM_REGSVR32 "/s"
  !insertmacro PLUGCAM_DELETE_OLD
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  !insertmacro PLUGCAM_REGSVR32 "/u /s"
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  !insertmacro PLUGCAM_DELETE_OLD
!macroend
