#!/usr/bin/env bash
# Run on Ubuntu 26.04 after building both release binaries.
set -euo pipefail
project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$project_root"
linuxdeploy="${LINUXDEPLOY:?Set LINUXDEPLOY to the pinned linuxdeploy AppImage}"
: "${LDAI_RUNTIME_FILE:?Set LDAI_RUNTIME_FILE to the pinned AppImage runtime}"
: "${PATCHELF:?Set PATCHELF to the pinned modern ELF patcher}"
pkg-config --print-errors --exists webkitgtk-6.0 gstreamer-1.0
version="${CARVER_VERSION:-$(python3 -c 'import tomllib; print(tomllib.load(open("Cargo.toml", "rb"))["workspace"]["package"]["version"])')}"
build_dir="$(mktemp -d "$project_root/target/appimage.XXXXXX")"
app_dir="$build_dir/AppDir"
lib_dir="$(pkg-config --variable=libdir webkitgtk-6.0)"
mkdir -p "$app_dir/usr/lib" "$app_dir/usr/share"
for binary in carver-gtk carver-mcp; do
  install -Dm755 "target/release/$binary" "$app_dir/usr/bin/$binary"
done
resources=apps/carver-gtk/resources
install -Dm644 "$resources/io.github.josbeir.Carver.metainfo.xml" "$app_dir/usr/share/metainfo/io.github.josbeir.Carver.metainfo.xml"
install -Dm644 LICENSE "$app_dir/usr/share/licenses/carver/LICENSE"
# These resources are loaded dynamically and cannot be inferred from ELF links.
for directory in webkitgtk-6.0 gio/modules gstreamer-1.0; do
  mkdir -p "$app_dir/usr/lib/$(dirname "$directory")"
  cp -a "$lib_dir/$directory" "$app_dir/usr/lib/$directory"
done
scanner="$(pkg-config --variable=pluginscannerdir gstreamer-1.0)/gst-plugin-scanner"
install -Dm755 "$scanner" "$app_dir/usr/libexec/gstreamer-1.0/gst-plugin-scanner"
for directory in glib-2.0/schemas gtksourceview-5 icons/Adwaita icons/hicolor; do
  mkdir -p "$app_dir/usr/share/$(dirname "$directory")"
  cp -a "/usr/share/$directory" "$app_dir/usr/share/$directory"
done
glib-compile-schemas "$app_dir/usr/share/glib-2.0/schemas"
# Retain distribution copyright notices for bundled libraries and data.
if [[ -d /usr/share/doc ]]; then
  mkdir -p "$app_dir/usr/share/doc"
  while IFS= read -r -d '' copyright; do
    install -Dm644 "$copyright" "$app_dir$copyright"
  done < <(find /usr/share/doc -name copyright -type f -print0)
fi
# Avoid the bundled strip, which can lag behind modern ELF sections.
export NO_STRIP=1 APPIMAGE_EXTRACT_AND_RUN=1 ARCH=x86_64
export LDAI_OUTPUT="$project_root/target/carver-$version-x86_64.AppImage"
export LDAI_VERSION="$version"
"$linuxdeploy" --appdir "$app_dir" \
  --deploy-deps-only "$app_dir/usr" \
  --desktop-file "$resources/io.github.josbeir.Carver.desktop" \
  --icon-file "$resources/icons/hicolor/scalable/apps/io.github.josbeir.Carver.svg" \
  --custom-apprun packaging/appimage/AppRun --output appimage
cd target
sha256sum "carver-$version-x86_64.AppImage" > "carver-$version-x86_64.AppImage.sha256"
