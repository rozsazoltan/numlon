$ErrorActionPreference = "Stop"
Set-Location (Resolve-Path "$PSScriptRoot\..")

Write-Host "Numlon dev launcher"
Write-Host "Using project-local dev runner; cargo-watch is not required."

cargo run --bin numlon-dev --
