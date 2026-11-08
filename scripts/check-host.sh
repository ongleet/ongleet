#!/usr/bin/env bash
set -u
failed=0
case "$(uname -s)" in
  Linux)
    for package in gtk+-3.0 webkit2gtk-4.1 openssl; do
      if ! pkg-config --modversion "$package"; then failed=1; fi
    done
    printf 'Session: %s; Wayland display: %s; X display: %s\n' "${XDG_SESSION_TYPE:-unset}" "${WAYLAND_DISPLAY:-unset}" "${DISPLAY:-unset}"
    ;;
  Darwin)
    test "$(uname -m)" = arm64 || { echo 'macOS preview requires Apple Silicon'; failed=1; }
    xcrun --find clang || failed=1
    macos_version=$(sw_vers -productVersion)
    printf 'macOS: %s\n' "$macos_version"
    test "${macos_version%%.*}" -ge 14 || { echo 'macOS 14 or later is required'; failed=1; }
    ;;
  *) echo 'Supported hosts: Linux and macOS Apple Silicon'; failed=1 ;;
esac
mise exec rust@stable -- cargo --version || failed=1
mise exec -- node --version || failed=1
exit "$failed"
