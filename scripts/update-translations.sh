#!/usr/bin/env bash
# Regenerate the gettext POT from Rust and resource sources, then refresh the
# committed PO catalogs. Run after adding, changing, or removing user-facing
# strings (see the Localization section of AGENTS.md).
set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$project_root"

domain="io.github.josbeir.Carver"
pot="po/${domain}.pot"
desktop="apps/carver-gtk/resources/${domain}.desktop"
metainfo="apps/carver-gtk/resources/${domain}.metainfo.xml"
metainfo_its="/usr/share/gettext/its/metainfo.its"

mapfile -t rust_sources < <(find apps/carver-gtk/src -name '*.rs' -print | sort)
if [ "${#rust_sources[@]}" -eq 0 ]; then
  echo "error: no Rust sources found under apps/carver-gtk/src" >&2
  exit 1
fi

version="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)"

common=(
  --from-code=UTF-8
  --package-name=Carver
  --package-version="$version"
  --msgid-bugs-address="https://github.com/josbeir/carver/issues"
)

rust_pot="${pot}.rust"
desktop_pot="${pot}.desktop"
metainfo_pot="${pot}.metainfo"

xgettext \
  --language=Rust \
  --add-comments \
  "${common[@]}" \
  -o "$rust_pot" \
  "${rust_sources[@]}"

xgettext \
  --language=Desktop \
  "${common[@]}" \
  -o "$desktop_pot" \
  "$desktop"

if [ -f "$metainfo_its" ]; then
  xgettext \
    --its="$metainfo_its" \
    "${common[@]}" \
    -o "$metainfo_pot" \
    "$metainfo"
else
  echo "warning: $metainfo_its not found; metainfo strings were not extracted" >&2
fi

msgcat --use-first --sort-by-file -o "$pot" "$rust_pot" "$desktop_pot" "$metainfo_pot"
rm -f "$rust_pot" "$desktop_pot" "$metainfo_pot"

shopt -s nullglob
for po in po/*.po; do
  msgmerge --no-wrap --update --backup=none "$po" "$pot"
done

echo "Updated $pot and po/*.po"
