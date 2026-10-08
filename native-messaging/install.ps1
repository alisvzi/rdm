<#
  Installs the RDM Native Messaging host for the current Windows user (no admin rights needed).

  Run from the folder that contains rdm-nm.exe (the "dist" folder), in PowerShell:

     powershell -ExecutionPolicy Bypass -File install.ps1 -ChromeExtensionId <the extension ID>

  The extension ID is shown at chrome://extensions (or edge://extensions) after you load the
  "extension" folder. Leave it out to install for Firefox only.
  Undo with:  install.ps1 -Remove
#>
param(
  [string]$ChromeExtensionId = "",
  [switch]$Remove
)

$ErrorActionPreference = "Stop"
$name = "com.rdm.downloadmanager"
$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$exe  = Join-Path $here "rdm-nm.exe"

$keys = @(
  "HKCU:\Software\Google\Chrome\NativeMessagingHosts\$name",
  "HKCU:\Software\Microsoft\Edge\NativeMessagingHosts\$name",
  "HKCU:\Software\BraveSoftware\Brave-Browser\NativeMessagingHosts\$name",
  "HKCU:\Software\Mozilla\NativeMessagingHosts\$name"
)

if ($Remove) {
  foreach ($k in $keys) { if (Test-Path $k) { Remove-Item $k -Recurse -Force } }
  Write-Host "Removed."
  exit 0
}

if (-not (Test-Path $exe)) { throw "rdm-nm.exe was not found next to this script ($exe)." }

# Chromium browsers (Chrome, Edge, Brave)
if ($ChromeExtensionId -ne "") {
  $manifest = [ordered]@{
    name = $name
    description = "RDM Download Manager"
    path = $exe
    type = "stdio"
    allowed_origins = @("chrome-extension://$ChromeExtensionId/")
  }
  $file = Join-Path $here "$name.chrome.json"
  $manifest | ConvertTo-Json | Set-Content -Path $file -Encoding UTF8
  foreach ($k in $keys[0..2]) {
    New-Item -Path $k -Force | Out-Null
    Set-ItemProperty -Path $k -Name "(default)" -Value $file
  }
  Write-Host "Installed for Chrome / Edge / Brave."
}

# Firefox
$ff = [ordered]@{
  name = $name
  description = "RDM Download Manager"
  path = $exe
  type = "stdio"
  allowed_extensions = @("rdm@rdm.local")
}
$ffFile = Join-Path $here "$name.firefox.json"
$ff | ConvertTo-Json | Set-Content -Path $ffFile -Encoding UTF8
New-Item -Path $keys[3] -Force | Out-Null
Set-ItemProperty -Path $keys[3] -Name "(default)" -Value $ffFile
Write-Host "Installed for Firefox."
Write-Host "Keep this folder where it is: the browsers read the files from here."
