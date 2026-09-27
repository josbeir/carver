#!/usr/bin/env bash
# Fail when the committed gettext catalogs do not match the current sources.
#
# The POT/PO files record a `#:` reference (file:line) for every message, so ANY
# source edit that shifts line numbers in a file containing `gettext` calls
# makes the catalogs stale — even when no string changed. CI regenerates the
# catalogs and requires them to be already committed, so run this before
# handing off a change; it regenerates them and fails until you commit them.
set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$project_root"

./scripts/update-translations.sh

if ! git diff --exit-code -I 'POT-Creation-Date:' -I 'rust-format' -- po/; then
  echo "error: translation catalogs are out of date; commit the regenerated po/ files" >&2
  exit 1
fi

echo "Translation catalogs match the current sources"
