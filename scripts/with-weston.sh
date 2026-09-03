#!/usr/bin/env bash
# Run one command inside an isolated, native Wayland compositor.
#
# This is deliberately a Weston harness rather than an Xvfb compatibility
# layer: Carver is a GTK/Wayland application and its interaction tests should
# exercise the Wayland backend used in production.
set -euo pipefail

if (( $# == 0 )); then
  echo "usage: $0 command [arguments...]" >&2
  exit 64
fi

runtime_dir="${XDG_RUNTIME_DIR:-}"
owns_runtime_dir=false
if [[ -z "$runtime_dir" || ! -d "$runtime_dir" || ! -O "$runtime_dir" || ! -w "$runtime_dir" ]]; then
  runtime_dir="$(mktemp -d "${TMPDIR:-/tmp}/carver-wayland.XXXXXX")"
  owns_runtime_dir=true
fi
socket_name="carver-test-${RANDOM}"
weston_log="$(mktemp "${TMPDIR:-/tmp}/carver-weston.XXXXXX.log")"
weston_pid=""

cleanup() {
  if [[ -n "$weston_pid" ]]; then
    kill "$weston_pid" 2>/dev/null || true
    wait "$weston_pid" 2>/dev/null || true
  fi
  if [[ "$owns_runtime_dir" == true ]]; then
    rm -rf "$runtime_dir"
  fi
  rm -f "$weston_log"
}
trap cleanup EXIT INT TERM

if [[ "$owns_runtime_dir" == true ]]; then
  chmod 700 "$runtime_dir"
fi
XDG_RUNTIME_DIR="$runtime_dir" weston \
  --backend=headless \
  --renderer=pixman \
  --socket="$socket_name" \
  --width=1280 \
  --height=900 \
  --idle-time=0 \
  --fake-seat \
  --log="$weston_log" &
weston_pid=$!

for _ in $(seq 1 100); do
  if [[ -S "$runtime_dir/$socket_name" ]]; then
    exec env \
      XDG_RUNTIME_DIR="$runtime_dir" \
      WAYLAND_DISPLAY="$socket_name" \
      GDK_BACKEND=wayland \
      GSK_RENDERER="${GSK_RENDERER:-cairo}" \
      "$@"
  fi
  if ! kill -0 "$weston_pid" 2>/dev/null; then
    cat "$weston_log" >&2
    exit 1
  fi
  sleep 0.05
done

echo "Timed out waiting for Weston to create $socket_name" >&2
cat "$weston_log" >&2
exit 1
