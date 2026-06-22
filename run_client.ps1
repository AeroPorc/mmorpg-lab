$ErrorActionPreference = 'Stop'
Set-Location $PSScriptRoot
Write-Host 'Launching an extra client...' -ForegroundColor Cyan
cargo run -p client
