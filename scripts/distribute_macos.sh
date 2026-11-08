#!/bin/sh
set -eu
export COPYFILE_DISABLE=1
PROJECT_ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$PROJECT_ROOT"
APP="$PROJECT_ROOT/target/release/bundle/macos/Zapret Desktop.app"
test -d "$APP"
ARCH=$(uname -m)
mkdir -p outputs work/dmg-root
cp -R "$APP" work/dmg-root/
ln -sfn /Applications work/dmg-root/Applications
# Plain image creation does not automate Finder or require AppleEvents permission.
hdiutil create -ov -volname 'Zapret Desktop' -format UDZO -srcfolder work/dmg-root "outputs/Zapret-Desktop-0.1.0-macos-$ARCH.dmg"
FULL="$PROJECT_ROOT/work/full-pkg-root"
mkdir -p "$FULL/Applications"
cp -R work/pkg-root/. "$FULL/"
cp -R "$APP" "$FULL/Applications/"
pkgbuild --analyze --root "$FULL" work/components.plist
python3 - <<'PY'
import plistlib
p='work/components.plist'
with open(p,'rb') as f:data=plistlib.load(f)
for component in data:component['BundleIsRelocatable']=False
with open(p,'wb') as f:plistlib.dump(data,f)
PY
pkgbuild --root "$FULL" --component-plist work/components.plist --identifier io.zapret.desktop.full \
  --version 0.1.0 --install-location / --scripts packaging/macos/scripts "outputs/Zapret-Desktop-0.1.0-macos-$ARCH.pkg"
if [ -n "${ZAPRET_INSTALLER_IDENTITY:-}" ]; then
  productsign --sign "$ZAPRET_INSTALLER_IDENTITY" "outputs/Zapret-Desktop-0.1.0-macos-$ARCH.pkg" work/full-signed.pkg
  mv work/full-signed.pkg "outputs/Zapret-Desktop-0.1.0-macos-$ARCH.pkg"
fi
ditto -c -k --sequesterRsrc --keepParent "$APP" "outputs/Zapret-Desktop-0.1.0-macos-$ARCH.zip"
