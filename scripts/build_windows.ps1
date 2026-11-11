$ErrorActionPreference = 'Stop'
Set-Location (Split-Path $PSScriptRoot -Parent)
cargo run -p zbuild --locked -- prepare-windows
if ($LASTEXITCODE -ne 0) { throw 'Engine preparation failed' }
cargo build --release -p zapretd --locked
if ($LASTEXITCODE -ne 0) { throw 'Service build failed' }
Copy-Item target/release/zapretd.exe resources/runtime/zapretd.exe -Force
npm ci
if ($LASTEXITCODE -ne 0) { throw 'Dependency installation failed' }
npm run desktop:build -- --bundles nsis
if ($LASTEXITCODE -ne 0) { throw 'Application build failed' }
