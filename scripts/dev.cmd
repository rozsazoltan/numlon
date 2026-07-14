@echo off
setlocal
cd /d "%~dp0.."
echo Numlon dev launcher
echo Using project-local dev runner; cargo-watch is not required.
cargo run --bin numlon-dev --
