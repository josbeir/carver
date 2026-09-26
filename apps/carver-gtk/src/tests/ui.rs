//! Display-backed interaction coverage for the MVU window surface.

mod add;
mod bases;
mod dialogs;
pub(crate) mod document_sidebar;
mod editor_shell;
mod excerpts;
mod export;
mod find;
mod html;
mod icons;
pub(crate) mod interactions;
mod library;
mod note_flow;
mod preferences;
mod printing;
mod properties;
mod rendering;
mod rich_mode;
mod shell;
mod source_mode;
mod trash;
mod window;

use std::{cell::Cell, rc::Rc, time::Duration};

use carver_config::{Config, SourceSyntaxStyle};
use gtk::gio::prelude::FileExt;
use gtk::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::{
    ActionRowExt, AdwDialogExt, BreakpointBinExt, ComboRowExt, PreferencesDialogExt,
    PreferencesPageExt, PreferencesRowExt, SidebarItemExt,
};
use sourceview5::prelude::*;
use webkit6::prelude::*;

use super::support::{
    TestResult, find_widget, run_main_context_until, run_main_context_until_for, test_state,
    widget_as,
};
use printing::*;
use source_mode::*;
use window::*;

#[test]
#[ignore = "requires a graphical display; CI runs it under headless Weston"]
fn mvu_window_should_keep_sidebar_and_browser_card_presentation() -> TestResult {
    gtk::disable_portals();
    glib::set_application_name("Carver test");
    gtk::init()?;
    rendering::rendering_preference_should_refresh_previews_without_saving()?;
    rendering::code_fences_should_be_highlighted_in_previews_and_source()?;
    rendering::code_blocks_should_anchor_the_picker_and_keep_diff_lines_inline()?;
    excerpts::note_card_should_display_the_complete_final_grapheme()?;
    crate::mvu::export_runtime_should_cover_completion_cancellation_and_failures()?;
    interactions::cancelled_source_link_should_leave_the_document_unchanged()?;
    interactions::stale_web_messages_should_not_change_the_active_document()?;
    interactions::source_link_should_keep_the_captured_selection()?;
    interactions::rich_link_should_update_canonical_source()?;
    interactions::source_image_paste_should_store_a_managed_asset()?;
    interactions::source_smart_paste_should_preserve_markdown_delimiters()?;
    interactions::rich_changes_should_be_ignored_while_another_mode_is_active()?;
    crate::ui::formatting::tests::image_description_should_import_only_after_confirmation()?;
    assert_pdf_page_setup()?;
    shell::assert_sidebar_reload_preserves_rows()?;
    bases::assert_base_reload_preserves_buttons()?;
    bases::assert_base_loading_delay()?;
    bases::assert_base_note_keyboard_activation()?;
    crate::ui::editor::preview_service_should_receive_a_copy_and_support_portal_export()?;
    document_sidebar::webkit_views_should_disable_smooth_scrolling()?;
    document_sidebar::media_sidebar_should_show_file_details_in_an_isolated_editor()?;
    properties::document_properties_button_should_follow_mode_and_setting()?;
    properties::default_properties_should_always_show_without_removal()?;
    properties::list_default_should_render_a_dropdown_when_single()?;
    properties::list_default_should_render_switches_when_multiple()?;
    properties::list_default_settings_should_offer_options_and_multiple()?;
    properties::date_default_should_render_a_picker_and_disable_invalid_values()?;
    properties::date_time_default_settings_should_persist_the_field_type()?;
    properties::ad_hoc_date_property_should_reopen_as_date()?;
    properties::changing_a_property_type_should_keep_the_row_expanded()?;
    properties::date_picker_should_offer_clear_and_done_controls()?;
    properties::default_properties_dialog_should_persist_typed_entries()?;
    properties::date_time_default_should_edit_the_picker()?;
    properties::typed_defaults_should_save_edited_values()?;
    properties::title_frontmatter_should_fill_the_title_row()?;
    properties::malformed_frontmatter_should_fall_back_to_raw_source()?;
    properties::defaults_dialog_should_remove_a_property()?;
    properties::defaults_dialog_should_handle_date_and_typed_values()?;
    properties::date_default_picker_should_edit_and_save()?;
    properties::ad_hoc_boolean_property_should_toggle_and_save()?;
    properties::authored_frontmatter_order_should_survive_an_unchanged_save()?;
    properties::explicit_empty_value_should_survive_an_unchanged_save()?;
    properties::complex_frontmatter_should_fall_back_to_raw_source()?;
    properties::heading_should_prefill_the_title_without_persisting()?;
    properties::edited_prefilled_title_should_persist()?;
    properties::edited_title_should_lead_the_block_on_reopen()?;
    document_sidebar::assert_document_sidebar_visibility_should_restore_without_reentrant_toggles(
    )?;
    document_sidebar::heading_navigation_should_preserve_content_and_focus()?;
    html::preview_and_copy_should_preserve_source_with_quoted_image_attributes()?;
    html::document_font_should_remain_css_text_inside_the_preview_head()?;
    crate::ui::formatting::tests::captured_source_selection_should_delete_marks_after_reading_offsets(
    );
    crate::app::load_styles();
    trash::trash_rows_should_keep_their_card_surface()?;
    trash::trash_contents_should_use_one_page_scroller()?;
    bases::base_header_sort_should_persist_from_native_controls()?;
    add::add_dialog_should_create_category_and_configure_new_base()?;
    add::add_dialog_should_resize_for_the_active_form()?;
    bases::delete_base_should_require_confirmation_and_keep_notes()?;
    bases::base_search_should_open_and_clear_from_native_controls()?;
    bases::configure_base_should_keep_the_form_in_the_scroll_viewport()?;
    bases::base_field_picker_should_add_a_valid_custom_path()?;
    icons::bundled_icons_should_be_discoverable()?;
    crate::mvu::tests::runtime_should_render_and_complete_each_initial_resource()?;
    crate::mvu::tests::runtime_should_refresh_visible_resources_after_a_separate_client_mutates_the_library()?;
    crate::ui::editor::source_commands::tests::gtk_source_commands_cover_selection_and_block_operations(
    );
    let fixture = window_fixture()?;
    dialogs::about_and_agent_setup_should_expose_shared_metadata(&fixture)?;
    preferences::document_appearance_should_persist(&fixture)?;
    preferences::source_preferences_should_toggle_gutter_and_font(&fixture)?;
    preferences::document_properties_preferences_should_persist(&fixture)?;
    preferences::preferences_should_expose_searchable_pages(&fixture)?;
    shell::window_shell_should_expose_sidebar_and_base_presentation(&fixture)?;
    shell::window_shortcuts_should_open_dialogs(&fixture)?;
    let note = library::browser_actions_should_import_and_create_a_note(&fixture)?;
    shell::responsive_navigation_should_switch_sidebar_and_content(&fixture)?;
    shell::sidebar_should_use_adw_sidebar_sections(&fixture)?;
    library::note_cards_should_group_and_favorite(&fixture, &note)?;
    library::move_picker_should_filter_and_move_notes(&fixture, &note)?;
    library::category_selection_should_show_empty_state(&fixture, &note)?;
    library::category_switch_should_retain_previous_browser(&fixture, &note)?;
    editor_shell::source_editor_should_configure_language_and_gutter(&fixture, &note)?;
    editor_shell::responsive_editor_should_switch_compact_and_desktop_toolbars(&fixture)?;
    editor_shell::editor_options_should_adapt_to_layout(&fixture)?;
    export::export_dialogs_should_validate_and_print(&fixture, &note)?;
    find::find_bar_and_shortcuts_should_navigate_matches(&fixture, &note)?;
    source_mode::formatting_controls_should_edit_carve(&fixture, &note)?;
    source_mode::highlighting_should_mark_carve_constructs(&fixture)?;
    source_mode::highlighting_should_mark_carve_blocks(&fixture)?;
    source_mode::rendered_preview_and_split_should_track_source(&fixture)?;
    rich_mode::rich_editor_should_round_trip_and_preserve_media(&fixture)?;
    library::browser_search_should_show_and_clear_empty_state(&fixture, &note)?;
    note_flow::note_should_delete_restore_and_favorite_from_shortcuts(&fixture, &note)?;
    Ok(())
}
