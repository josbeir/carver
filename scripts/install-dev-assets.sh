#!/usr/bin/env bash
# Install Carver's desktop integration for source-tree development.
set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
data_home="${XDG_DATA_HOME:-$HOME/.local/share}"
desktop_dir="$data_home/applications"
icon_theme_dir="$data_home/icons/hicolor"
icon_dir="$icon_theme_dir/scalable/apps"

install -d "$desktop_dir" "$icon_dir"
# A previous development installer accidentally wrote a replacement Hicolor
# index.  That masks the system theme and makes unrelated applications fall
# back to generic icons. Remove only that known bad file and its cache.
if [[ -f "$icon_theme_dir/index.theme" ]] \
  && grep -Fqx 'Name=Carver' "$icon_theme_dir/index.theme"; then
  rm -f "$icon_theme_dir/index.theme" "$icon_theme_dir/icon-theme.cache"
fi
# The distributable desktop entry launches the installed `carver-gtk` binary.
# For source development that binary is absent, and GLib then ignores the
# entry entirely. Generate a developer entry that invokes this workspace.
while IFS= read -r line || [[ -n "$line" ]]; do
  if [[ "$line" == Exec=* ]]; then
    printf 'Exec=cargo run --manifest-path %s --package carver-gtk -- %%U\n' \
      "$project_root/Cargo.toml"
  else
    printf '%s\n' "$line"
  fi
done < "$project_root/apps/carver-gtk/resources/io.github.josbeir.Carver.desktop" \
  > "$desktop_dir/io.github.josbeir.Carver.desktop"
install -m 644 \
  "$project_root/apps/carver-gtk/resources/icons/hicolor/scalable/apps/io.github.josbeir.Carver.svg" \
  "$icon_dir/io.github.josbeir.Carver.svg"

if command -v update-desktop-database >/dev/null; then
  update-desktop-database "$desktop_dir"
fi

echo "Installed Carver's development desktop assets in $data_home."
