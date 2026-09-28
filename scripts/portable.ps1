# Packs the portable build after `pnpm tauri build`: Plugcam.exe with the same files the installer
# ships, plus portable.txt, zipped as Plugcam_<version>_x64-portable.zip next to the installer.
# With TAURI_SIGNING_PRIVATE_KEY set it also signs the zip for the updater (.zip.sig).
#
#   .\scripts\portable.ps1
$ErrorActionPreference = 'Stop'
$root = Join-Path $PSScriptRoot '..' | Resolve-Path
$tauri = "$root\src-tauri"
$version = (Get-Content "$tauri\tauri.conf.json" -Raw | ConvertFrom-Json).version
$out = "$tauri\target\release\bundle\portable"
$stage = "$out\Plugcam"

Remove-Item $out -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force "$stage\resources\x86", "$stage\licenses" | Out-Null

Copy-Item "$tauri\target\release\plugcam.exe" "$stage\Plugcam.exe"
$res = "$tauri\resources"
foreach ($f in 'adb.exe', 'AdbWinApi.dll', 'AdbWinUsbApi.dll', 'plugcam_cam.dll', 'scrcpy-server', 'scrcpy-server.sha256') {
    Copy-Item "$res\$f" "$stage\resources\"
}
Copy-Item "$res\x86\plugcam_cam.dll" "$stage\resources\x86\"
Copy-Item "$res\licenses\*" "$stage\licenses\"
Copy-Item "$root\LICENSE" "$stage\LICENSE.txt"
Copy-Item "$root\THIRD_PARTY_NOTICES.md" "$stage\THIRD_PARTY_NOTICES.md"
@"
This is the portable Plugcam: nothing is installed, and settings are kept in the "data" folder
next to Plugcam.exe. On the first start Plugcam asks once for administrator rights to add
"Plugcam Camera" to Windows. Before deleting this folder, open Settings and choose
"Remove Plugcam Camera from Windows".

Delete this file to make Plugcam keep its settings in %APPDATA% like the installed version.
"@ | Out-File -Encoding utf8 "$stage\portable.txt"

$zip = "$out\Plugcam_${version}_x64-portable.zip"
# The files go at the root of the zip: "Extract all" already makes a folder named after it.
Add-Type -AssemblyName System.IO.Compression.FileSystem
[IO.Compression.ZipFile]::CreateFromDirectory($stage, $zip, [IO.Compression.CompressionLevel]::Optimal, $false)
Remove-Item $stage -Recurse -Force

if ($env:TAURI_SIGNING_PRIVATE_KEY -or $env:TAURI_SIGNING_PRIVATE_KEY_PATH) {
    Push-Location $root
    try { pnpm tauri signer sign --app-version $version $zip | Out-Host } finally { Pop-Location }
    if (-not (Test-Path "$zip.sig")) { throw "signing failed: $zip.sig is missing" }
}
Write-Host "portable: $zip ($([math]::Round((Get-Item $zip).Length / 1MB, 1)) MB)"
