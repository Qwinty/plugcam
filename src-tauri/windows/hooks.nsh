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
!macro PLUGCAM_MOVE_ASIDE FILE
  ${If} ${FileExists} "${FILE}"
    Delete "${FILE}.old"
    Rename "${FILE}" "${FILE}.old"
  ${EndIf}
!macroend

; The adb server Plugcam started keeps running after the app exits and holds adb.exe and its
; DLLs open. Ask it to exit; if it does not, its files are moved aside like the camera DLL.
!macro PLUGCAM_STOP_ADB
  ${If} ${FileExists} "$INSTDIR\resources\adb.exe"
    nsExec::Exec /TIMEOUT=5000 '"$INSTDIR\resources\adb.exe" kill-server'
    Pop $R9
  ${EndIf}
  !insertmacro PLUGCAM_MOVE_ASIDE "$INSTDIR\resources\adb.exe"
  !insertmacro PLUGCAM_MOVE_ASIDE "$INSTDIR\resources\AdbWinApi.dll"
  !insertmacro PLUGCAM_MOVE_ASIDE "$INSTDIR\resources\AdbWinUsbApi.dll"
!macroend

!macro PLUGCAM_DELETE_OLD
  Delete /REBOOTOK "$INSTDIR\resources\plugcam_cam.dll.old"
  Delete /REBOOTOK "$INSTDIR\resources\x86\plugcam_cam.dll.old"
  Delete /REBOOTOK "$INSTDIR\resources\adb.exe.old"
  Delete /REBOOTOK "$INSTDIR\resources\AdbWinApi.dll.old"
  Delete /REBOOTOK "$INSTDIR\resources\AdbWinUsbApi.dll.old"
!macroend

!macro NSIS_HOOK_PREINSTALL
  !insertmacro PLUGCAM_STOP_ADB
  !insertmacro PLUGCAM_MOVE_ASIDE "$INSTDIR\resources\plugcam_cam.dll"
  !insertmacro PLUGCAM_MOVE_ASIDE "$INSTDIR\resources\x86\plugcam_cam.dll"
!macroend

!macro NSIS_HOOK_POSTINSTALL
  !insertmacro PLUGCAM_REGSVR32 "/s"
  !insertmacro PLUGCAM_DELETE_OLD
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  !insertmacro PLUGCAM_REGSVR32 "/u /s"
  !insertmacro PLUGCAM_STOP_ADB
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  !insertmacro PLUGCAM_DELETE_OLD
!macroend
