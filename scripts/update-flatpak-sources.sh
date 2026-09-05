#!/usr/bin/env bash
# Regenerate Flatpak's offline source manifests from the committed lockfiles.
set -euo pipefail

if ! command -v flatpak-cargo-generator >/dev/null; then
  echo 'flatpak-cargo-generator is required; install flatpak-builder-tools first.' >&2
  exit 1
fi

if ! command -v flatpak-node-generator >/dev/null; then
  echo 'flatpak-node-generator is required; install flatpak-builder-tools first.' >&2
  exit 1
fi

flatpak-cargo-generator Cargo.lock -o packaging/flatpak/cargo-sources.json
flatpak-node-generator npm apps/carver-gtk/web/package-lock.json \
  -o packaging/flatpak/node-sources.json
