#!/bin/sh
# Cross-builds the Windows installer on macOS or Linux.
# Needs the x86_64-pc-windows-msvc Rust target, cargo-xwin, LLVM and Node.
# On macOS: brew install llvm; cargo install --locked cargo-xwin.
set -eu
cd "$(dirname "$0")/.."
target=x86_64-pc-windows-msvc
out="target/$target/release"
# makensis is gone, but tools still misbehave with an empty locale.
export LC_ALL="${LC_ALL:-en_US.UTF-8}" LANG="${LANG:-en_US.UTF-8}"
if [ -d /opt/homebrew/opt/llvm/bin ]; then
  PATH="/opt/homebrew/opt/llvm/bin:$PATH"
fi
cargo run -p zbuild --locked -- prepare-windows
cargo xwin build --release -p zapretd --locked --target "$target"
cp "$out/zapretd.exe" resources/runtime/zapretd.exe
[ -d node_modules ] || npm ci
npx tauri build --runner cargo-xwin --target "$target" --no-bundle
mkdir -p work outputs
cargo run -p zbuild --locked -- installer-payload \
  --app "$out/zapret-desktop.exe" --service "$out/zapretd.exe" \
  --out "$PWD/work/installer-payload.tar.gz"
ZAPRET_SETUP_PAYLOAD="$PWD/work/installer-payload.tar.gz" \
  cargo xwin build --release -p zsetup --locked --target "$target"
cp "$out/zapret-setup.exe" outputs/Zapret-Desktop-Setup.exe
echo "Installer: outputs/Zapret-Desktop-Setup.exe"
