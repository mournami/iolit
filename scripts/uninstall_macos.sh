#!/bin/sh
set -eu
if [ "$(id -u)" -ne 0 ]; then
  printf '%s\n' 'Run this script with sudo to unregister the system service.' >&2
  exit 1
fi
"/Library/PrivilegedHelperTools/io.zapret.desktop.engine" uninstall-daemon \
  --label io.zapret.desktop.engine --plist /Library/LaunchDaemons/io.zapret.desktop.engine.plist \
  --data "/Library/Application Support/Zapret Desktop" \
  --socket /var/run/zapret-desktop/control.sock --anchor zapret-desktop
rm -f /Library/PrivilegedHelperTools/io.zapret.desktop.engine
printf '%s\n' 'Service removed. User data and the app were retained.'
