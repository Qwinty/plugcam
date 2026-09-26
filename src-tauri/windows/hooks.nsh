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

!macro NSIS_HOOK_POSTINSTALL
  !insertmacro PLUGCAM_REGSVR32 "/s"
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  !insertmacro PLUGCAM_REGSVR32 "/u /s"
!macroend
