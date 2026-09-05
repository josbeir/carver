#!/usr/bin/env sh

set -eu

if [ -z "${WORKSPACE_ROOT:-}" ] || [ -z "${NEW_VERSION:-}" ]; then
  printf '%s\n' 'WORKSPACE_ROOT and NEW_VERSION must be set by cargo-release.' >&2
  exit 1
fi

metainfo="$WORKSPACE_ROOT/apps/carver-gtk/resources/io.github.josbeir.Carver.metainfo.xml"
release="    <release version=\"$NEW_VERSION\" date=\"$(date +%F)\"/>"

if grep -Fq "version=\"$NEW_VERSION\"" "$metainfo"; then
  exit 0
fi

if [ "${DRY_RUN:-false}" = "true" ]; then
  printf '%s\n' "Would add $NEW_VERSION to $metainfo."
  exit 0
fi

temporary_metainfo="$(mktemp "$metainfo.XXXXXX")"
trap 'rm -f "$temporary_metainfo"' EXIT

awk -v release="$release" '
  { print }
  /  <releases>/ { print release }
' "$metainfo" > "$temporary_metainfo"

mv "$temporary_metainfo" "$metainfo"
trap - EXIT
