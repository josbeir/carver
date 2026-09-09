#!/usr/bin/env bash
set -euo pipefail
image="$(realpath "${1:?Pass the AppImage path}")"
test_dir="$(mktemp -d)"
export XDG_CONFIG_HOME="$test_dir/config" XDG_DATA_HOME="$test_dir/data" XDG_CACHE_HOME="$test_dir/cache"
mkdir -m700 "$test_dir/runtime"
export XDG_RUNTIME_DIR="$test_dir/runtime"
# The isolated test session has no accessibility bus or persistent preferences.
export GTK_A11Y=none GSETTINGS_BACKEND=memory
host_helper_dir="$(pkg-config --variable=libdir webkitgtk-6.0)/webkitgtk-6.0"
# Hide the build host's helpers in a private mount namespace. A package that
# accidentally uses WebKit's compiled system path must fail this test.
test -d "$host_helper_dir"
set +e
bwrap --die-with-parent --ro-bind / / --dev-bind /dev /dev --proc /proc \
  --bind "$test_dir" "$test_dir" --bind /tmp /tmp \
  --tmpfs "$host_helper_dir" \
  dbus-run-session -- ./scripts/with-weston.sh timeout 20s "$image" --appimage-extract-and-run > "$test_dir/launch.log" 2>&1
status=$?
set -e
cat "$test_dir/launch.log"
echo "GUI launch exit status: $status"
# A healthy GUI remains open until timeout; crashes or missing libraries exit early.
[[ "$status" == 124 ]]
! grep -Ei 'error while loading shared libraries|symbol lookup error|Failed to launch|WebProcess.*crash|Unable to spawn|Failed to spawn|core dumped' "$test_dir/launch.log"
