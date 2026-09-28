# Writes latest.json, the updater manifest attached to every release: the version, its notes and,
# per target, the download URL and signature. The app reads it from
# https://github.com/Qwinty/plugcam/releases/latest/download/latest.json
#   windows-x86_64            the installer (installed copies)
#   windows-x86_64-portable   the portable zip (see portable.ps1)
#
#   .\scripts\update-manifest.ps1 -Tag v0.2.0 [-BaseUrl http://127.0.0.1:8000] [-Out latest.json]
param(
    [Parameter(Mandatory)][string]$Tag,
    [string]$BaseUrl = "https://github.com/Qwinty/plugcam/releases/download/$Tag",
    [string]$Out
)
$ErrorActionPreference = 'Stop'
$root = Join-Path $PSScriptRoot '..' | Resolve-Path
$bundle = "$root\src-tauri\target\release\bundle"
if (-not $Out) { $Out = "$bundle\latest.json" }

function Entry($file) {
    if (-not (Test-Path "$file.sig")) { throw "$file.sig is missing: build with TAURI_SIGNING_PRIVATE_KEY set" }
    @{ url = "$BaseUrl/$([uri]::EscapeDataString((Split-Path $file -Leaf)))"; signature = (Get-Content "$file.sig" -Raw).Trim() }
}

$installer = Get-ChildItem "$bundle\nsis\*-setup.exe" | Select-Object -First 1
$portable = Get-ChildItem "$bundle\portable\*-portable.zip" | Select-Object -First 1
if (-not $installer -or -not $portable) { throw 'build the installer and the portable zip first' }

$notesFile = "$root\docs\releases\$Tag.md"
$manifest = [ordered]@{
    version   = $Tag.TrimStart('v')
    notes     = if (Test-Path $notesFile) { Get-Content $notesFile -Raw -Encoding utf8 } else { '' }
    pub_date  = (Get-Date).ToUniversalTime().ToString("yyyy-MM-ddTHH:mm:ssZ")
    platforms = [ordered]@{
        'windows-x86_64'          = Entry $installer.FullName
        'windows-x86_64-portable' = Entry $portable.FullName
    }
}
$json = $manifest | ConvertTo-Json -Depth 5
[IO.File]::WriteAllText($Out, $json, [Text.UTF8Encoding]::new($false))
Write-Host "manifest: $Out"
