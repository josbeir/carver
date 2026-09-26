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

# Test modules hold no translatable strings and trip the fallback C parser, so
# they are excluded from extraction.
mapfile -t rust_sources < <(
  find apps/carver-gtk/src -name '*.rs' ! -path '*/tests/*' ! -name 'tests.rs' -print | sort
)
if [ "${#rust_sources[@]}" -eq 0 ]; then
  echo "error: no Rust sources found under apps/carver-gtk/src" >&2
  exit 1
fi

# GNU gettext parses Rust natively from 0.24 onward. Older releases (for
# example Ubuntu's gettext 0.23.2) only ship the C parser, so fall back to it;
# the extracted message body is identical, only the `rust-format` flags differ.
probe="$(mktemp)"
printf 'gettext("probe");\n' > "$probe"
if xgettext --language=Rust -o "$probe.pot" "$probe" >/dev/null 2>&1; then
  rust_language="Rust"
else
  rust_language="C"
  echo "warning: xgettext lacks Rust support; extracting as C (gettext >= 0.24 recommended)" >&2
fi
rm -f "$probe" "$probe.pot"

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
  --language="$rust_language" \
  --add-comments \
  --keyword=gettext \
  --keyword=ngettext:1,2 \
  --keyword=pgettext:1c,2 \
  --keyword=npgettext:1c,2,3 \
  "${common[@]}" \
  -o "$rust_pot" \
  "${rust_sources[@]}"

xgettext \
  --language=Desktop \
  "${common[@]}" \
  -o "$desktop_pot" \
  "$desktop"

inputs=("$rust_pot" "$desktop_pot")
if [ -f "$metainfo_its" ]; then
  xgettext \
    --its="$metainfo_its" \
    "${common[@]}" \
    -o "$metainfo_pot" \
    "$metainfo"
  inputs+=("$metainfo_pot")
else
  echo "warning: $metainfo_its not found; metainfo strings were not extracted" >&2
fi

msgcat --use-first --sort-by-file -o "$pot" "${inputs[@]}"
rm -f "$rust_pot" "$desktop_pot" "$metainfo_pot"

shopt -s nullglob
for po in po/*.po; do
  msgmerge --no-wrap --update --backup=none "$po" "$pot"
  # Drop entries whose source string was removed so catalogs stay clean.
  msgattrib --no-obsolete --no-wrap "$po" > "$po.tmp"
  mv "$po.tmp" "$po"
done

echo "Updated $pot and po/*.po"
