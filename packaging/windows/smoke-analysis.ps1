# Actual process -> HTTP simulator -> engine -> native pivot -> XLSX round trip, on Windows.
param([string] $Bin = $PSScriptRoot)
$ErrorActionPreference = 'Stop'
$Work = Join-Path ([IO.Path]::GetTempPath()) ([Guid]::NewGuid().ToString())
New-Item -ItemType Directory $Work | Out-Null
$Processes = [Collections.Generic.List[Diagnostics.Process]]::new()
function Start-Synthetic([string] $Scenario) {
  $Log = Join-Path $Work "$Scenario.log"
  $Process = Start-Process (Join-Path $Bin 'gridcraft-bw-sim.exe') -ArgumentList '--port', '0', '--scenario', $Scenario -RedirectStandardOutput $Log -PassThru -NoNewWindow
  $Processes.Add($Process)
  for ($Attempt = 0; $Attempt -lt 50; $Attempt++) {
    if ($Process.HasExited) { throw "Simulator $Scenario exited at startup" }
    $Text = Get-Content $Log -Raw -ErrorAction SilentlyContinue
    if ($Text -match 'SYNTHETIC XMLA SIMULATOR: (http://127\.0\.0\.1:[0-9]+/xmla)') { return $Matches[1] }
    Start-Sleep -Milliseconds 100
  }
  throw "Simulator $Scenario did not become ready"
}
try {
  $Cli = Join-Path $Bin 'gridcraft-cli.exe'
  $Endpoint = Start-Synthetic 'normal'
  $Config = @{provider='bwSimulator';analyticalUrl=$Endpoint} | ConvertTo-Json -Compress
  $Query = @{query='ZDEMO_SALES';fiscalYear=0;rows='Region';columns='Product';measure='Revenue';bw=@{endpoint=$Endpoint;catalog='SYNTHETIC_BW';mdx='SELECT {[Measures].[Revenue]} ON COLUMNS, CrossJoin([Region].Members,[Product].Members) ON ROWS FROM [ZDEMO_SALES] WHERE ({{YEAR}})';variables=@{YEAR='[FiscalYear].[2026]'};unitField='Currency';simulator=$true}} | ConvertTo-Json -Depth 5 -Compress
  $Book = Join-Path $Work 'synthetic.xlsx'
  & $Cli run --cmd "analysis.configure=$Config" --cmd 'analysis.connect={}' --cmd "analysis.insert=$Query" --out $Book
  if ($LASTEXITCODE -ne 0 -or -not (Test-Path $Book)) { throw 'Network analysis insertion failed' }
  $Result = & $Cli run --in $Book --cmd "analysis.configure=$Config" --cmd 'analysis.connect={}' --cmd 'analysis.refresh={}' --cmd 'analysis.status={}' --out $Book | Out-String
  if ($LASTEXITCODE -ne 0 -or $Result -notmatch '"refreshCount"\s*:\s*2') { throw "Refresh/reopen failed: $Result" }
  Write-Output 'PASS: native Windows CLI, local HTTP XMLA, pivot creation, XLSX save/reopen, refresh count 2'
  foreach ($Scenario in 'unauthorized', 'forbidden', 'expired') {
    $Endpoint = Start-Synthetic $Scenario
    $Config = @{provider='bwSimulator';analyticalUrl=$Endpoint} | ConvertTo-Json -Compress
    $Rejected = Join-Path $Work "$Scenario.xlsx"
    # Expected errors are captured and asserted; only this native call may fail.
    $ErrorActionPreference = 'Continue'
    $Result = & $Cli run --cmd "analysis.configure=$Config" --cmd 'analysis.connect={}' --out $Rejected 2>&1 | Out-String
    $Exit = $LASTEXITCODE
    $ErrorActionPreference = 'Stop'
    $Code = if ($Scenario -eq 'forbidden') { '403' } else { '401' }
    if ($Exit -eq 0 -or (Test-Path $Rejected) -or $Result -notmatch $Code) { throw "Expected sanitized $Scenario rejection was not observed" }
    Write-Output "PASS: $Scenario rejected (HTTP $Code), nonzero exit, no workbook created"
  }
} finally {
  foreach ($Process in $Processes) { Stop-Process -Id $Process.Id -ErrorAction SilentlyContinue }
  Remove-Item -Recurse -Force $Work -ErrorAction SilentlyContinue
}
