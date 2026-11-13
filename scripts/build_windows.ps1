$ErrorActionPreference = 'Stop'
Set-Location (Split-Path $PSScriptRoot -Parent)
cargo run -p zbuild --locked -- prepare-windows
if ($LASTEXITCODE -ne 0) { throw 'Engine preparation failed' }
cargo build --release -p zapretd --locked
if ($LASTEXITCODE -ne 0) { throw 'Service build failed' }
Copy-Item target/release/zapretd.exe resources/runtime/zapretd.exe -Force
npm ci
if ($LASTEXITCODE -ne 0) { throw 'Dependency installation failed' }
npm run desktop:build -- --no-bundle
if ($LASTEXITCODE -ne 0) { throw 'Application build failed' }
New-Item -ItemType Directory -Force work, outputs | Out-Null
cargo run -p zbuild --locked -- installer-payload --app target/release/zapret-desktop.exe --service target/release/zapretd.exe --out work/installer-payload.tar.gz
if ($LASTEXITCODE -ne 0) { throw 'Installer payload failed' }
$env:ZAPRET_SETUP_PAYLOAD = (Resolve-Path work/installer-payload.tar.gz).Path
cargo build --release -p zsetup --locked
if ($LASTEXITCODE -ne 0) { throw 'Installer build failed' }
Copy-Item target/release/zapret-setup.exe outputs/Zapret-Desktop-Setup.exe -Force
Write-Host 'Installer: outputs/Zapret-Desktop-Setup.exe'
