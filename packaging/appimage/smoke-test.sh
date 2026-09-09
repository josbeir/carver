#!/usr/bin/env bash
set -euo pipefail
image="$(realpath "${1:?Pass the AppImage path}")"
test_dir="$(mktemp -d)"
export XDG_CONFIG_HOME="$test_dir/config" XDG_DATA_HOME="$test_dir/data" XDG_CACHE_HOME="$test_dir/cache"
# The isolated test session has no accessibility bus or persistent preferences.
export GTK_A11Y=none GSETTINGS_BACKEND=memory
set +e
dbus-run-session -- ./scripts/with-weston.sh timeout 20s "$image" --appimage-extract-and-run > "$test_dir/launch.log" 2>&1
status=$?
set -e
cat "$test_dir/launch.log"
echo "GUI launch exit status: $status"
# A healthy GUI remains open until timeout; crashes or missing libraries exit early.
[[ "$status" == 124 ]]
! grep -Ei 'error while loading shared libraries|symbol lookup error|Failed to launch|WebProcess.*crash' "$test_dir/launch.log"
