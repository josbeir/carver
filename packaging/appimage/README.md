# AppImage packaging

The release workflow builds Flatpak and AppImage x86_64 artifacts in parallel
on version tags and manual dispatch. A single publish job waits for both builds
and attaches both formats and their SHA-256 checksums to the tagged release.
Manual runs upload CI artifacts without publishing a release.

Build on Ubuntu 26.04, matching CI, with GTK 4.22, Libadwaita 1.9,
GtkSourceView 5 and WebKitGTK 6 development packages. Build the web editor and
both release binaries first, then run `bash packaging/appimage/build.sh` with
`LINUXDEPLOY`, `LDAI_RUNTIME_FILE` and `PATCHELF` pointing to the pinned tools
specified in the workflow. All downloads are verified with SHA-256.

We reuse linuxdeploy for ELF dependency discovery and AppImage creation. Its
GTK plugin targets GTK 2/3, so the small packaging script supplies GTK4 data,
WebKit subprocesses, GIO modules and GStreamer plugins explicitly.

These images use the build distribution's glibc baseline: Ubuntu 26.04 or a
compatible newer system is required. They are not a compatibility solution for
older distributions; use Flatpak there. Graphics drivers, fonts, a desktop
session and optional GNOME Sushi integration come from the host. GTK image
decoding also requires the host's `glycin-loaders` package and Bubblewrap:
Glycin's sandbox uses host libraries to execute its loaders. WebKit's sandbox
remains enabled. This is an experimental distribution format until the first
Ubuntu CI build and manual editor checks have passed.

Release WebKit ignores `WEBKIT_EXEC_PATH`. The launcher makes a private copy of
its library and relocates the compiled helper directory using a fixed-width
replacement. An absolute path is necessary for WebKit's process sandbox,
which remains enabled. The temporary library and helper symlink are removed
on normal exit; the mounted image is never modified. This costs roughly 90 MiB
of temporary space per GUI launch. The MCP-only launcher needs no copy.

The smoke test checks MCP launch and that the GUI survives startup under
Weston with the host WebKit helper directory hidden in a private mount
namespace. Local tests use Bubblewrap; CI creates the mount namespace with
`unshare` and drops privileges before launching the GUI, avoiding Ubuntu's
restrictions on nested Bubblewrap sandboxes without disabling AppArmor. It does not
establish compatibility with other distributions or verify
all editor interactions. Test those before advertising broader compatibility.

AppImages use the native XDG library, separate from a Flatpak installation.
Run the bundled server with `./carver-<version>-x86_64.AppImage --appimage-extract-and-run
--command=carver-mcp`. The write gate remains `--allow-write`.
