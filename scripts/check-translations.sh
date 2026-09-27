#!/usr/bin/env bash
# Fail when the committed gettext catalogs do not match the current sources.
#
# Only message content is enforced. References are extracted at file granularity
# (`--add-location=file`) and the `#:` lines are ignored here, so a line shift or a refactor can
# never make a catalog stale; adding, changing, or removing a string still fails the check until
# the regenerated catalogs are committed.
set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$project_root"

./scripts/update-translations.sh

if ! git diff --exit-code -I 'POT-Creation-Date:' -I 'rust-format' -I '^#:' -- po/; then
  echo "error: translation catalogs are out of date; commit the regenerated po/ files" >&2
  exit 1
fi

echo "Translation catalogs match the current sources"
