#!/usr/bin/env sh

set -eu

if [ -z "${WORKSPACE_ROOT:-}" ]; then
  printf '%s\n' 'WORKSPACE_ROOT must be set by cargo-release.' >&2
  exit 1
fi

sh "$WORKSPACE_ROOT/scripts/update-metainfo-release.sh"
# The catalogs carry the workspace version in their Project-Id-Version header,
# so a version bump must regenerate them or CI's translation check fails.
bash "$WORKSPACE_ROOT/scripts/update-translations.sh"
sh "$WORKSPACE_ROOT/scripts/update-flatpak-sources.sh"
