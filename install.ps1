#!/usr/bin/env pwsh
# adopted from https://github.com/denoland/deno_install/blob/master/install.ps1

$ErrorActionPreference = 'Stop'

if ($v) {
  $Version = "v${v}"
}

if ($args.Length -eq 1) {
  $Version = $args.Get(0)
}

$RelayerInstall = "$Home\.webb"
$BinDir = $RelayerInstall
$RelayerZip = "$BinDir\webb-relayer.zip"
$RelayerExe = "$BinDir\webb-relayer.exe"
$Target = 'x86_64-pc-windows-msvc'

# GitHub requires TLS 1.2
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12

$RelayerUri = if (!$Version) {
  "https://github.com/webb-tools/relayer/releases/latest/download/webb-relayer-${Target}.zip"
} else {
  "https://github.com/webb-tools/relayer/releases/download/${Version}/webb-relayer-${Target}.zip"
}

if (!(Test-Path $BinDir)) {
  New-Item $BinDir -ItemType Directory | Out-Null
}

Invoke-WebRequest $RelayerUri -OutFile $RelayerZip -UseBasicParsing

if (Get-Command Expand-Archive -ErrorAction SilentlyContinue) {
  Expand-Archive $RelayerZip -Destination $BinDir -Force
} else {
  if (Test-Path $RelayerExe) {
    Remove-Item $RelayerExe
  }
  Add-Type -AssemblyName System.IO.Compression.FileSystem
  [IO.Compression.ZipFile]::ExtractToDirectory($RelayerZip, $BinDir)
}

Remove-Item $RelayerZip

$User = [EnvironmentVariableTarget]::User
$Path = [Environment]::GetEnvironmentVariable('Path', $User)
if (!(";$Path;".ToLower() -like "*;$BinDir;*".ToLower())) {
  [Environment]::SetEnvironmentVariable('Path', "$Path;$BinDir", $User)
  $Env:Path += ";$BinDir"
}

Write-Output "Webb Relayer was installed successfully to $RelayerExe"
Write-Output "Run 'webb-relayer --help' to get started"
