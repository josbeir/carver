#!/usr/bin/env bash
# Relocate only WebKit's compiled helper directory, preserving ELF offsets.
set -euo pipefail
library="${1:?Pass the WebKit library}"
output="${2:?Pass the private output library}"
helper_dir="${3:?Pass the original WebKit helper directory}"
replacement="${4:?Pass the private absolute helper directory}"
export LC_ALL=C
if [[ ! "$helper_dir" =~ ^/usr/[a-zA-Z0-9_./-]+$ || ! "$replacement" =~ ^/tmp/cv\.[a-zA-Z0-9]+/w$ ]] \
  || (( ${#replacement} > ${#helper_dir} )); then
  echo 'Unsupported WebKit relocation path' >&2
  exit 1
fi
if ! grep -aFq "$helper_dir" "$library"; then
  echo 'WebKit helper path not found; review upstream layout' >&2
  exit 1
fi
# Trailing slashes preserve the original byte count without changing ELF
# offsets or embedded string lengths. The directory remains an absolute path.
while (( ${#replacement} < ${#helper_dir} )); do replacement+=/; done
pattern="${helper_dir//./\\.}"
sed "s|$pattern|$replacement|g" "$library" > "$output"
