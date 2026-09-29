#!/bin/sh
# Carver's Flatpak launcher.
#
# Flatpak installs runtime locale data only for the languages listed in
# `flatpak config languages`. When any locale category resolves to a locale the
# runtime does not have, glibc keeps the `C` locale and WebKit's Flatpak process
# launcher aborts the UI process with "Connection: failed to receive
# credentials". Fall back to a UTF-8 locale so the editor still starts; the
# interface is English either way until the locale data is installed.
available="$(locale -a 2>/dev/null)"

is_available() {
  [ -z "$1" ] && return 0
  printf '%s\n' "$available" | grep -Eqi "^${1%%.*}([.@]|$)"
}

fallback=false
if [ -n "${LC_ALL:-}" ]; then
  is_available "$LC_ALL" || fallback=true
else
  for category in \
    LC_CTYPE LC_NUMERIC LC_TIME LC_COLLATE LC_MONETARY LC_MESSAGES \
    LC_PAPER LC_NAME LC_ADDRESS LC_TELEPHONE LC_MEASUREMENT LC_IDENTIFICATION
  do
    eval "value=\${$category:-\${LANG:-}}"
    is_available "$value" || fallback=true
  done
fi
# A `C`/`POSIX` or other non-UTF-8 locale cannot convert non-ASCII environment
# values, which is what aborts the WebKit launch.
[ "$(locale charmap 2>/dev/null)" = "UTF-8" ] || fallback=true

if [ "$fallback" = true ]; then
  LC_ALL=C.UTF-8
  export LC_ALL
fi
exec /app/bin/carver-gtk.bin "$@"
