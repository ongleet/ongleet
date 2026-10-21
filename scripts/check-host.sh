#!/usr/bin/env bash
set -u
failed=0
for package in gtk+-3.0 webkit2gtk-4.1 openssl; do
  if ! pkg-config --modversion "$package"; then failed=1; fi
done
mise exec rust@stable -- cargo --version || failed=1
node --version || failed=1
printf 'Session: %s; Wayland display: %s; X display: %s\n' "${XDG_SESSION_TYPE:-unset}" "${WAYLAND_DISPLAY:-unset}" "${DISPLAY:-unset}"
exit "$failed"
