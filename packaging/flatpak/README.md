# Flatpak packaging

This directory contains Carver's reproducible, offline Flatpak build inputs.
The manifest deliberately builds the checked-out source tree so pull requests
and CI artifacts exercise the change being reviewed. It is not the manifest to
submit to Flathub: a Flathub release manifest must replace the local `dir`
source with a versioned, immutable upstream archive or commit.

The local source input explicitly excludes development outputs such as `target/`
and `node_modules/`; the package always rebuilds the editor and binaries from
the committed sources and lockfiles.

## Local build

Install Flatpak Builder and the matching runtime, SDK, and SDK extensions:

```sh
sudo pacman -S flatpak-builder
flatpak install --user flathub org.gnome.Platform//50 org.gnome.Sdk//50 \
  org.freedesktop.Sdk.Extension.rust-stable//25.08 \
  org.freedesktop.Sdk.Extension.node24//25.08
```

Build and install the checked-out tree:

```sh
flatpak-builder --user --install --install-deps-from=flathub --force-clean \
  --repo=repo build-dir packaging/flatpak/io.github.josbeir.Carver.json
flatpak run io.github.josbeir.Carver
```

To produce a shareable test bundle instead, omit `--install` and run:

```sh
flatpak build-bundle repo carver.flatpak io.github.josbeir.Carver \
  --runtime-repo=https://dl.flathub.org/repo/flathub.flatpakrepo
```

The Flatpak stores Carver's configuration, SQLite library, and managed assets
inside its per-application sandbox. A host-side MCP client can start the
bundled server with:

```sh
flatpak run --command=carver-mcp io.github.josbeir.Carver
```

## Locked dependency sources

`cargo-sources.json` and `node-sources.json` are generated from the committed
lockfiles. Regenerate both whenever `Cargo.lock` or the editor
`package-lock.json` changes:

```sh
./scripts/update-flatpak-sources.sh
```

The script requires the Cargo and Node generators from
[`flatpak-builder-tools`](https://github.com/flatpak/flatpak-builder-tools).
They are intentionally development-only tools; Flatpak CI consumes the
generated manifests and never downloads dependencies while compiling.
