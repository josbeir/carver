//! Display-backed icon theme coverage for bundled and Adwaita icons.
use super::*;

pub(super) fn bundled_icons_should_be_discoverable() -> TestResult {
    let display = gtk::gdk::Display::default().ok_or("display")?;
    assert!(
        gtk::IconTheme::for_display(&display).has_icon("carver-agent-codex-symbolic"),
        "registered agent icons should be discoverable by GTK's icon theme"
    );
    assert!(
        gtk::IconTheme::for_display(&display).has_icon("carver-agent-opencode-symbolic"),
        "the bundled OpenCode agent icon should be discoverable by GTK's icon theme"
    );
    assert!(
        gtk::IconTheme::for_display(&display).has_icon("carver-database-symbolic"),
        "the bundled Lucide database icon should be available for saved Bases"
    );
    let database_icon = include_str!("../../../resources/icons/database.svg");
    assert!(database_icon.contains("fill=\"currentColor\""));
    assert!(!database_icon.contains("stroke="));
    assert!(
        gtk::IconTheme::for_display(&display).has_icon("package-x-generic-symbolic"),
        "the Adwaita package icon should be available to the category picker"
    );
    assert!(
        gtk::IconTheme::for_display(&display).has_icon("bookmark-new-symbolic"),
        "the Adwaita bookmark icon should be available to the category picker"
    );
    assert!(
        gtk::IconTheme::for_display(&display).has_icon("x-office-calendar-symbolic"),
        "the Adwaita calendar icon should be available to the category picker"
    );
    assert!(
        gtk::IconTheme::for_display(&display).has_icon("system-users-symbolic"),
        "the Adwaita people icon should be available to the category picker"
    );
    Ok(())
}
