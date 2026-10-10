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
mod image_zoom;
pub(crate) mod interactions;
mod library;
mod note_flow;
mod note_focus;
mod palette;
mod preferences;
mod printing;
mod properties;
mod rendering;
mod rich_mode;
mod screenshots;
mod shell;
mod source_input;
mod source_mode;
mod task_lists;
mod templates;
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
#[expect(
    clippy::too_many_lines,
    reason = "one display-backed entry point sequences scenarios that share GTK state"
)]
fn mvu_window_should_keep_sidebar_and_browser_card_presentation() -> TestResult {
    gtk::disable_portals();
    glib::set_application_name("Carver test");
    gtk::init()?;
    crate::app::load_styles();
    palette::palette_should_search_destinations_and_preserve_note_metadata()?;
    palette::palette_should_remove_destinations_when_the_library_changes()?;
    palette::palette_should_format_the_original_source_selection_and_restore_focus()?;
    palette::palette_should_preserve_rich_selection_and_keep_preview_read_only()?;
    palette::palette_should_follow_base_and_trash_context_and_keep_delete_confirmation()?;
    palette::palette_shortcut_should_work_and_suppress_nested_dialogs()?;
    palette::palette_icons_should_use_shared_glyphs_and_bundled_task_icon()?;
    palette::quote_should_toggle_from_the_palette_and_toolbar_in_both_editable_modes()?;
    palette::quote_should_unwrap_lazy_continuations_from_palette_and_toolbar()?;
    palette::palette_should_refresh_visible_commands_after_async_editor_replies()?;
    palette::palette_should_gate_base_mutations_while_definitions_reload()?;

    rendering::rendering_preference_should_refresh_previews_without_saving()?;
    rendering::code_fences_should_be_highlighted_in_previews_and_source()?;
    rendering::code_blocks_should_anchor_the_picker_and_keep_diff_lines_inline()?;
    excerpts::note_card_should_display_the_complete_final_grapheme()?;
    crate::mvu::export_runtime_should_cover_completion_cancellation_and_failures()?;
    crate::mvu::runtime_error_paths_should_surface_failures()?;
    interactions::cancelled_source_link_should_leave_the_document_unchanged()?;
    interactions::stale_web_messages_should_not_change_the_active_document()?;
    interactions::source_link_should_keep_the_captured_selection()?;
    interactions::rich_link_should_update_canonical_source()?;
    interactions::source_image_paste_should_store_a_managed_asset()?;
    interactions::source_smart_paste_should_preserve_markdown_delimiters()?;
    interactions::rich_changes_should_be_ignored_while_another_mode_is_active()?;
    source_input::enter_should_continue_and_exit_source_lists_with_native_undo()?;
    source_input::tab_should_move_source_subtrees_and_restore_selection()?;
    source_input::tab_should_move_nested_quotes_and_preserve_native_undo()?;
    source_input::source_input_should_preserve_native_keys_and_ime_composition()?;
    source_input::ghost_text_should_follow_bare_markers_without_editing_the_buffer()?;
    source_input::ghost_text_should_keep_its_source_position_font_and_read_only_load()?;
    source_input::source_list_edits_should_round_trip_and_persist_canonical_content()?;
    crate::ui::formatting::tests::image_description_should_import_only_after_confirmation()?;
    assert_pdf_page_setup()?;
    #[cfg(target_os = "linux")]
    assert_print_to_file_printer_resolves()?;
    shell::assert_sidebar_reload_preserves_rows()?;
    bases::assert_base_reload_preserves_buttons()?;
    bases::assert_base_loading_delay()?;
    bases::assert_base_note_keyboard_activation()?;
    crate::ui::editor::preview_service_should_receive_a_copy_and_support_portal_export()?;
    document_sidebar::webkit_views_should_disable_smooth_scrolling()?;
    document_sidebar::webkit_views_should_paint_the_document_background()?;
    document_sidebar::source_editor_should_start_on_the_dark_scheme_when_dark()?;
    document_sidebar::media_sidebar_should_show_file_details_in_an_isolated_editor()?;
    document_sidebar::media_download_should_save_a_managed_attachment_copy()?;
    properties::document_properties_button_should_follow_mode_and_setting()?;
    properties::default_properties_should_always_show_without_removal()?;
    properties::list_default_should_render_a_dropdown_when_single()?;
    properties::list_default_should_render_switches_when_multiple()?;
    properties::list_default_settings_should_offer_options_and_multiple()?;
    properties::date_default_should_render_a_picker_and_disable_invalid_values()?;
    properties::date_time_default_settings_should_persist_the_field_type()?;
    properties::ad_hoc_date_property_should_reopen_as_date()?;
    properties::ad_hoc_collapsed_date_picker_should_save_the_picked_value()?;
    properties::changing_a_property_type_should_keep_the_row_expanded()?;
    properties::date_picker_should_offer_clear_and_done_controls()?;
    properties::default_properties_dialog_should_persist_typed_entries()?;
    properties::date_time_default_should_edit_the_picker()?;
    properties::ad_hoc_date_time_picker_should_save_the_shown_now()?;
    properties::time_spinner_should_follow_a_twelve_hour_clock()?;
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
    document_sidebar::link_rows_should_render_markup();
    document_sidebar::outline_should_preserve_nonbreaking_spaces_in_heading_labels()?;
    document_sidebar::heading_navigation_should_preserve_content_and_focus()?;
    html::preview_and_copy_should_preserve_source_with_quoted_image_attributes()?;
    html::document_font_should_remain_css_text_inside_the_preview_head()?;
    image_zoom::rich_image_zoom_should_preserve_selection_source_and_saved_size()?;
    image_zoom::preview_image_zoom_should_close_on_backdrop_and_mode_switch()?;
    image_zoom::split_preview_image_zoom_should_close_when_split_is_hidden()?;
    image_zoom::image_zoom_should_dismiss_when_another_note_loads()?;
    crate::ui::formatting::tests::table_picker_should_reflect_live_table_and_reset();
    trash::trash_rows_should_keep_their_card_surface()?;
    trash::trash_contents_should_use_one_page_scroller()?;
    bases::base_header_sort_should_persist_from_native_controls()?;
    add::add_dialog_should_create_category_and_configure_new_base()?;
    add::add_dialog_should_balance_page_sizes()?;
    add::add_dialog_should_cancel_base_setup_when_closed_while_loading()?;
    bases::delete_base_should_require_confirmation_and_keep_notes()?;
    bases::base_search_should_open_and_clear_from_native_controls()?;
    bases::base_view_rows_should_select_one_mode_when_activated()?;
    bases::configure_base_should_keep_the_form_in_the_scroll_viewport()?;
    bases::base_field_picker_should_add_a_valid_custom_path()?;
    bases::base_rule_controls_should_edit_rules_and_fields()?;
    bases::base_cell_editors_should_commit_typed_values()?;
    bases::clicking_a_cell_should_reveal_the_editor()?;
    bases::base_cell_editor_should_reject_invalid_input_and_escape()?;
    bases::base_cell_editor_should_clear_a_value()?;
    bases::base_cell_editor_should_commit_on_click_away()?;
    bases::base_grid_should_keep_a_lossy_list_cell_read_only()?;
    bases::base_grid_edits_should_persist_to_the_note()?;
    bases::base_grid_should_clear_the_title_override()?;
    bases::base_grid_should_toggle_a_boolean_property()?;
    bases::base_grid_list_should_offer_a_dropdown()?;
    bases::base_properties_should_set_the_category()?;
    bases::base_grid_date_should_expose_a_picker_icon()?;
    bases::base_grid_date_picker_should_commit_on_close()?;
    bases::base_grid_date_picker_should_commit_an_unset_datetime()?;
    bases::base_grid_cleared_unset_datetime_should_not_commit()?;
    bases::base_grid_date_picker_clear_should_commit_immediately()?;
    bases::base_date_picker_should_release_widgets_after_column_rebuild()?;
    bases::base_list_view_should_render_note_cards()?;
    bases::base_list_should_rebind_when_category_colors_change()?;
    icons::bundled_icons_should_be_discoverable()?;
    crate::mvu::tests::runtime_should_render_and_complete_each_initial_resource()?;
    crate::mvu::tests::runtime_should_refresh_visible_resources_after_a_separate_client_mutates_the_library()?;
    crate::mvu::tests::runtime_should_edit_base_properties_without_leaving_the_base()?;
    crate::mvu::tests::runtime_should_refresh_sidebar_counts_after_an_editor_category_move()?;
    crate::ui::editor::source_commands::tests::gtk_source_commands_cover_selection_and_block_operations(
    );
    let fixture = window_fixture()?;
    dialogs::about_and_agent_setup_should_expose_shared_metadata(&fixture)?;
    dialogs::move_picker_rows_should_render_names_with_markup_characters();
    preferences::document_appearance_should_persist(&fixture)?;
    preferences::source_preferences_should_toggle_gutter_and_font(&fixture)?;
    preferences::document_properties_preferences_should_persist(&fixture)?;
    preferences::preferences_should_expose_searchable_pages(&fixture)?;
    shell::window_shell_should_expose_sidebar_and_base_presentation(&fixture)?;
    shell::window_shortcuts_should_open_dialogs(&fixture)?;
    let note = library::browser_actions_should_import_and_create_a_note(&fixture)?;
    shell::responsive_navigation_should_switch_sidebar_and_content(&fixture)?;
    shell::sidebar_should_use_adw_sidebar_sections(&fixture)?;
    shell::sidebar_count_badge_should_cap_large_counts()?;
    library::note_cards_should_group_and_favorite(&fixture, &note)?;
    library::move_picker_should_filter_and_move_notes(&fixture, &note)?;
    library::category_selection_should_show_empty_state(&fixture, &note)?;
    library::browser_hero_should_stay_fixed_above_the_feed(&fixture)?;
    library::category_hero_icon_should_use_a_light_glyph(&fixture)?;
    library::category_switch_should_retain_previous_browser(&fixture, &note)?;
    editor_shell::source_editor_should_configure_language_and_gutter(&fixture, &note)?;
    editor_shell::responsive_editor_should_switch_compact_and_desktop_toolbars(&fixture, &note)?;
    task_lists::task_button_should_toggle_source_lists_at_each_width(&fixture)?;
    task_lists::task_button_should_toggle_rich_lists_and_preserve_checked_items(&fixture)?;
    editor_shell::editor_options_should_adapt_to_layout(&fixture)?;
    rich_mode::rich_table_selection_should_update_the_picker(&fixture)?;
    export::export_dialogs_should_validate_and_print(&fixture, &note)?;
    #[cfg(target_os = "linux")]
    assert_native_print_dialog_should_print_to_file(
        &fixture.window.clone().upcast::<gtk::Window>(),
    )?;
    find::find_bar_and_shortcuts_should_navigate_matches(&fixture, &note)?;
    source_mode::formatting_controls_should_edit_carve(&fixture, &note)?;
    source_mode::highlighting_should_mark_carve_constructs(&fixture)?;
    source_mode::highlighting_should_mark_carve_blocks(&fixture)?;
    source_mode::rendered_preview_and_split_should_track_source(&fixture)?;
    rich_mode::rich_editor_should_round_trip_and_preserve_media(&fixture)?;
    rich_mode::short_rich_document_should_not_scroll_the_writing_surface(&fixture)?;
    library::browser_search_should_show_and_clear_empty_state(&fixture, &note)?;
    note_flow::tab_shortcuts_should_cycle_tabs(&fixture, &note)?;
    note_flow::note_should_delete_restore_and_favorite_from_shortcuts(&fixture, &note)?;
    note_flow::activating_a_background_tab_should_render_its_preview(&fixture)?;
    // No-op unless CARVER_SCREENSHOT_DIR is set; keeps one GTK entry point.
    screenshots::capture_docs_screenshots()?;
    templates::property_only_source_insertion_should_preserve_selection_persist_and_undo()?;
    templates::invalid_insertion_template_should_show_error_and_allow_another_choice()?;
    templates::template_save_conflict_should_preserve_draft_and_restore_controls()?;
    templates::saving_an_unopened_note_as_template_should_return_to_the_browser()?;
    templates::template_creation_should_report_missing_targets_without_creating_notes()?;
    templates::template_pattern_menu_should_insert_at_the_cursor_and_show_reference()?;
    templates::custom_template_format_should_preview_and_reject_invalid_formats()?;
    templates::category_templates_should_expand_patterns_at_note_creation()?;
    templates::inserting_template_should_preserve_properties_and_undo_in_source()?;
    templates::inserting_property_only_template_should_undo_in_rich_mode()?;
    templates::control_click_should_choose_template_without_creating_a_note()?;
    templates::template_shortcut_should_ignore_trash_and_base_contexts()?;
    templates::template_properties_should_scroll_inside_a_bounded_panel_when_many()?;
    templates::template_property_values_should_stay_on_one_line_at_all_dialog_widths()?;
    templates::template_empty_state_should_teach_and_open_a_first_draft()?;
    templates::templates_should_manage_validate_duplicate_and_delete()?;
    templates::category_template_should_seed_notes_and_allow_blank_override()?;
    templates::template_editor_should_confirm_discard()?;
    templates::saving_as_template_should_copy_unsaved_editor_source()?;
    note_focus::note_cards_should_keep_keyboard_focus_inside_the_card()?;
    Ok(())
}
