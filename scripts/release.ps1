$ErrorActionPreference = "Stop"
Set-Location (Resolve-Path "$PSScriptRoot\..")

cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all
cargo build --release --bin numlon

New-Item -ItemType Directory -Force dist | Out-Null
Copy-Item target\release\numlon.exe dist\numlon-windows-x64.exe -Force
Write-Host "Release asset: dist\numlon-windows-x64.exe"
