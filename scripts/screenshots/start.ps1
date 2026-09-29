# Starts a Plugcam build for screenshots and UI checks: portable, so it keeps its own settings in
# data\ next to the exe and never touches the installed app's, and with WebView2's remote
# debugging on, so cdp.mjs can drive it.
#   pwsh scripts/screenshots/start.ps1 -Exe <path to plugcam.exe or Plugcam.exe> [-Port 9222]
# Stops an earlier copy of that same exe first; any other Plugcam keeps running.
param(
    [Parameter(Mandatory)][string]$Exe,
    [int]$Port = 9222
)
$ErrorActionPreference = 'Stop'
$Exe = (Resolve-Path $Exe).Path
$dir = Split-Path $Exe

Get-Process plugcam -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq $Exe } | Stop-Process -Confirm:$false
$marker = Join-Path $dir 'portable.txt'
if (-not (Test-Path $marker)) { New-Item -ItemType File $marker | Out-Null }

# The variable replaces the flags tauri.conf.json gives WebView2, so pass those too.
$conf = Get-Content (Join-Path $PSScriptRoot '..\..\src-tauri\tauri.conf.json') -Raw | ConvertFrom-Json
$appArgs = $conf.app.windows[0].additionalBrowserArgs
$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = "--remote-debugging-port=$Port $appArgs"
Start-Process -FilePath $Exe -WorkingDirectory $dir

for ($i = 0; $i -lt 40; $i++) {
    Start-Sleep -Milliseconds 500
    try {
        $pages = Invoke-RestMethod "http://127.0.0.1:$Port/json" -TimeoutSec 2
        if ($pages | Where-Object type -eq 'page') { "ready on port $Port"; exit 0 }
    } catch {}
}
Write-Error "no WebView2 page on port $Port after 20 s: is another Plugcam with the same identifier running? It takes over (single instance)."
