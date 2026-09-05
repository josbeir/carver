//! GNOME dialogs and window-scoped actions.

use std::rc::Rc;

use adw::prelude::*;
use carver_agent_integration::{AgentClient, InstallChannel, setup_instruction};
use carver_config::SourceSyntaxStyle;
use carver_sdk::{
    CategoryAppearance, CategoryColor, CategoryIcon, CategoryId, CategorySummary,
    DocumentImportFormat, NoteId,
};
use gtk::prelude::*;
use libadwaita as adw;

use crate::mvu::{
    ActionMsg, AppDispatcher, AppMsg, AppRuntime, EditorMsg, NavigationMsg, PreferencesMsg, Route,
    TrashMsg,
};
use crate::{editor::normalize_source_font_description, editor::system_monospace_font_description};
use carver_storage_sqlite::SqliteLibrary;

pub(crate) const NEW_NOTE_ACTION: &str = "win.new-note";
pub(crate) const IMPORT_NOTE_ACTION: &str = "win.import-note";
pub(crate) const EXPORT_NOTE_ACTION: &str = "win.export-note";
pub(crate) const PRINT_NOTE_ACTION: &str = "win.print-note";
pub(crate) const TRASH_NOTE_ACTION: &str = "win.trash-note";
pub(crate) const KEYBOARD_SHORTCUTS_ACTION: &str = "win.keyboard-shortcuts";

#[derive(Clone, Copy)]
struct Shortcut {
    title: &'static str,
    accelerator: &'static str,
}

#[derive(Clone, Copy)]
struct ShortcutSection {
    title: &'static str,
    shortcuts: &'static [Shortcut],
}

const GENERAL_SHORTCUTS: &[Shortcut] = &[Shortcut {
    title: "Keyboard shortcuts",
    accelerator: "<Control>question",
}];

const NOTES_SHORTCUTS: &[Shortcut] = &[
    Shortcut {
        title: "New note",
        accelerator: "<Control>n",
    },
    Shortcut {
        title: "Import note",
        accelerator: "<Control>o",
    },
];

const EDITOR_SHORTCUTS: &[Shortcut] = &[
    Shortcut {
        title: "Export note",
        accelerator: "<Control>e",
    },
    Shortcut {
        title: "Print note",
        accelerator: "<Control>p",
    },
    Shortcut {
        title: "Move note to Trash",
        accelerator: "<Control>d",
    },
];

const FIND_SHORTCUTS: &[Shortcut] = &[
    Shortcut {
        title: "Find in note",
        accelerator: "<Control>f",
    },
    Shortcut {
        title: "Next result",
        accelerator: "<Control>g",
    },
    Shortcut {
        title: "Next result",
        accelerator: "F3",
    },
    Shortcut {
        title: "Previous result",
        accelerator: "<Control><Shift>g",
    },
    Shortcut {
        title: "Previous result",
        accelerator: "<Shift>F3",
    },
    Shortcut {
        title: "Close search",
        accelerator: "Escape",
    },
];

const SOURCE_FORMATTING_SHORTCUTS: &[Shortcut] = &[
    Shortcut {
        title: "Bold",
        accelerator: "<Control>b",
    },
    Shortcut {
        title: "Italic",
        accelerator: "<Control>i",
    },
    Shortcut {
        title: "Strikethrough",
        accelerator: "<Control><Shift>x",
    },
    Shortcut {
        title: "Underline",
        accelerator: "<Control>u",
    },
    Shortcut {
        title: "Highlight",
        accelerator: "<Control><Shift>h",
    },
    Shortcut {
        title: "Superscript",
        accelerator: "<Control><Shift>period",
    },
    Shortcut {
        title: "Subscript",
        accelerator: "<Control><Shift>comma",
    },
    Shortcut {
        title: "Bulleted list",
        accelerator: "<Control><Shift>8",
    },
    Shortcut {
        title: "Numbered list",
        accelerator: "<Control><Shift>7",
    },
    Shortcut {
        title: "Insert line break",
        accelerator: "<Shift>Return",
    },
];

const SHORTCUT_SECTIONS: &[ShortcutSection] = &[
    ShortcutSection {
        title: "General",
        shortcuts: GENERAL_SHORTCUTS,
    },
    ShortcutSection {
        title: "Notes",
        shortcuts: NOTES_SHORTCUTS,
    },
    ShortcutSection {
        title: "Editor",
        shortcuts: EDITOR_SHORTCUTS,
    },
    ShortcutSection {
        title: "Find in note",
        shortcuts: FIND_SHORTCUTS,
    },
    ShortcutSection {
        title: "Source formatting",
        shortcuts: SOURCE_FORMATTING_SHORTCUTS,
    },
];

/// Installs actions exposed from the application menu.
pub(crate) fn install_window_actions(
    window: &adw::ApplicationWindow,
    dispatcher: &AppDispatcher,
    runtime: &AppRuntime<SqliteLibrary>,
) {
    let preferences = gtk::gio::SimpleAction::new("preferences", None);
    let dispatcher_for_preferences = dispatcher.clone();
    let runtime_for_preferences = runtime.clone();
    let window_for_preferences = window.clone();
    preferences.connect_activate(move |_, _| {
        let config = runtime_for_preferences.model().config;
        let _ = show_preferences_dialog(
            &window_for_preferences,
            &dispatcher_for_preferences,
            &config,
        );
    });
    window.add_action(&preferences);

    let connect_agent = gtk::gio::SimpleAction::new("connect-agent", None);
    let window_for_agent_setup = window.clone();
    connect_agent.connect_activate(move |_, _| {
        let _ = show_agent_setup_dialog(&window_for_agent_setup);
    });
    window.add_action(&connect_agent);

    let about = gtk::gio::SimpleAction::new("about", None);
    let window_for_about = window.clone();
    about.connect_activate(move |_, _| {
        let _ = show_about_window(&window_for_about);
    });
    window.add_action(&about);

    let keyboard_shortcuts = gtk::gio::SimpleAction::new("keyboard-shortcuts", None);
    let window_for_shortcuts = window.clone();
    keyboard_shortcuts.connect_activate(move |_, _| {
        let _ = show_keyboard_shortcuts_dialog(&window_for_shortcuts);
    });
    window.add_action(&keyboard_shortcuts);

    install_note_actions(window, dispatcher, runtime);
    install_application_accelerators(window);
    install_window_shortcuts(window);
    install_trash_actions(window, dispatcher);
    install_mvu_actions(window, dispatcher, runtime);
}

fn install_note_actions(
    window: &adw::ApplicationWindow,
    dispatcher: &AppDispatcher,
    runtime: &AppRuntime<SqliteLibrary>,
) {
    let new_note = gtk::gio::SimpleAction::new("new-note", None);
    let dispatcher_for_new_note = dispatcher.clone();
    let runtime_for_new_note = runtime.clone();
    new_note.connect_activate(move |_, _| {
        if runtime_for_new_note.model().route == Route::Browser {
            let _ = dispatcher_for_new_note.dispatch(AppMsg::Navigation(NavigationMsg::CreateNote));
        }
    });
    window.add_action(&new_note);

    let import_note = gtk::gio::SimpleAction::new("import-note", None);
    let dispatcher_for_import = dispatcher.clone();
    let runtime_for_import = runtime.clone();
    let window_for_import = window.clone();
    import_note.connect_activate(move |_, _| {
        if runtime_for_import.model().route == Route::Browser {
            show_import_file_dialog(
                window_for_import.upcast_ref::<gtk::Window>(),
                dispatcher_for_import.clone(),
            );
        }
    });
    window.add_action(&import_note);

    let export_note = gtk::gio::SimpleAction::new("export-note", None);
    let dispatcher_for_export = dispatcher.clone();
    let runtime_for_export = runtime.clone();
    export_note.connect_activate(move |_, _| {
        if runtime_for_export.model().editor.is_some() {
            let _ =
                dispatcher_for_export.dispatch(AppMsg::Editor(EditorMsg::ExportDialogRequested));
        }
    });
    window.add_action(&export_note);

    let print_note = gtk::gio::SimpleAction::new("print-note", None);
    let dispatcher_for_print = dispatcher.clone();
    let runtime_for_print = runtime.clone();
    print_note.connect_activate(move |_, _| {
        if runtime_for_print.model().editor.is_some() {
            let _ = dispatcher_for_print.dispatch(AppMsg::Editor(EditorMsg::PrintRequested));
        }
    });
    window.add_action(&print_note);

    let trash_note = gtk::gio::SimpleAction::new("trash-note", None);
    let dispatcher_for_trash = dispatcher.clone();
    let runtime_for_trash = runtime.clone();
    trash_note.connect_activate(move |_, _| {
        if runtime_for_trash.model().editor.is_some() {
            let _ = dispatcher_for_trash.dispatch(AppMsg::Editor(EditorMsg::TrashRequested));
        }
    });
    window.add_action(&trash_note);
}

fn install_application_accelerators(window: &adw::ApplicationWindow) {
    let Some(application) = window.application() else {
        return;
    };
    for (action, accelerator) in [
        (NEW_NOTE_ACTION, "<Control>n"),
        (IMPORT_NOTE_ACTION, "<Control>o"),
        (EXPORT_NOTE_ACTION, "<Control>e"),
        (PRINT_NOTE_ACTION, "<Control>p"),
        (TRASH_NOTE_ACTION, "<Control>d"),
        (KEYBOARD_SHORTCUTS_ACTION, "<Control>question"),
    ] {
        application.set_accels_for_action(action, &[accelerator]);
    }
}

pub(crate) fn show_import_file_dialog(parent: &gtk::Window, dispatcher: AppDispatcher) {
    let filter = gtk::FileFilter::new();
    filter.set_name(Some("Carve and Markdown documents"));
    for suffix in ["crv", "md", "markdown"] {
        filter.add_suffix(suffix);
    }
    let filters = gtk::gio::ListStore::new::<gtk::FileFilter>();
    filters.append(&filter);
    let dialog = gtk::FileDialog::builder()
        .title("Import note")
        .accept_label("Import")
        .build();
    dialog.set_filters(Some(&filters));
    dialog.set_default_filter(Some(&filter));
    dialog.open(
        Some(parent),
        None::<&gtk::gio::Cancellable>,
        move |result| {
            let Ok(file) = result else {
                return;
            };
            read_import_file(&file, dispatcher.clone());
        },
    );
}

pub(crate) fn read_import_file(file: &gtk::gio::File, dispatcher: AppDispatcher) {
    let Some(format) = import_format_for_file(file) else {
        let _ = dispatcher.dispatch(AppMsg::Navigation(NavigationMsg::ImportFailed(
            String::from("Select a Carve (.crv) or Markdown (.md) file."),
        )));
        return;
    };
    file.load_bytes_async(None::<&gtk::gio::Cancellable>, move |result| {
        let message = match result {
            Ok((bytes, _)) => import_message_from_bytes(format, bytes.as_ref()),
            Err(_) => {
                NavigationMsg::ImportFailed(String::from("Could not read the selected file."))
            }
        };
        let _ = dispatcher.dispatch(AppMsg::Navigation(message));
    });
}

pub(crate) fn import_message_from_bytes(
    format: DocumentImportFormat,
    bytes: &[u8],
) -> NavigationMsg {
    match String::from_utf8(bytes.to_vec()) {
        Ok(source) => NavigationMsg::ImportNote { format, source },
        Err(_) => {
            NavigationMsg::ImportFailed(String::from("The selected file is not valid UTF-8 text."))
        }
    }
}

pub(crate) fn import_format_for_file(file: &gtk::gio::File) -> Option<DocumentImportFormat> {
    let extension = file
        .basename()?
        .extension()?
        .to_string_lossy()
        .to_ascii_lowercase();
    match extension.as_str() {
        "crv" => Some(DocumentImportFormat::Carve),
        "md" | "markdown" => Some(DocumentImportFormat::Markdown),
        _ => None,
    }
}

fn install_window_shortcuts(window: &adw::ApplicationWindow) {
    let controller = gtk::EventControllerKey::new();
    controller.set_name(Some("window-shortcuts"));
    controller.set_propagation_phase(gtk::PropagationPhase::Capture);
    let action_host = window.clone().upcast::<gtk::Widget>();
    let action_host_for_callback = action_host.clone();
    controller.connect_key_pressed(move |_, key, _, modifiers| {
        if key != gtk::gdk::Key::question
            || !modifiers.contains(gtk::gdk::ModifierType::CONTROL_MASK)
        {
            return glib::Propagation::Proceed;
        }
        let _ = action_host_for_callback
            .activate_action(KEYBOARD_SHORTCUTS_ACTION, None::<&glib::Variant>);
        glib::Propagation::Stop
    });
    action_host.add_controller(controller);
}

fn install_mvu_actions(
    window: &impl IsA<gtk::Widget>,
    dispatcher: &AppDispatcher,
    runtime: &AppRuntime<SqliteLibrary>,
) {
    let actions = gtk::gio::SimpleActionGroup::new();
    let undo_move = gtk::gio::SimpleAction::new("undo-move", None);
    let dispatcher_for_undo = dispatcher.clone();
    undo_move.connect_activate(move |_, _| {
        let _ = dispatcher_for_undo.dispatch(AppMsg::Action(ActionMsg::UndoMove));
    });
    actions.add_action(&undo_move);
    let undo_trash_note = gtk::gio::SimpleAction::new("undo-trash-note", None);
    let dispatcher_for_trash_undo = dispatcher.clone();
    let runtime_for_trash_undo = runtime.clone();
    undo_trash_note.connect_activate(move |_, _| {
        let Some(note_id) = runtime_for_trash_undo.model().undo_trash_note else {
            return;
        };
        let _ = dispatcher_for_trash_undo.dispatch(AppMsg::Trash(TrashMsg::RestoreNote(note_id)));
    });
    actions.add_action(&undo_trash_note);
    let retry_save = gtk::gio::SimpleAction::new("retry-save", None);
    let dispatcher_for_retry = dispatcher.clone();
    retry_save.connect_activate(move |_, _| {
        let _ = dispatcher_for_retry.dispatch(AppMsg::Editor(EditorMsg::RetrySave));
    });
    actions.add_action(&retry_save);
    window.insert_action_group("mvu", Some(&actions));
}

fn install_trash_actions(window: &impl IsA<gtk::Widget>, dispatcher: &AppDispatcher) {
    let actions = gtk::gio::SimpleActionGroup::new();
    let restore_category =
        gtk::gio::SimpleAction::new("restore-category", Some(&String::static_variant_type()));
    let dispatcher_for_category = dispatcher.clone();
    restore_category.connect_activate(move |_, parameter| {
        let Some(category_id) = parameter
            .and_then(gtk::glib::Variant::get::<String>)
            .and_then(|value| uuid::Uuid::parse_str(&value).ok())
            .map(CategoryId::from_uuid)
        else {
            return;
        };
        let _ =
            dispatcher_for_category.dispatch(AppMsg::Trash(TrashMsg::RestoreCategory(category_id)));
    });
    actions.add_action(&restore_category);

    let restore_note =
        gtk::gio::SimpleAction::new("restore-note", Some(&String::static_variant_type()));
    let dispatcher_for_note = dispatcher.clone();
    restore_note.connect_activate(move |_, parameter| {
        let Some(note_id) = parameter
            .and_then(gtk::glib::Variant::get::<String>)
            .and_then(|value| uuid::Uuid::parse_str(&value).ok())
            .map(NoteId::from_uuid)
        else {
            return;
        };
        let _ = dispatcher_for_note.dispatch(AppMsg::Trash(TrashMsg::RestoreNote(note_id)));
    });
    actions.add_action(&restore_note);
    window.insert_action_group("trash", Some(&actions));
}

fn show_preferences_dialog(
    parent: &adw::ApplicationWindow,
    dispatcher: &AppDispatcher,
    config: &carver_config::Config,
) -> adw::PreferencesDialog {
    let dialog = adw::PreferencesDialog::new();
    dialog.set_search_enabled(false);
    let page = adw::PreferencesPage::new();
    page.set_title("Editor");
    let group = adw::PreferencesGroup::new();
    group.set_title("Editing");

    let remote_images = adw::SwitchRow::new();
    remote_images.set_widget_name("remote-images-setting");
    remote_images.set_title("Load remote images automatically");
    remote_images.set_subtitle("Download images referenced by notes when they are displayed.");
    remote_images.set_active(config.images.load_remote_automatically);
    group.add(&remote_images);
    let formatting_toolbar = preference_switch_row(
        "formatting-toolbar-setting",
        "Show formatting toolbar",
        "Show formatting controls at the bottom of the editor.",
        config.editor.show_formatting_toolbar,
    );
    group.add(&formatting_toolbar);

    let source_group = source_editor_preferences_group(parent, dispatcher, config);
    page.add(&group);
    page.add(&source_group);
    dialog.add(&page);

    let dispatcher_for_images = dispatcher.clone();
    remote_images.connect_active_notify(move |remote_images| {
        let _ = dispatcher_for_images.dispatch(AppMsg::Preferences(
            PreferencesMsg::SetRemoteImages(remote_images.is_active()),
        ));
    });
    let dispatcher_for_toolbar = dispatcher.clone();
    formatting_toolbar.connect_active_notify(move |formatting_toolbar| {
        let _ = dispatcher_for_toolbar.dispatch(AppMsg::Preferences(
            PreferencesMsg::SetFormattingToolbarVisible(formatting_toolbar.is_active()),
        ));
    });
    dialog.present(Some(parent));
    dialog
}

fn show_agent_setup_dialog(parent: &adw::ApplicationWindow) -> adw::PreferencesDialog {
    let dialog = adw::PreferencesDialog::new();
    dialog.set_title("Agents & MCP");
    dialog.set_search_enabled(false);
    let page = adw::PreferencesPage::new();
    let group = adw::PreferencesGroup::new();
    group.set_title("MCP setup");

    let agent = adw::ComboRow::new();
    agent.set_widget_name("agent-setup-agent");
    agent.set_title("Agent");
    agent.set_model(Some(&gtk::StringList::new(&[
        "Codex",
        "Claude Code",
        "GitHub Copilot CLI",
        "VS Code Copilot",
        "Generic / Other",
    ])));
    group.add(&agent);

    let cards = agent_cards_group(&agent);

    let allow_write = adw::SwitchRow::new();
    allow_write.set_widget_name("agent-setup-allow-write");
    allow_write.set_title("Allow note changes");
    allow_write.set_subtitle("Enable reversible create, save, move, trash, and restore actions.");
    group.add(&allow_write);

    let command = adw::ActionRow::new();
    command.set_widget_name("agent-setup-command");
    command.set_title("Setup command");
    command.set_subtitle_selectable(true);
    let copy = gtk::Button::from_icon_name("edit-copy-symbolic");
    copy.set_tooltip_text(Some("Copy setup configuration"));
    copy.add_css_class("flat");
    copy.add_css_class("circular");
    copy.set_valign(gtk::Align::Center);
    command.add_suffix(&copy);
    group.add(&command);
    page.add(&cards);
    page.add(&group);
    dialog.add(&page);

    let update: Rc<dyn Fn()> = Rc::new({
        let command = command.clone();
        let agent = agent.clone();
        let allow_write = allow_write.clone();
        move || {
            let client = match agent.selected() {
                0 => AgentClient::Codex,
                1 => AgentClient::ClaudeCode,
                2 => AgentClient::CopilotCli,
                3 => AgentClient::VsCodeCopilot,
                _ => AgentClient::Generic,
            };
            let text =
                match setup_instruction(client, &InstallChannel::detect(), allow_write.is_active())
                {
                    Ok(instruction) => match instruction.command.or(instruction.configuration) {
                        Some(text) => text,
                        None => "No setup instruction is available.".to_owned(),
                    },
                    Err(error) => error.to_string(),
                };
            command.set_subtitle(&text);
        }
    });
    let command_for_copy = command.clone();
    copy.connect_clicked(move |_| {
        if let (Some(display), Some(text)) =
            (gtk::gdk::Display::default(), command_for_copy.subtitle())
        {
            display.clipboard().set_text(&text);
        }
    });
    update();
    let update_for_agent = update.clone();
    agent.connect_selected_notify(move |_| update_for_agent());
    allow_write.connect_active_notify(move |_| update());
    dialog.present(Some(parent));
    dialog
}

fn agent_cards_group(agent: &adw::ComboRow) -> adw::PreferencesGroup {
    let group = adw::PreferencesGroup::new();
    group.set_title("Choose your agent");
    group.set_description(Some(
        "Select a client to reveal its private, user-level MCP setup.",
    ));
    for (index, title, subtitle, icon) in [
        (0, "Codex", "OpenAI's coding agent", "codex-openai"),
        (1, "Claude Code", "Anthropic's terminal agent", "anthropic"),
        (
            2,
            "GitHub Copilot CLI",
            "GitHub's terminal assistant",
            "copilot",
        ),
        (
            3,
            "VS Code Copilot",
            "Configure VS Code's MCP server list",
            "copilot",
        ),
        (
            4,
            "Generic / Other",
            "Any client that supports stdio MCP",
            "applications-system-symbolic",
        ),
    ] {
        let card = adw::ActionRow::new();
        card.set_widget_name(&format!("agent-card-{index}"));
        card.set_title(title);
        card.set_subtitle(subtitle);
        card.set_activatable(true);
        card.add_prefix(&agent_icon(icon));
        let agent_for_card = agent.clone();
        card.connect_activated(move |_| agent_for_card.set_selected(index));
        group.add(&card);
    }
    group
}

fn agent_icon(icon: &str) -> gtk::Image {
    let icon_name = match icon {
        "codex-openai" => "carver-agent-codex-symbolic",
        "anthropic" => "carver-agent-claude-symbolic",
        "copilot" => "carver-agent-copilot-symbolic",
        _ => icon,
    };
    let image = gtk::Image::from_icon_name(icon_name);
    image.set_pixel_size(24);
    image
}

fn source_editor_preferences_group(
    parent: &adw::ApplicationWindow,
    dispatcher: &AppDispatcher,
    config: &carver_config::Config,
) -> adw::PreferencesGroup {
    let group = adw::PreferencesGroup::new();
    group.set_title("Source editor");
    let (source_font, font_value, reset_font) = source_font_rows(config);
    group.add(&source_font);
    group.add(&reset_font);
    let line_numbers = preference_switch_row(
        "source-line-numbers-setting",
        "Show line numbers",
        "Show source line positions in the editor gutter.",
        config.editor.source_line_numbers,
    );
    group.add(&line_numbers);
    let current_line = preference_switch_row(
        "source-current-line-setting",
        "Highlight current line",
        "Shade the line containing the cursor in Source mode.",
        config.editor.source_highlight_current_line,
    );
    group.add(&current_line);
    let syntax_style = source_syntax_style_row(config.editor.source_syntax_style);
    group.add(&syntax_style);
    connect_source_font_controls(parent, dispatcher, &source_font, &font_value, &reset_font);
    connect_source_switches(dispatcher, &line_numbers, &current_line, &syntax_style);
    group
}

fn source_font_rows(
    config: &carver_config::Config,
) -> (adw::ActionRow, gtk::Label, adw::ActionRow) {
    let row = adw::ActionRow::new();
    row.set_widget_name("source-font-setting");
    row.set_title("Source font");
    row.set_activatable(true);
    let system_font = system_monospace_font_description();
    let selected_font = config
        .editor
        .source_font
        .as_deref()
        .and_then(normalize_source_font_description)
        .unwrap_or_else(|| system_font.clone());
    let value = gtk::Label::new(Some(&selected_font));
    value.set_widget_name("source-font-value");
    value.add_css_class("dim-label");
    value.set_ellipsize(gtk::pango::EllipsizeMode::End);
    row.add_suffix(&value);
    let reset = adw::ActionRow::new();
    reset.set_widget_name("source-font-reset-row");
    reset.set_title("Use system monospace font");
    reset.set_subtitle(&format!("Follow the desktop setting ({system_font})."));
    reset.set_activatable(true);
    reset.set_visible(config.editor.source_font.is_some());
    (row, value, reset)
}

fn source_font_dialog() -> gtk::FontDialog {
    let font_filter = gtk::CustomFilter::new(|item| {
        item.downcast_ref::<gtk::pango::FontFamily>()
            .is_some_and(gtk::pango::prelude::FontFamilyExt::is_monospace)
    });
    let dialog = gtk::FontDialog::new();
    dialog.set_title("Choose Source Font");
    dialog.set_filter(Some(&font_filter));
    dialog
}

fn preference_switch_row(
    widget_name: &str,
    title: &str,
    subtitle: &str,
    active: bool,
) -> adw::SwitchRow {
    let row = adw::SwitchRow::new();
    row.set_widget_name(widget_name);
    row.set_title(title);
    row.set_subtitle(subtitle);
    row.set_active(active);
    row
}

fn source_syntax_style_row(style: SourceSyntaxStyle) -> adw::ComboRow {
    let options = gtk::StringList::new(&["Detailed", "Writing focus", "Off"]);
    let expression = gtk::PropertyExpression::new(
        gtk::StringObject::static_type(),
        None::<gtk::Expression>,
        "string",
    );
    let row = adw::ComboRow::new();
    row.set_widget_name("source-syntax-style-setting");
    row.set_title("Syntax style");
    row.set_subtitle("Choose how much markup colour appears in Source mode.");
    row.set_model(Some(&options));
    row.set_expression(Some(&expression));
    row.set_selected(source_syntax_style_index(style));
    row
}

const fn source_syntax_style_index(style: SourceSyntaxStyle) -> u32 {
    match style {
        SourceSyntaxStyle::Detailed => 0,
        SourceSyntaxStyle::WritingFocus => 1,
        SourceSyntaxStyle::None => 2,
    }
}

const fn source_syntax_style_from_index(index: u32) -> SourceSyntaxStyle {
    match index {
        1 => SourceSyntaxStyle::WritingFocus,
        2 => SourceSyntaxStyle::None,
        _ => SourceSyntaxStyle::Detailed,
    }
}

fn connect_source_font_controls(
    parent: &adw::ApplicationWindow,
    dispatcher: &AppDispatcher,
    source_font: &adw::ActionRow,
    font_value: &gtk::Label,
    reset_font: &adw::ActionRow,
) {
    let dispatcher_for_font = dispatcher.clone();
    let parent = parent.clone();
    let font_value_for_change = font_value.clone();
    let reset_font_for_change = reset_font.clone();
    source_font.connect_activated(move |_| {
        let dialog = source_font_dialog();
        let initial_font = gtk::pango::FontDescription::from_string(&font_value_for_change.label());
        let dispatcher_for_result = dispatcher_for_font.clone();
        let font_value_for_result = font_value_for_change.clone();
        let reset_font_for_result = reset_font_for_change.clone();
        dialog.choose_font(
            Some(&parent),
            Some(&initial_font),
            None::<&gtk::gio::Cancellable>,
            move |result| {
                let Ok(font) = result else {
                    return;
                };
                let Some(font) = normalize_source_font_description(&font.to_string()) else {
                    return;
                };
                font_value_for_result.set_label(&font);
                reset_font_for_result.set_visible(true);
                let _ = dispatcher_for_result.dispatch(AppMsg::Preferences(
                    PreferencesMsg::SetSourceFont(Some(font)),
                ));
            },
        );
    });
    let dispatcher_for_font_reset = dispatcher.clone();
    let font_value_for_reset = font_value.clone();
    let reset_font_for_reset = reset_font.clone();
    reset_font.connect_activated(move |_| {
        let system_font = system_monospace_font_description();
        font_value_for_reset.set_label(&system_font);
        reset_font_for_reset.set_visible(false);
        let _ = dispatcher_for_font_reset
            .dispatch(AppMsg::Preferences(PreferencesMsg::SetSourceFont(None)));
    });
}

fn connect_source_switches(
    dispatcher: &AppDispatcher,
    line_numbers: &adw::SwitchRow,
    current_line: &adw::SwitchRow,
    syntax_style: &adw::ComboRow,
) {
    let dispatcher_for_line_numbers = dispatcher.clone();
    line_numbers.connect_active_notify(move |line_numbers| {
        let _ = dispatcher_for_line_numbers.dispatch(AppMsg::Preferences(
            PreferencesMsg::SetSourceLineNumbers(line_numbers.is_active()),
        ));
    });
    let dispatcher_for_current_line = dispatcher.clone();
    current_line.connect_active_notify(move |current_line| {
        let _ = dispatcher_for_current_line.dispatch(AppMsg::Preferences(
            PreferencesMsg::SetSourceHighlightCurrentLine(current_line.is_active()),
        ));
    });
    let dispatcher_for_syntax_style = dispatcher.clone();
    syntax_style.connect_selected_notify(move |syntax_style| {
        let _ = dispatcher_for_syntax_style.dispatch(AppMsg::Preferences(
            PreferencesMsg::SetSourceSyntaxStyle(source_syntax_style_from_index(
                syntax_style.selected(),
            )),
        ));
    });
}

#[cfg(test)]
pub(crate) fn present_dialogs_for_test(
    parent: &adw::ApplicationWindow,
    config: &carver_config::Config,
    dispatcher: &AppDispatcher,
) -> (adw::PreferencesDialog, adw::AboutDialog) {
    let preferences = show_preferences_dialog(parent, dispatcher, config);
    let about = show_about_window(parent);
    show_category_name_dialog(Some(parent.upcast_ref()), "New Category", "", |_| {});
    show_category_dialog(
        Some(parent.upcast_ref()),
        "New Category",
        "",
        CategoryAppearance::default(),
        |_, _| {},
    );
    show_category_trash_confirmation(Some(parent.upcast_ref()), "Category", || {});
    (preferences, about)
}

#[cfg(test)]
pub(crate) fn show_agent_setup_dialog_for_test(
    parent: &adw::ApplicationWindow,
) -> adw::PreferencesDialog {
    show_agent_setup_dialog(parent)
}

fn show_about_window(parent: &adw::ApplicationWindow) -> adw::AboutDialog {
    let about = adw::AboutDialog::builder()
        .application_name("Carver")
        .application_icon(crate::app::APPLICATION_ICON)
        .version(env!("CARGO_PKG_VERSION"))
        .developer_name("Carver contributors")
        .comments("A native GNOME notebook for Carve markup.")
        .issue_url("https://github.com/josbeir/carver/issues")
        .license_type(gtk::License::MitX11)
        .build();
    about.present(Some(parent));
    about
}

/// Presents the searchable reference for every explicit Carver keyboard shortcut.
pub(crate) fn show_keyboard_shortcuts_dialog(
    parent: &adw::ApplicationWindow,
) -> adw::ShortcutsDialog {
    let dialog = adw::ShortcutsDialog::builder()
        .title("Keyboard Shortcuts")
        .build();
    dialog.set_widget_name("keyboard-shortcuts-dialog");
    for section in SHORTCUT_SECTIONS {
        let shortcuts = adw::ShortcutsSection::new(Some(section.title));
        for shortcut in section.shortcuts {
            shortcuts.add(adw::ShortcutsItem::new(
                shortcut.title,
                shortcut.accelerator,
            ));
        }
        dialog.add(shortcuts);
    }
    dialog.present(Some(parent));
    dialog
}

/// Presents a validated single-line category name dialog.
pub(crate) fn show_category_name_dialog(
    parent: Option<&gtk::Window>,
    title: &str,
    initial_name: &str,
    on_submit: impl Fn(String) + 'static,
) {
    let entry = gtk::Entry::new();
    entry.set_widget_name("category-name-entry");
    entry.set_text(initial_name);
    entry.set_placeholder_text(Some("Category name"));
    entry.set_activates_default(true);
    let dialog = adw::AlertDialog::builder()
        .heading(title)
        .extra_child(&entry)
        .default_response("save")
        .close_response("cancel")
        .build();
    dialog.add_responses(&[("cancel", "Cancel"), ("save", "Save")]);
    dialog.set_response_enabled("save", !initial_name.trim().is_empty());
    let dialog_for_entry = dialog.clone();
    entry.connect_changed(move |entry| {
        dialog_for_entry.set_response_enabled("save", !entry.text().trim().is_empty());
    });
    dialog.connect_response(None, move |_dialog, response| {
        if response == "save" {
            let name = entry.text().trim().to_owned();
            if !name.is_empty() {
                on_submit(name);
            }
        }
    });
    dialog.present(parent);
}

/// Presents one category form with a validated name and an explicit visual identity.
pub(crate) fn show_category_dialog(
    parent: Option<&gtk::Window>,
    title: &str,
    initial_name: &str,
    initial_appearance: CategoryAppearance,
    on_submit: impl Fn(String, CategoryAppearance) + 'static,
) {
    let content = gtk::Box::new(gtk::Orientation::Vertical, 14);
    content.set_widget_name("category-dialog-content");
    let entry = gtk::Entry::new();
    entry.set_widget_name("category-name-entry");
    entry.set_text(initial_name);
    entry.set_placeholder_text(Some("Category name"));
    entry.set_activates_default(true);
    content.append(&entry);
    let (appearance_picker, icon, color) = category_appearance_picker(initial_appearance);
    content.append(&appearance_picker);
    let dialog = adw::AlertDialog::builder()
        .heading(title)
        .extra_child(&content)
        .default_response("save")
        .close_response("cancel")
        .build();
    dialog.add_responses(&[("cancel", "Cancel"), ("save", "Save")]);
    dialog.set_response_enabled("save", !initial_name.trim().is_empty());
    let dialog_for_entry = dialog.clone();
    entry.connect_changed(move |entry| {
        dialog_for_entry.set_response_enabled("save", !entry.text().trim().is_empty());
    });
    dialog.connect_response(None, move |_dialog, response| {
        if response == "save" {
            let name = entry.text().trim().to_owned();
            if !name.is_empty() {
                on_submit(
                    name,
                    CategoryAppearance {
                        icon: icon.get(),
                        color: color.get(),
                    },
                );
            }
        }
    });
    dialog.present(parent);
}

fn category_appearance_picker(
    initial: CategoryAppearance,
) -> (
    gtk::Box,
    Rc<std::cell::Cell<CategoryIcon>>,
    Rc<std::cell::Cell<CategoryColor>>,
) {
    let picker = gtk::Box::new(gtk::Orientation::Vertical, 8);
    let icon_label = gtk::Label::new(Some("Icon"));
    icon_label.set_xalign(0.0);
    icon_label.add_css_class("heading");
    picker.append(&icon_label);
    let icons = gtk::FlowBox::new();
    icons.set_selection_mode(gtk::SelectionMode::None);
    icons.set_column_spacing(6);
    icons.set_row_spacing(6);
    let selected_icon = Rc::new(std::cell::Cell::new(initial.icon));
    let mut icon_group = None;
    for icon in CATEGORY_ICONS {
        let button = gtk::ToggleButton::new();
        button.set_widget_name(&format!("category-icon-{}", category_icon_id(icon)));
        button.set_tooltip_text(Some(category_icon_label(icon)));
        button.add_css_class("category-appearance-option");
        button.set_active(icon == initial.icon);
        button.set_child(Some(&gtk::Image::from_icon_name(category_icon_name(icon))));
        button.set_group(icon_group.as_ref());
        icon_group.get_or_insert_with(|| button.clone());
        let selected_icon = Rc::clone(&selected_icon);
        button.connect_toggled(move |button| {
            if button.is_active() {
                selected_icon.set(icon);
            }
        });
        icons.insert(&button, -1);
    }
    picker.append(&icons);
    let color_label = gtk::Label::new(Some("Colour"));
    color_label.set_xalign(0.0);
    color_label.add_css_class("heading");
    picker.append(&color_label);
    let colors = gtk::FlowBox::new();
    colors.set_selection_mode(gtk::SelectionMode::None);
    colors.set_column_spacing(6);
    colors.set_row_spacing(6);
    let selected_color = Rc::new(std::cell::Cell::new(initial.color));
    let mut color_group = None;
    for color in CATEGORY_COLORS {
        let button = gtk::ToggleButton::with_label(category_color_label(color));
        button.set_widget_name(&format!("category-color-{}", category_color_id(color)));
        button.set_tooltip_text(Some(category_color_label(color)));
        button.add_css_class("category-color-option");
        button.add_css_class(category_color_css_class(color));
        button.set_active(color == initial.color);
        button.set_group(color_group.as_ref());
        color_group.get_or_insert_with(|| button.clone());
        let selected_color = Rc::clone(&selected_color);
        button.connect_toggled(move |button| {
            if button.is_active() {
                selected_color.set(color);
            }
        });
        colors.insert(&button, -1);
    }
    picker.append(&colors);
    (picker, selected_icon, selected_color)
}

const CATEGORY_ICONS: [CategoryIcon; 10] = [
    CategoryIcon::Folder,
    CategoryIcon::Briefcase,
    CategoryIcon::Calendar,
    CategoryIcon::Book,
    CategoryIcon::Heart,
    CategoryIcon::Home,
    CategoryIcon::People,
    CategoryIcon::Star,
    CategoryIcon::Tag,
    CategoryIcon::Lightbulb,
];

const CATEGORY_COLORS: [CategoryColor; 8] = [
    CategoryColor::Auto,
    CategoryColor::Rose,
    CategoryColor::Tangerine,
    CategoryColor::Yellow,
    CategoryColor::Olive,
    CategoryColor::Teal,
    CategoryColor::Blue,
    CategoryColor::Purple,
];

pub(crate) fn category_icon_name(icon: CategoryIcon) -> &'static str {
    match icon {
        CategoryIcon::Folder => "folder-symbolic",
        CategoryIcon::Briefcase => "package-x-generic-symbolic",
        CategoryIcon::Calendar => "x-office-calendar-symbolic",
        CategoryIcon::Book => "x-office-document-symbolic",
        CategoryIcon::Heart => "emblem-favorite-symbolic",
        CategoryIcon::Home => "user-home-symbolic",
        CategoryIcon::People => "system-users-symbolic",
        CategoryIcon::Star => "starred-symbolic",
        CategoryIcon::Tag => "bookmark-new-symbolic",
        CategoryIcon::Lightbulb => "dialog-information-symbolic",
    }
}

fn category_icon_id(icon: CategoryIcon) -> &'static str {
    match icon {
        CategoryIcon::Folder => "folder",
        CategoryIcon::Briefcase => "briefcase",
        CategoryIcon::Calendar => "calendar",
        CategoryIcon::Book => "book",
        CategoryIcon::Heart => "heart",
        CategoryIcon::Home => "home",
        CategoryIcon::People => "people",
        CategoryIcon::Star => "star",
        CategoryIcon::Tag => "tag",
        CategoryIcon::Lightbulb => "lightbulb",
    }
}

fn category_icon_label(icon: CategoryIcon) -> &'static str {
    match icon {
        CategoryIcon::Folder => "Folder",
        CategoryIcon::Briefcase => "Briefcase",
        CategoryIcon::Calendar => "Calendar",
        CategoryIcon::Book => "Book",
        CategoryIcon::Heart => "Heart",
        CategoryIcon::Home => "Home",
        CategoryIcon::People => "People",
        CategoryIcon::Star => "Star",
        CategoryIcon::Tag => "Tag",
        CategoryIcon::Lightbulb => "Idea",
    }
}

fn category_color_id(color: CategoryColor) -> &'static str {
    match color {
        CategoryColor::Auto => "auto",
        CategoryColor::Rose => "rose",
        CategoryColor::Tangerine => "tangerine",
        CategoryColor::Yellow => "yellow",
        CategoryColor::Olive => "olive",
        CategoryColor::Teal => "teal",
        CategoryColor::Blue => "blue",
        CategoryColor::Purple => "purple",
    }
}

fn category_color_label(color: CategoryColor) -> &'static str {
    match color {
        CategoryColor::Auto => "Auto",
        CategoryColor::Rose => "Rose",
        CategoryColor::Tangerine => "Tangerine",
        CategoryColor::Yellow => "Yellow",
        CategoryColor::Olive => "Olive",
        CategoryColor::Teal => "Teal",
        CategoryColor::Blue => "Blue",
        CategoryColor::Purple => "Purple",
    }
}

pub(crate) fn category_color_css_class(color: CategoryColor) -> &'static str {
    match color {
        CategoryColor::Auto => "category-color-auto",
        CategoryColor::Rose => "category-color-rose",
        CategoryColor::Tangerine => "category-color-tangerine",
        CategoryColor::Yellow => "category-color-yellow",
        CategoryColor::Olive => "category-color-olive",
        CategoryColor::Teal => "category-color-teal",
        CategoryColor::Blue => "category-color-blue",
        CategoryColor::Purple => "category-color-purple",
    }
}

/// Presents a searchable category picker for moving one note.
///
/// The dialog retains only transient widget state. Selecting a destination or creating one
/// dispatches a typed action; the MVU runtime owns all persistence and subsequent reloads.
pub(crate) fn show_move_note_dialog(
    parent: Option<&gtk::Window>,
    dispatcher: &AppDispatcher,
    note_id: NoteId,
    source_category_id: CategoryId,
    note_title: &str,
    categories: &[CategorySummary],
) -> adw::Dialog {
    let dialog = adw::Dialog::builder()
        .title(format!("Move “{note_title}”"))
        .content_width(420)
        .content_height(460)
        .build();
    dialog.set_widget_name("move-note-dialog");
    let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
    content.set_margin_start(18);
    content.set_margin_end(18);
    content.set_margin_top(18);
    content.set_margin_bottom(18);
    let title = gtk::Label::new(Some(&format!("Move “{note_title}”")));
    title.add_css_class("title-3");
    title.set_xalign(0.0);
    content.append(&title);
    let search = gtk::SearchEntry::new();
    search.set_widget_name("move-note-search");
    search.set_placeholder_text(Some("Search categories"));
    content.append(&search);
    let list = gtk::ListBox::new();
    list.set_widget_name("move-note-category-list");
    list.set_selection_mode(gtk::SelectionMode::None);
    list.add_css_class("boxed-list");
    let scroll = gtk::ScrolledWindow::new();
    scroll.set_vexpand(true);
    scroll.set_child(Some(&list));
    content.append(&scroll);
    let new_category = gtk::Button::with_label("New Category…");
    new_category.set_widget_name("move-note-new-category-button");
    new_category.add_css_class("flat");
    new_category.set_halign(gtk::Align::Start);
    content.append(&new_category);
    let cancel = gtk::Button::with_label("Cancel");
    cancel.set_halign(gtk::Align::End);
    content.append(&cancel);
    dialog.set_child(Some(&content));

    let categories = std::rc::Rc::new(categories.to_vec());
    populate_move_categories(
        &list,
        &categories,
        "",
        dispatcher,
        note_id,
        source_category_id,
        &dialog,
    );
    connect_move_picker_search(
        &search,
        &list,
        &categories,
        dispatcher,
        note_id,
        source_category_id,
        &dialog,
    );
    connect_move_picker_new_category(
        &new_category,
        parent,
        dispatcher,
        note_id,
        source_category_id,
        &dialog,
    );
    let dialog_for_cancel = dialog.clone();
    cancel.connect_clicked(move |_| {
        dialog_for_cancel.close();
    });
    dialog.present(parent);
    dialog
}

fn connect_move_picker_search(
    search: &gtk::SearchEntry,
    list: &gtk::ListBox,
    categories: &std::rc::Rc<Vec<CategorySummary>>,
    dispatcher: &AppDispatcher,
    note_id: NoteId,
    source_category_id: CategoryId,
    dialog: &adw::Dialog,
) {
    let list = list.clone();
    let categories = std::rc::Rc::clone(categories);
    let dispatcher = dispatcher.clone();
    let dialog = dialog.clone();
    search.connect_search_changed(move |search| {
        populate_move_categories(
            &list,
            &categories,
            search.text().as_str(),
            &dispatcher,
            note_id,
            source_category_id,
            &dialog,
        );
    });
}

fn connect_move_picker_new_category(
    button: &gtk::Button,
    parent: Option<&gtk::Window>,
    dispatcher: &AppDispatcher,
    note_id: NoteId,
    source_category_id: CategoryId,
    dialog: &adw::Dialog,
) {
    let parent = parent.cloned();
    let dispatcher = dispatcher.clone();
    let dialog = dialog.clone();
    button.connect_clicked(move |_| {
        dialog.close();
        let dispatcher = dispatcher.clone();
        show_category_name_dialog(parent.as_ref(), "New Category", "", move |name| {
            let _ = dispatcher.dispatch(AppMsg::Action(ActionMsg::CreateCategoryAndMoveNote {
                name,
                note_id,
                source_category_id,
            }));
        });
    });
}

fn populate_move_categories(
    list: &gtk::ListBox,
    categories: &[CategorySummary],
    query: &str,
    dispatcher: &AppDispatcher,
    note_id: NoteId,
    source_category_id: CategoryId,
    dialog: &adw::Dialog,
) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }
    let query = query.trim().to_lowercase();
    let mut matching_categories = 0;
    for summary in categories {
        let category = &summary.category;
        if !query.is_empty() && !category.name.to_lowercase().contains(&query) {
            continue;
        }
        matching_categories += 1;
        let row = gtk::ListBoxRow::new();
        row.set_widget_name(&format!("move-note-category:{}", category.id));
        let button = gtk::Button::new();
        button.add_css_class("flat");
        button.set_hexpand(true);
        let content = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        content.set_margin_start(12);
        content.set_margin_end(12);
        content.set_margin_top(8);
        content.set_margin_bottom(8);
        content.append(&gtk::Image::from_icon_name("folder-symbolic"));
        let label = gtk::Label::new(Some(&category.name));
        label.set_xalign(0.0);
        label.set_hexpand(true);
        content.append(&label);
        if category.id == source_category_id {
            let current = gtk::Label::new(Some("Current"));
            current.add_css_class("dim-label");
            content.append(&current);
            button.set_sensitive(false);
        } else {
            let dispatcher = dispatcher.clone();
            let dialog = dialog.clone();
            let category_id = category.id;
            button.connect_clicked(move |_| {
                dialog.close();
                let _ = dispatcher.dispatch(AppMsg::Action(ActionMsg::MoveNote {
                    note_id,
                    source_category_id,
                    category_id,
                }));
            });
        }
        button.set_child(Some(&content));
        row.set_child(Some(&button));
        list.append(&row);
    }
    if matching_categories == 0 {
        let row = gtk::ListBoxRow::new();
        row.set_selectable(false);
        let label = gtk::Label::new(Some("No categories found"));
        label.add_css_class("dim-label");
        label.set_margin_top(12);
        label.set_margin_bottom(12);
        row.set_child(Some(&label));
        list.append(&row);
    }
}

/// Presents a destructive confirmation before a category is moved to Trash.
pub(crate) fn show_category_trash_confirmation(
    parent: Option<&gtk::Window>,
    category_name: &str,
    on_confirm: impl Fn() + 'static,
) {
    let dialog = category_trash_dialog(category_name, on_confirm);
    dialog.present(parent);
}

/// Builds the category-trash confirmation so its destructive behavior is testable.
pub(crate) fn category_trash_dialog(
    category_name: &str,
    on_confirm: impl Fn() + 'static,
) -> adw::AlertDialog {
    let dialog = adw::AlertDialog::new(
        Some(&format!("Move “{category_name}” to Trash?")),
        Some(
            "Notes in this category will no longer appear in your library. You can restore the category from Trash later.",
        ),
    );
    dialog.set_widget_name("category-trash-confirmation");
    dialog.add_responses(&[("cancel", "Cancel"), ("trash", "Move to Trash")]);
    dialog.set_response_appearance("trash", adw::ResponseAppearance::Destructive);
    dialog.set_default_response(Some("cancel"));
    dialog.set_close_response("cancel");
    dialog.connect_response(None, move |_dialog, response| {
        if response == "trash" {
            on_confirm();
        }
    });
    dialog
}
