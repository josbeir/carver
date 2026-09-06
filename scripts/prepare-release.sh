#!/usr/bin/env sh

set -eu

if [ -z "${WORKSPACE_ROOT:-}" ]; then
  printf '%s\n' 'WORKSPACE_ROOT must be set by cargo-release.' >&2
  exit 1
fi

sh "$WORKSPACE_ROOT/scripts/update-metainfo-release.sh"
sh "$WORKSPACE_ROOT/scripts/update-flatpak-sources.sh"
