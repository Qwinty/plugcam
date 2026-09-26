# Registers (or with -Unregister, removes) "Plugcam Camera" from vcam\dist for development.
# Needs admin rights: the script re-launches itself elevated (UAC prompt).
param([switch]$Unregister)

$ErrorActionPreference = 'Stop'
$bin = Join-Path $PSScriptRoot '..\vcam\dist\bin' | Resolve-Path
$targets = @(
    @{ Exe = "$env:WINDIR\System32\regsvr32.exe"; Dll = "$bin\x64\plugcam_cam.dll" },
    @{ Exe = "$env:WINDIR\SysWOW64\regsvr32.exe"; Dll = "$bin\Win32\plugcam_cam.dll" }
)

$admin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole(
    [Security.Principal.WindowsBuiltInRole]::Administrator)
if (-not $admin) {
    $argList = @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', "`"$PSCommandPath`"")
    if ($Unregister) { $argList += '-Unregister' }
    $p = Start-Process powershell -Verb RunAs -ArgumentList $argList -Wait -PassThru -WindowStyle Hidden
    exit $p.ExitCode
}

$failed = 0
foreach ($t in $targets) {
    if (-not (Test-Path $t.Dll)) { Write-Warning "missing $($t.Dll), build vcam first"; $failed++; continue }
    $regArgs = @('/s') + $(if ($Unregister) { @('/u') } else { @() }) + @("`"$($t.Dll)`"")
    $p = Start-Process $t.Exe -ArgumentList $regArgs -Wait -PassThru
    if ($p.ExitCode -ne 0) { $failed++ }
}
exit $failed
