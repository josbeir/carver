# Carver

Carver is a native GNOME note-taking application for the Carve markup language.

## Development

The application targets GTK 4.22 and Libadwaita 1.9 or newer.

```sh
cargo run -p carver-gtk
cargo test --workspace
# Full native GTK signal suite (requires Xvfb or an active desktop session)
xvfb-run -a cargo test --workspace -- --include-ignored --test-threads=1
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

Data follows the XDG base-directory convention:

- Configuration: `$XDG_CONFIG_HOME/carver/config.toml`
- Library: `$XDG_DATA_HOME/carver/library.sqlite3`
- Managed assets: `$XDG_DATA_HOME/carver/assets/`
