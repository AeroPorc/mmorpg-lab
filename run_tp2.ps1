$ErrorActionPreference = 'Stop'
$root = $PSScriptRoot
Set-Location $root

$broker = '127.0.0.1:5000'

Write-Host 'Stopping any previous tp2 processes...' -ForegroundColor Cyan
Get-Process broker, spatial_service, dedicated_server -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Milliseconds 500

Write-Host 'Building broker, spatial_service, dedicated_server...' -ForegroundColor Cyan
cargo build -p broker -p spatial_service -p dedicated_server
if ($LASTEXITCODE -ne 0) { Write-Host 'Build failed.' -ForegroundColor Red; exit 1 }

function Start-Component {
    param([string]$Title, [string]$Cmd)
    $inner = "`$host.UI.RawUI.WindowTitle='$Title'; Set-Location '$root'; $Cmd"
    Start-Process powershell -ArgumentList '-NoExit', '-Command', $inner
}

Write-Host 'Launching broker...' -ForegroundColor Cyan
Start-Component -Title 'tp2-broker' -Cmd 'cargo run -p broker'

Start-Sleep -Seconds 3

Write-Host 'Launching spatial service + shard 0 (with test entity) + shard 1...' -ForegroundColor Cyan
Start-Component -Title 'tp2-spatial' -Cmd "`$env:BROKER_ADDR='$broker'; `$env:SHARD_COUNT='2'; cargo run -p spatial_service"
Start-Component -Title 'tp2-shard-0' -Cmd "`$env:BROKER_ADDR='$broker'; `$env:SHARD_ID='0'; `$env:SHARD_COUNT='2'; `$env:SPAWN_DUMMY='1'; cargo run -p dedicated_server"
Start-Component -Title 'tp2-shard-1' -Cmd "`$env:BROKER_ADDR='$broker'; `$env:SHARD_ID='1'; `$env:SHARD_COUNT='2'; cargo run -p dedicated_server"

Write-Host ''
Write-Host 'Launched. What to watch for:' -ForegroundColor Green
Write-Host '  tp2-shard-0 : JOIN -> WELCOME -> Registered pub/sub -> Spawned test entity -> "spawned N enemies / culled N enemies"'
Write-Host '  tp2-spatial : "CrossingAlert: entity 1 owned by shard 0 approaching shard 1"'
Write-Host '  tp2-shard-1 : receives 0x20 HandoffRequest, sends 0x21 Accept, then 0x24 Complete -> takes ownership'
