#!/usr/bin/env bash
# Compile PO catalogs and the translated desktop/metainfo files into a
# destination tree (for example /app/share for Flatpak or the AppDir for
# AppImage). Translations are validated with `msgfmt --check`.
set -euo pipefail

if [ "$#" -ne 1 ]; then
  echo "usage: $0 <destdir>" >&2
  exit 2
fi

destdir="$1"
project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$project_root"

domain="io.github.josbeir.Carver"
desktop="apps/carver-gtk/resources/${domain}.desktop"
metainfo="apps/carver-gtk/resources/${domain}.metainfo.xml"

languages="$(sed -e 's/#.*//' -e '/^[[:space:]]*$/d' po/LINGUAS)"
if [ -z "$languages" ]; then
  echo "error: po/LINGUAS lists no languages" >&2
  exit 1
fi

for lang in $languages; do
  if [ ! -f "po/$lang.po" ]; then
    echo "error: po/$lang.po is listed in po/LINGUAS but missing" >&2
    exit 1
  fi
  install -d "$destdir/locale/$lang/LC_MESSAGES"
  msgfmt --check -c \
    -o "$destdir/locale/$lang/LC_MESSAGES/${domain}.mo" \
    "po/$lang.po"
done

install -d "$destdir/applications" "$destdir/metainfo"
msgfmt --desktop --template="$desktop" -d po -o "$destdir/applications/${domain}.desktop"
msgfmt --xml --template="$metainfo" -d po -o "$destdir/metainfo/${domain}.metainfo.xml"

echo "Installed translations into $destdir"
