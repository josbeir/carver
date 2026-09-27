#!/usr/bin/env bash
# Capture the docs site's application screenshots.
#
# Runs the opt-in `capture_docs_screenshots` test under the same isolated Weston
# harness the display-backed test suite uses, writing 2x light/dark PNGs into
# docs/src/assets/screenshots. Review the output and commit it by hand.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
output_dir="${1:-$repo_root/docs/src/assets/screenshots}"

mkdir -p "$output_dir"
export CARVER_SCREENSHOT_DIR="$output_dir"
echo "Capturing Carver screenshots into $output_dir"

"$repo_root/scripts/with-weston.sh" cargo test -p carver-gtk --locked -- \
  --include-ignored --test-threads=1 capture_docs_screenshots

echo
echo "Captured:"
ls -1 "$output_dir"
