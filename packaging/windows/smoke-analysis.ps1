# Actual process -> HTTP simulator -> engine -> native pivot -> XLSX round trip, on Windows.
param([string] $Bin = $PSScriptRoot)
$ErrorActionPreference = 'Stop'
$Sim = Start-Process (Join-Path $Bin 'gridcraft-bw-sim.exe') -ArgumentList '--port', '18787' -PassThru -NoNewWindow
$Work = Join-Path ([IO.Path]::GetTempPath()) ([Guid]::NewGuid().ToString())
New-Item -ItemType Directory $Work | Out-Null
try {
  Start-Sleep -Milliseconds 600
  $Cli = Join-Path $Bin 'gridcraft-cli.exe'
  $Endpoint = 'http://127.0.0.1:18787/xmla'
  $Config = @{provider='bwSimulator';analyticalUrl=$Endpoint} | ConvertTo-Json -Compress
  $Query = @{query='ZDEMO_SALES';fiscalYear=0;rows='Region';columns='Product';measure='Revenue';bw=@{endpoint=$Endpoint;catalog='SYNTHETIC_BW';mdx='SELECT {[Measures].[Revenue]} ON COLUMNS, CrossJoin([Region].Members,[Product].Members) ON ROWS FROM [ZDEMO_SALES] WHERE ({{YEAR}})';variables=@{YEAR='[FiscalYear].[2026]'};unitField='Currency';simulator=$true}} | ConvertTo-Json -Depth 5 -Compress
  $Book = Join-Path $Work 'synthetic.xlsx'
  & $Cli run --cmd "analysis.configure=$Config" --cmd 'analysis.connect={}' --cmd "analysis.insert=$Query" --out $Book
  if ($LASTEXITCODE -ne 0 -or -not (Test-Path $Book)) { throw 'Network analysis insertion failed' }
  $Result = & $Cli run --in $Book --cmd "analysis.configure=$Config" --cmd 'analysis.connect={}' --cmd 'analysis.refresh={}' --cmd 'analysis.status={}' --out $Book | Out-String
  if ($LASTEXITCODE -ne 0 -or $Result -notmatch '"refreshCount"\s*:\s*2') { throw "Refresh/reopen failed: $Result" }
  Write-Output 'PASS: native Windows CLI, local HTTP XMLA, pivot creation, XLSX save/reopen, refresh count 2'
} finally {
  Stop-Process -Id $Sim.Id -ErrorAction SilentlyContinue
  Remove-Item -Recurse -Force $Work -ErrorAction SilentlyContinue
}
