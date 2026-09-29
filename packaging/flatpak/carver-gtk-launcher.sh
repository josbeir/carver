#!/bin/sh
# Carver's Flatpak launcher.
#
# Flatpak installs runtime locale data only for the languages listed in
# `flatpak config languages`. When the session locale is not among them, glibc
# falls back to the C locale and WebKit's Flatpak process launcher aborts the UI
# process with "Connection: failed to receive credentials". Fall back to a UTF-8
# locale so the editor still starts; the interface is English either way until
# the locale data is installed (see the Flatpak README).
requested="${LC_ALL:-${LC_MESSAGES:-${LANG:-}}}"
if [ -z "$requested" ] || ! locale -a 2>/dev/null | grep -qi "^${requested%%.*}"; then
  LC_ALL=C.UTF-8
  export LC_ALL
fi
exec /app/bin/carver-gtk.bin "$@"
