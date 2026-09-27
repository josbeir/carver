#!/usr/bin/env bash
# Capture the docs site's application screenshots.
#
# The capture lives in the display-backed test orchestrator as an opt-in step, so
# this runs the ignored suite the same way the CI harness does and sets
# CARVER_SCREENSHOT_DIR to switch the capture on. It writes 2x light/dark PNGs
# into docs/src/assets/screenshots; review the output and commit it by hand.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
output_dir="${1:-$repo_root/docs/src/assets/screenshots}"

mkdir -p "$output_dir"
export CARVER_SCREENSHOT_DIR="$output_dir"
echo "Capturing Carver screenshots into $output_dir"

"$repo_root/scripts/with-weston.sh" cargo test -p carver-gtk --locked -- \
  --include-ignored --test-threads=1 mvu_window_should_keep_sidebar

echo
echo "Captured:"
ls -1 "$output_dir"
