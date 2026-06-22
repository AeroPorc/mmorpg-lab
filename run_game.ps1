$ErrorActionPreference = 'Stop'
$root = $PSScriptRoot
Set-Location $root

$broker = '127.0.0.1:5000'

Write-Host 'Stopping any previous processes...' -ForegroundColor Cyan
Get-Process broker, gatekeeper, dedicated_server, client -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Milliseconds 500

Write-Host 'Building...' -ForegroundColor Cyan
cargo build -p broker -p gatekeeper -p dedicated_server -p client
if ($LASTEXITCODE -ne 0) { Write-Host 'Build failed.' -ForegroundColor Red; exit 1 }

function Start-Component {
    param([string]$Title, [string]$Cmd)
    $inner = "`$host.UI.RawUI.WindowTitle='$Title'; Set-Location '$root'; $Cmd"
    Start-Process powershell -ArgumentList '-NoExit', '-Command', $inner
}

Write-Host 'Launching broker + gatekeeper...' -ForegroundColor Cyan
Start-Component -Title 'game-broker' -Cmd 'cargo run -p broker'
Start-Component -Title 'game-gatekeeper' -Cmd 'cargo run -p gatekeeper'
Start-Sleep -Seconds 3

Write-Host 'Launching shard 0...' -ForegroundColor Cyan
Start-Component -Title 'game-shard-0' -Cmd "`$env:BROKER_ADDR='$broker'; `$env:SHARD_ID='0'; `$env:SHARD_COUNT='1'; `$env:ARENA_WIDTH='900'; `$env:ARENA_HEIGHT='600'; `$env:QUAD_DEPTH='0'; cargo run -p dedicated_server"
Start-Sleep -Seconds 2

Write-Host 'Launching client (a window will open)...' -ForegroundColor Cyan
Start-Component -Title 'game-client' -Cmd 'cargo run -p client'

Write-Host ''
Write-Host 'A game window should open. Controls: WASD / arrow keys.' -ForegroundColor Green
Write-Host 'Green square = you, red squares = enemies.' -ForegroundColor Green
