# Builds plugcam_cam.dll (x64 and Win32) into vcam\dist with the newest installed MSVC toolset.
# If the DLL is loaded by some app (browsers enumerate cameras), the old file is renamed to
# *.old so the new one can take its place.
$ErrorActionPreference = 'Stop'
$vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
$vs = & $vswhere -latest -products * -requires Microsoft.Component.MSBuild -property installationPath
$msbuild = "$vs\MSBuild\Current\Bin\MSBuild.exe"
$vcam = Join-Path $PSScriptRoot '..\vcam' | Resolve-Path

# The project pins v143 (VS 2022, as on CI). Use whatever toolset this machine has instead.
$toolset = (Get-ChildItem "$vs\MSBuild\Microsoft\VC\*\Platforms\x64\PlatformToolsets\*" -Directory |
    Where-Object Name -match '^v\d+$' | Sort-Object Name | Select-Object -Last 1).Name
if (-not $toolset) { throw "no MSVC platform toolset found under $vs" }

foreach ($platform in 'x64', 'Win32') {
    $dll = "$vcam\dist\bin\$platform\plugcam_cam.dll"
    if (Test-Path $dll) {
        try { [IO.File]::Open($dll, 'Open', 'ReadWrite', 'None').Close() }
        catch { Remove-Item "$dll.old" -ErrorAction SilentlyContinue; Move-Item $dll "$dll.old" }
    }
    & $msbuild "$vcam\softcam.sln" /t:softcam /p:Configuration=Release /p:Platform=$platform /p:PlatformToolset=$toolset /m /v:minimal /nologo
    if ($LASTEXITCODE) { exit $LASTEXITCODE }
}
