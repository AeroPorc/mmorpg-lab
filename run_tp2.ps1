$ErrorActionPreference = 'Stop'
$root = $PSScriptRoot
Set-Location $root

$broker = '127.0.0.1:5000'

Write-Host 'Stopping any previous tp2 processes...' -ForegroundColor Cyan
Get-Process broker, spatial_service, dedicated_server, orchestrator -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Milliseconds 500

Write-Host 'Building broker, spatial_service, orchestrator, dedicated_server...' -ForegroundColor Cyan
cargo build -p broker -p spatial_service -p orchestrator -p dedicated_server
if ($LASTEXITCODE -ne 0) { Write-Host 'Build failed.' -ForegroundColor Red; exit 1 }

function Start-Component {
    param([string]$Title, [string]$Cmd)
    $inner = "`$host.UI.RawUI.WindowTitle='$Title'; Set-Location '$root'; $Cmd"
    Start-Process powershell -ArgumentList '-NoExit', '-Command', $inner
}

Write-Host 'Launching broker + spatial service...' -ForegroundColor Cyan
Start-Component -Title 'tp2-broker' -Cmd 'cargo run -p broker'
Start-Sleep -Seconds 3
Start-Component -Title 'tp2-spatial' -Cmd "`$env:BROKER_ADDR='$broker'; `$env:SHARD_COUNT='2'; cargo run -p spatial_service"

Write-Host 'Launching orchestrator (it spawns + supervises shard 0 and shard 1)...' -ForegroundColor Cyan
Start-Component -Title 'tp2-orchestrator' -Cmd "`$env:BROKER_ADDR='$broker'; `$env:SHARD_COUNT='2'; `$env:SPAWN_DUMMY_SHARD='0'; cargo run -p orchestrator"

Write-Host ''
Write-Host 'Launched. What to watch for:' -ForegroundColor Green
Write-Host '  tp2-orchestrator : "spawning" shard 0 and 1, then "Heartbeat from shard X" every ~2s'
Write-Host '  shard 0 window   : Spawned test entity -> "spawned N enemies / culled N enemies"'
Write-Host '  tp2-spatial      : "CrossingAlert: entity 1 owned by shard 0 approaching shard 1"'
Write-Host '  shard 1 window   : receives 0x20 HandoffRequest -> 0x21 Accept -> 0x24 Complete'
Write-Host 'Tip: close a shard window and watch the orchestrator respawn it.' -ForegroundColor Green
