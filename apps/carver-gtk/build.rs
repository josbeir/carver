//! Compiles the application-owned symbolic icon resources.

fn main() {
    glib_build_tools::compile_resources(
        &["resources"],
        "resources/agent-icons.gresource.xml",
        "carver-agent-icons.gresource",
    );
}
