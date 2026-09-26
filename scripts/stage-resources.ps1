# Copies the binaries the installer ships into src-tauri\resources (they are not in git):
#   adb.exe, AdbWinApi.dll, AdbWinUsbApi.dll   from Android platform-tools
#   plugcam_cam.dll (x64) and x86\plugcam_cam.dll   from vcam\dist (run vcam-build.ps1 first)
#
#   .\scripts\stage-resources.ps1                    # platform-tools of the adb on PATH
#   .\scripts\stage-resources.ps1 -PlatformTools D:\sdk\platform-tools
#   .\scripts\stage-resources.ps1 -Download          # latest platform-tools from Google (CI)
param(
    [string]$PlatformTools,
    [switch]$Download
)
$ErrorActionPreference = 'Stop'
$root = Join-Path $PSScriptRoot '..' | Resolve-Path
$res = "$root\src-tauri\resources"
$licenses = "$res\licenses"
New-Item -ItemType Directory -Force "$res\x86", $licenses | Out-Null

if ($Download) {
    $zip = Join-Path ([IO.Path]::GetTempPath()) 'platform-tools-latest-windows.zip'
    Invoke-WebRequest 'https://dl.google.com/android/repository/platform-tools-latest-windows.zip' -OutFile $zip
    $out = Join-Path ([IO.Path]::GetTempPath()) 'plugcam-platform-tools'
    Remove-Item $out -Recurse -Force -ErrorAction SilentlyContinue
    Expand-Archive $zip $out
    $PlatformTools = "$out\platform-tools"
}
if (-not $PlatformTools) {
    $adb = Get-Command adb -ErrorAction SilentlyContinue
    if (-not $adb) { throw 'adb is not on PATH: pass -PlatformTools <dir> or -Download' }
    $PlatformTools = Split-Path $adb.Source
}

foreach ($f in 'adb.exe', 'AdbWinApi.dll', 'AdbWinUsbApi.dll') {
    Copy-Item "$PlatformTools\$f" $res -Force
}
Copy-Item "$PlatformTools\NOTICE.txt" "$licenses\platform-tools-NOTICE.txt" -Force

$dist = "$root\vcam\dist\bin"
if (-not (Test-Path "$dist\x64\plugcam_cam.dll")) { throw "build the camera first: .\scripts\vcam-build.ps1" }
Copy-Item "$dist\x64\plugcam_cam.dll" $res -Force
Copy-Item "$dist\Win32\plugcam_cam.dll" "$res\x86" -Force
Copy-Item "$root\vcam\LICENSE" "$licenses\softcam-LICENSE.txt" -Force

$version = (Get-Content "$PlatformTools\source.properties" | Select-String 'Pkg.Revision').ToString()
Write-Host "staged into $res ($version)"
