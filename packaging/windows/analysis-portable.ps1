# Independent fork package: portable, unsigned, no installer, signing secret, association or release.
param([switch] $SkipBuild)
$ErrorActionPreference = 'Stop'
$Root = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
Push-Location $Root
try {
  $Target = 'x86_64-pc-windows-msvc'
  $env:CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS = '-C target-feature=+crt-static'
  $env:GRIDCRAFT_REQUIRE_WINRES = '1'
  $env:GRIDCRAFT_BUILD_SHA = (git rev-parse HEAD)
  if (-not $SkipBuild) {
    cargo build --locked --release --target $Target -p gridcraft -p gridcraft-cli -p gridcraft-bw-sim -p gridcraft-bw
    if ($LASTEXITCODE -ne 0) { throw 'Windows build failed' }
  }
  $Bin = Join-Path $Root "target\$Target\release"
  $Stage = Join-Path $Root 'target\analysis-windows-package'
  if (Test-Path $Stage) { Remove-Item -Recurse -Force $Stage }
  New-Item -ItemType Directory -Force $Stage | Out-Null
  foreach ($Name in 'gridcraft', 'gridcraft-cli', 'gridcraft-bw', 'gridcraft-bw-sim') {
    $Exe = Join-Path $Bin "$Name.exe"
    $Bytes = [IO.File]::ReadAllBytes($Exe)
    $Pe = [BitConverter]::ToInt32($Bytes, 0x3c)
    if ([BitConverter]::ToUInt16($Bytes, $Pe + 4) -ne 0x8664) { throw "$Name is not Windows x64" }
    $ExpectedSubsystem = if ($Name -eq 'gridcraft') { 2 } else { 3 }
    if ([BitConverter]::ToUInt16($Bytes, $Pe + 0x5c) -ne $ExpectedSubsystem) { throw "$Name has wrong subsystem" }
    Copy-Item $Exe $Stage
  }
  Copy-Item LICENSE-MIT, LICENSE-APACHE, NOTICE, ATTRIBUTION.md, README.md $Stage
  Copy-Item docs/analysis-workspace.md, docs/windows-analysis.md $Stage
  Copy-Item packaging/windows/smoke-analysis.ps1 $Stage
  "Commit: $env:GRIDCRAFT_BUILD_SHA`nWindows x64 MSVC. Unsigned experimental independent fork. Synthetic simulator included; live SAP validation pending." | Set-Content (Join-Path $Stage 'BUILD.txt')
  & (Join-Path $Stage 'gridcraft-cli.exe') eval '=SUM(1,2,3)'
  if ($LASTEXITCODE -ne 0) { throw 'CLI smoke failed' }
  & (Join-Path $Stage 'smoke-analysis.ps1') -Bin $Stage
  $Dist = Join-Path $Root 'dist\analysis'
  New-Item -ItemType Directory -Force $Dist | Out-Null
  $Zip = Join-Path $Dist 'gridcraft-analysis-windows-x64-portable.zip'
  Compress-Archive -Path "$Stage\*" -DestinationPath $Zip -Force
  $Hash = (Get-FileHash $Zip -Algorithm SHA256).Hash.ToLowerInvariant()
  "$Hash  gridcraft-analysis-windows-x64-portable.zip" | Set-Content "$Zip.sha256"
  Copy-Item (Join-Path $Stage 'BUILD.txt') $Dist
} finally { Pop-Location }
