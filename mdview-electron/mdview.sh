#!/bin/bash
# Launch mdview-electron as a Windows-native process from WSL.
# Prereq: Windows 側で `npm install -g electron@34` 済み
set -e

# Find Windows-side electron.exe (auto-detect from /mnt/c/Users/*)
ELECTRON_EXE="${MDVIEW_ELECTRON_EXE:-}"
if [ -z "$ELECTRON_EXE" ]; then
  for f in /mnt/c/Users/*/AppData/Roaming/npm/node_modules/electron/dist/electron.exe; do
    [ -x "$f" ] && ELECTRON_EXE="$f" && break
  done
fi

if [ -z "$ELECTRON_EXE" ] || [ ! -x "$ELECTRON_EXE" ]; then
  echo "error: Windows-side electron.exe not found." >&2
  echo "  Windows 側で: npm install -g electron@34" >&2
  echo "  または: MDVIEW_ELECTRON_EXE=/mnt/c/path/to/electron.exe $0 ..." >&2
  exit 1
fi

SCRIPT_DIR="$(dirname "$(readlink -f "$0")")"
FILE_ABS=""
[ -n "$1" ] && FILE_ABS="$(readlink -f "$1")"

APP_WIN="$(wslpath -w "$SCRIPT_DIR")"
if [ -n "$FILE_ABS" ]; then
  exec "$ELECTRON_EXE" "$APP_WIN" "$(wslpath -w "$FILE_ABS")"
else
  exec "$ELECTRON_EXE" "$APP_WIN"
fi
