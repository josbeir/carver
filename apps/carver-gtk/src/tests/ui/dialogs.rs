//! Display-backed about, agent setup, and settings metadata coverage.
use super::*;

pub(super) fn about_and_agent_setup_should_expose_shared_metadata(
    fixture: &WindowFixture,
) -> TestResult {
    let window = fixture.window.clone();
    let about_dialog = fixture.about_dialog.clone();
    let preferences_dialog = fixture.preferences_dialog.clone();
    assert_eq!(
        about_dialog.application_icon(),
        crate::app::APPLICATION_ICON
    );
    assert_eq!(
        about_dialog.issue_url(),
        "https://github.com/josbeir/carver/issues"
    );
    assert_eq!(
        about_link_uri(about_dialog.upcast_ref(), "Carve cheat sheet").as_deref(),
        Some("https://markup-carve.github.io/carve/cheatsheet")
    );
    assert!(window.lookup_action("connect-agent").is_some());
    assert!(window.lookup_action("toggle-favorite").is_some());
    let agent_setup = crate::ui::dialogs::show_agent_setup_dialog_for_test(&window);
    let agent = widget_as::<adw::ComboRow>(agent_setup.upcast_ref(), "agent-setup-agent")
        .ok_or("agent setup selection")?;
    let allow_agent_write =
        widget_as::<adw::SwitchRow>(agent_setup.upcast_ref(), "agent-setup-allow-write")
            .ok_or("agent write switch")?;
    let agent_command =
        widget_as::<adw::ActionRow>(agent_setup.upcast_ref(), "agent-setup-command")
            .ok_or("agent setup command")?;
    let claude_card = widget_as::<adw::ActionRow>(agent_setup.upcast_ref(), "agent-card-1")
        .ok_or("Claude Code agent card")?;
    claude_card.emit_by_name::<()>("activated", &[]);
    assert_eq!(agent.selected(), 1);
    assert!(
        agent_command
            .subtitle()
            .is_some_and(|command| command.contains("claude mcp add"))
    );
    allow_agent_write.set_active(true);
    assert!(
        agent_command
            .subtitle()
            .is_some_and(|command| command.contains("--allow-write"))
    );
    let opencode_card = widget_as::<adw::ActionRow>(agent_setup.upcast_ref(), "agent-card-4")
        .ok_or("OpenCode agent card")?;
    opencode_card.emit_by_name::<()>("activated", &[]);
    assert_eq!(agent.selected(), 4);
    assert!(
        agent_command
            .subtitle()
            .is_some_and(|command| command.contains("opencode mcp add carver --global --"))
    );
    agent_setup.close();
    assert_eq!(
        widget_as::<adw::SwitchRow>(preferences_dialog.upcast_ref(), "remote-images-setting")
            .map(|row| row.subtitle()),
        Some(Some(
            "Download images referenced by notes when they are displayed.".into()
        ))
    );
    Ok(())
}

/// Returns the URI of the About dialog link row titled `title`, if present.
fn about_link_uri(root: &gtk::Widget, title: &str) -> Option<String> {
    if let Ok(row) = root.clone().downcast::<adw::PreferencesRow>()
        && row.title().as_str() == title
    {
        let uri = row.property::<String>("uri");
        if !uri.is_empty() {
            return Some(uri);
        }
    }
    let mut child = root.first_child();
    while let Some(widget) = child {
        if let Some(uri) = about_link_uri(&widget, title) {
            return Some(uri);
        }
        child = widget.next_sibling();
    }
    None
}

/// Category names are user data, so `&` must render literally instead of as Pango markup.
pub(super) fn move_picker_rows_should_render_names_with_markup_characters() {
    let row = crate::ui::dialogs::move_category_row_for_test("Work & Play");
    assert!(
        find_label(row.upcast_ref::<gtk::Widget>(), "Work & Play").is_some(),
        "a category name containing '&' must render literally"
    );
}
