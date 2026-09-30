//! Native template dialogs. Callbacks translate input into MVU messages.
use super::editor::source::{SourceEditor, SourceSyntaxError};
use crate::mvu::{AppDispatcher, AppMsg, RequestId, TemplatePurpose, TemplatesMsg, UiError};
use adw::prelude::*;
use carver_sdk::NoteTemplate;
use gettextrs::gettext;
use gtk::prelude::*;
use libadwaita as adw;
use std::path::Path;

pub(crate) struct EditorHandle {
    pub(crate) id: RequestId,
    dialog: adw::Dialog,
    error: gtk::Label,
    save: gtk::Button,
    editor: SourceEditor,
    entry: adw::EntryRow,
    cancel: gtk::Button,
}
impl EditorHandle {
    pub(crate) fn finish(&self, error: Option<&UiError>) {
        if let Some(error) = error {
            self.error.set_text(&error.message);
            self.error.set_visible(true);
            self.save.set_sensitive(true);
            self.entry.set_sensitive(true);
            self.cancel.set_sensitive(true);
            self.editor.view().set_editable(true);
        } else {
            self.dialog.force_close();
        }
    }
}

// CONTEXT: The native list assembly keeps row activation and menu bindings beside their widgets.
#[expect(
    clippy::too_many_lines,
    reason = "native template list construction and signal bindings"
)]
pub(crate) fn show_list(
    parent: &gtk::Window,
    dispatcher: &AppDispatcher,
    templates: &[NoteTemplate],
    purpose: &TemplatePurpose,
) -> adw::Dialog {
    let picking = matches!(purpose, TemplatePurpose::Pick(_));
    let dialog = adw::Dialog::builder()
        .title(if picking {
            gettext("New from Template")
        } else {
            gettext("Templates")
        })
        .content_width(480)
        .content_height(500)
        .build();
    dialog.set_widget_name("templates-dialog");
    let toolbar = adw::ToolbarView::new();
    let header = adw::HeaderBar::new();
    let new = gtk::Button::from_icon_name("list-add-symbolic");
    new.set_tooltip_text(Some(&gettext("New Template")));
    new.set_widget_name("new-template-button");
    header.pack_end(&new);
    let weak = dialog.downgrade();
    let d = dispatcher.clone();
    new.connect_clicked(move |_| {
        if let Some(dialog) = weak.upgrade() {
            dialog.close();
        }
        let _ = d.dispatch(AppMsg::Templates(TemplatesMsg::Edit(None)));
    });
    toolbar.add_top_bar(&header);
    let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
    content.set_margin_top(18);
    content.set_margin_bottom(18);
    content.set_margin_start(18);
    content.set_margin_end(18);
    let search = gtk::SearchEntry::new();
    search.set_widget_name("templates-search");
    search.set_placeholder_text(Some(&gettext("Search templates…")));
    content.append(&search);
    let group = adw::PreferencesGroup::new();
    let mut rows = Vec::new();
    for template in templates {
        let row = adw::ActionRow::builder()
            .title(&template.name)
            .activatable(true)
            .build();
        row.set_use_markup(false);
        row.set_widget_name(&format!("template-row-{}", template.id));
        let weak = dialog.downgrade();
        let d = dispatcher.clone();
        let template_for_open = template.clone();
        let purpose = purpose.clone();
        row.connect_activated(move |_| {
            if let Some(dialog) = weak.upgrade() {
                dialog.close();
            }
            let message = match &purpose {
                TemplatePurpose::Pick(category_id) => TemplatesMsg::Create {
                    category_id: *category_id,
                    template_id: template_for_open.id,
                },
                _ => TemplatesMsg::Edit(Some(template_for_open.clone())),
            };
            let _ = d.dispatch(AppMsg::Templates(message));
        });
        if !picking {
            row.add_suffix(&template_menu(parent, &dialog, dispatcher, template));
        }
        group.add(&row);
        rows.push((template.name.to_lowercase(), row));
    }
    content.append(&group);
    let empty = adw::StatusPage::builder()
        .title(gettext("No Templates"))
        .description(gettext(
            "Create a template to reuse its properties and content in new notes.",
        ))
        .icon_name("document-new-symbolic")
        .build();
    empty.set_visible(templates.is_empty());
    content.append(&empty);
    search.connect_search_changed(move |entry| {
        let query = entry.text().to_lowercase();
        let mut any = false;
        for (name, row) in &rows {
            let visible = name.contains(&query);
            row.set_visible(visible);
            any |= visible;
        }
        empty.set_visible(!any);
    });
    if picking {
        let manage = gtk::Button::with_label(&gettext("Manage Templates…"));
        let d = dispatcher.clone();
        let weak = dialog.downgrade();
        manage.connect_clicked(move |_| {
            if let Some(dialog) = weak.upgrade() {
                dialog.close();
            }
            let _ = d.dispatch(AppMsg::Templates(TemplatesMsg::Manage));
        });
        content.append(&manage);
    }
    let scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .child(&content)
        .vexpand(true)
        .build();
    toolbar.set_content(Some(&scroll));
    dialog.set_child(Some(&toolbar));
    dialog.present(Some(parent));
    dialog
}
fn template_menu(
    parent: &gtk::Window,
    dialog: &adw::Dialog,
    dispatcher: &AppDispatcher,
    template: &NoteTemplate,
) -> gtk::MenuButton {
    let button = gtk::MenuButton::builder()
        .icon_name("view-more-symbolic")
        .valign(gtk::Align::Center)
        .tooltip_text(gettext("Template actions"))
        .build();
    button.set_widget_name("template-actions");
    button.add_css_class("flat");
    let menu = gtk::gio::Menu::new();
    menu.append(Some(&gettext("Duplicate")), Some("template.duplicate"));
    menu.append(Some(&gettext("Delete")), Some("template.delete"));
    button.set_menu_model(Some(&menu));
    let actions = gtk::gio::SimpleActionGroup::new();
    let duplicate = gtk::gio::SimpleAction::new("duplicate", None);
    let d = dispatcher.clone();
    let t = template.clone();
    let weak = dialog.downgrade();
    duplicate.connect_activate(move |_, _| {
        if let Some(dialog) = weak.upgrade() {
            dialog.close();
        }
        let _ = d.dispatch(AppMsg::Templates(TemplatesMsg::Duplicate(t.clone())));
    });
    actions.add_action(&duplicate);
    let delete = gtk::gio::SimpleAction::new("delete", None);
    let d = dispatcher.clone();
    let id = template.id;
    let revision = template.revision;
    let parent = parent.downgrade();
    delete.connect_activate(move |_, _| {
        let confirmation = adw::AlertDialog::builder().heading(gettext("Delete Template?")).body(gettext("Categories using this template will return to blank notes. Existing notes will not change.")).close_response("cancel").default_response("cancel").build();
        confirmation.add_responses(&[("cancel", &gettext("Cancel")), ("delete", &gettext("Delete"))]);
        confirmation.set_response_appearance("delete", adw::ResponseAppearance::Destructive);
        let d = d.clone();
        confirmation.connect_response(Some("delete"), move |_, _| { let _ = d.dispatch(AppMsg::Templates(TemplatesMsg::Delete { id, revision })); });
        confirmation.present(parent.upgrade().as_ref());
    });
    actions.add_action(&delete);
    button.insert_action_group("template", Some(&actions));
    button
}

pub(crate) struct EditorDraft<'a> {
    pub(crate) id: RequestId,
    pub(crate) original: Option<NoteTemplate>,
    pub(crate) name: &'a str,
    pub(crate) source: &'a str,
}

// CONTEXT: Source-only dialog signals bind directly to their widgets and immutable draft snapshot.
#[expect(
    clippy::too_many_lines,
    reason = "source-only template dialog construction and input bindings"
)]
pub(crate) fn show_editor(
    parent: &gtk::Window,
    dispatcher: &AppDispatcher,
    draft: EditorDraft<'_>,
    syntax: &Path,
    preferences: &crate::mvu::SourceEditorPreferences,
) -> Result<EditorHandle, SourceSyntaxError> {
    let EditorDraft {
        id,
        original,
        name,
        source,
    } = draft;
    let editor = SourceEditor::new(syntax, adw::StyleManager::default().is_dark())?;
    editor.render_preferences(preferences, adw::StyleManager::default().is_dark());
    editor.buffer().set_text(source);
    let dialog = adw::Dialog::builder()
        .title(if original.is_some() {
            gettext("Edit Template")
        } else {
            gettext("New Template")
        })
        .content_width(760)
        .content_height(600)
        .build();
    dialog.set_widget_name("template-editor-dialog");
    let toolbar = adw::ToolbarView::new();
    let header = adw::HeaderBar::new();
    header.set_show_start_title_buttons(false);
    header.set_show_end_title_buttons(false);
    let cancel = gtk::Button::with_label(&gettext("Cancel"));
    let save = gtk::Button::with_label(&gettext("Save"));
    save.set_widget_name("template-save-button");
    save.add_css_class("suggested-action");
    save.set_sensitive(false);
    header.pack_start(&cancel);
    header.pack_end(&save);
    toolbar.add_top_bar(&header);
    let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
    content.set_margin_top(18);
    content.set_margin_bottom(18);
    content.set_margin_start(18);
    content.set_margin_end(18);
    let group = adw::PreferencesGroup::new();
    let entry = adw::EntryRow::builder()
        .title(gettext("Template name"))
        .text(name)
        .build();
    entry.set_widget_name("template-name-entry");
    group.add(&entry);
    content.append(&group);
    let label = gtk::Label::builder()
        .label(gettext("Carve source"))
        .xalign(0.0)
        .build();
    label.add_css_class("heading");
    content.append(&label);
    editor.view().set_widget_name("template-source-view");
    let scroll = gtk::ScrolledWindow::builder()
        .child(editor.view())
        .vexpand(true)
        .hexpand(true)
        .build();
    scroll.add_css_class("card");
    content.append(&scroll);
    let error = gtk::Label::builder()
        .wrap(true)
        .xalign(0.0)
        .visible(false)
        .build();
    error.set_widget_name("template-error");
    error.add_css_class("error");
    content.append(&error);
    let help = gtk::Label::builder().label(gettext("Template properties override matching defaults. Other enabled default properties are added when creating a note.")).wrap(true).xalign(0.0).build();
    help.add_css_class("dim-label");
    content.append(&help);
    toolbar.set_content(Some(&content));
    dialog.set_child(Some(&toolbar));
    let new_draft = original.is_none();
    let original_name = name.to_owned();
    let original_source = source.to_owned();
    let buffer = editor.buffer().clone();
    let update = {
        let weak = dialog.downgrade();
        let save = save.clone();
        let entry = entry.clone();
        let buffer = buffer.clone();
        move || {
            let source = buffer.text(&buffer.start_iter(), &buffer.end_iter(), true);
            let dirty = if new_draft {
                !entry.text().is_empty() || !source.is_empty()
            } else {
                entry.text().as_str() != original_name || source.as_str() != original_source
            };
            save.set_sensitive(!entry.text().trim().is_empty() && dirty);
            if let Some(dialog) = weak.upgrade() {
                dialog.set_can_close(!dirty);
            }
        }
    };
    update();
    let update = std::rc::Rc::new(update);
    let changed = std::rc::Rc::clone(&update);
    entry.connect_changed(move |_| changed());
    buffer.connect_changed(move |_| update());
    let weak = dialog.downgrade();
    cancel.connect_clicked(move |_| {
        if let Some(dialog) = weak.upgrade() {
            dialog.close();
        }
    });
    dialog.connect_close_attempt(move |dialog| {
        let alert = adw::AlertDialog::builder()
            .heading(gettext("Discard Template Changes?"))
            .body(gettext("Your unsaved template changes will be lost."))
            .close_response("cancel")
            .default_response("cancel")
            .build();
        alert.add_responses(&[
            ("cancel", &gettext("Cancel")),
            ("discard", &gettext("Discard")),
        ]);
        alert.set_response_appearance("discard", adw::ResponseAppearance::Destructive);
        let weak = dialog.downgrade();
        alert.connect_response(Some("discard"), move |_, _| {
            if let Some(dialog) = weak.upgrade() {
                dialog.force_close();
            }
        });
        alert.present(Some(dialog));
    });
    let d = dispatcher.clone();
    let error_for_save = error.clone();
    let entry_for_save = entry.clone();
    let cancel_for_save = cancel.clone();
    let view_for_save = editor.view().clone();
    save.connect_clicked(move |button| {
        button.set_sensitive(false);
        entry_for_save.set_sensitive(false);
        cancel_for_save.set_sensitive(false);
        view_for_save.set_editable(false);
        error_for_save.set_visible(false);
        let source = buffer
            .text(&buffer.start_iter(), &buffer.end_iter(), true)
            .to_string();
        let _ = d.dispatch(AppMsg::Templates(TemplatesMsg::Save {
            request_id: id,
            original: original.clone(),
            name: entry_for_save.text().to_string(),
            source,
        }));
    });
    let d = dispatcher.clone();
    dialog.connect_closed(move |_| {
        let _ = d.dispatch(AppMsg::Templates(TemplatesMsg::EditorClosed(id)));
    });
    dialog.present(Some(parent));
    Ok(EditorHandle {
        id,
        dialog,
        error,
        save,
        editor,
        entry,
        cancel,
    })
}
