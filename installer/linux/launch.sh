#!/bin/sh
umask 077
case "$0" in
  */coven-host-desktop) app=/usr/lib/coven-host-desktop; mode=launch ;;
  *) app=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd) || exit 1; mode=portable ;;
esac
if command -v python3 >/dev/null 2>&1 && python3 -Ic 'import sys; sys.exit(sys.version_info < (3, 8))' 2>/dev/null; then
    exec python3 -I "$app/desktop.py" "$mode" "$@"
fi
kernel=$(uname -r | sed -n 's/^\([0-9]*\.[0-9]*\.[0-9]*\).*/\1/p')
libc=$(getconf GNU_LIBC_VERSION 2>/dev/null | sed -n '/^glibc [0-9][0-9.]*$/p')
report=$(printf 'Coven Host needs Python 3.8 or later. Install Python 3 with your Linux software manager, then open Coven Host again.\nCode: CH-LINUX-PYTHON\nStage: start launcher\nVersion: 0.1.2\nTime (UTC): %s\nPlatform: Linux\nArchitecture: %s\nKernel: %s\nC runtime: %s\nPython: missing or too old\nExpected: Python 3.8+\nActual: Python missing or too old\nReproduce: Open the Coven Host launcher in your user session.\nNext step: Install Python 3, then open Coven Host again.' "$(date -u '+%Y-%m-%dT%H:%M:%SZ')" "$(uname -m)" "${kernel:-unavailable}" "${libc:-unavailable}")
printf '%s\n' "$report" >&2
file=$(mktemp "${TMPDIR:-/tmp}/coven-host-diagnostic.XXXXXXXX") && printf '%s\n' "$report" > "$file"
if [ -n "${DISPLAY:-}${WAYLAND_DISPLAY:-}" ]; then
    if command -v zenity >/dev/null; then zenity --text-info --title='Coven Host needs Python 3' --filename="$file"
    elif command -v kdialog >/dev/null; then kdialog --textbox "$file"
    elif command -v xdg-open >/dev/null; then xdg-open "$file"; fi
fi
exit 1
