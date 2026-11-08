#!/bin/sh
set -eu
export COPYFILE_DISABLE=1
PROJECT_ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$PROJECT_ROOT"
STAGE="$PROJECT_ROOT/work/pkg-root"
mkdir -p "$STAGE/Library/PrivilegedHelperTools" "$STAGE/Library/Application Support/Zapret Desktop" outputs
install -m 0755 resources/runtime/macos/zapretd "$STAGE/Library/PrivilegedHelperTools/io.zapret.desktop.engine"
cp -R resources/runtime/macos/data/. "$STAGE/Library/Application Support/Zapret Desktop/"
mkdir -p "$STAGE/Library/Application Support/Zapret Desktop/tools"
install -m 0755 resources/runtime/macos/zaprctl "$STAGE/Library/Application Support/Zapret Desktop/tools/zaprctl"
chmod 0755 packaging/macos/scripts/postinstall
chmod 0755 packaging/macos/scripts/preinstall
pkgbuild --root "$STAGE" --identifier io.zapret.desktop.service --version 0.1.0 --install-location / \
  --scripts packaging/macos/scripts resources/runtime/ZapretService.pkg
if [ -n "${ZAPRET_INSTALLER_IDENTITY:-}" ]; then
  productsign --sign "$ZAPRET_INSTALLER_IDENTITY" resources/runtime/ZapretService.pkg work/ZapretService-signed.pkg
  mv work/ZapretService-signed.pkg resources/runtime/ZapretService.pkg
fi
printf '%s\n' 'Service package prepared. Build the app next with npm run desktop:build.'
