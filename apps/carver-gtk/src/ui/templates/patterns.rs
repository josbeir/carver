//! Discoverable template patterns, formatting examples, and a local reference.
use adw::prelude::*;
use gettextrs::gettext;
use gtk::prelude::*;
use libadwaita as adw;

pub(super) fn menu(parent: &adw::Dialog, buffer: &gtk::TextBuffer) -> gtk::MenuButton {
    let menu = gtk::MenuButton::builder()
        .label(gettext("Insert Pattern"))
        .build();
    menu.set_widget_name("template-insert-pattern");
    let actions = gtk::gio::SimpleActionGroup::new();
    let model = gtk::gio::Menu::new();
    let patterns = gtk::gio::Menu::new();
    for (label, token, name) in [
        (gettext("Current Date"), "{{date}}", "date"),
        (gettext("Current Time"), "{{time}}", "time"),
        (gettext("Current Date and Time"), "{{datetime}}", "datetime"),
        (gettext("Category Name"), "{{category}}", "category"),
    ] {
        let action = gtk::gio::SimpleAction::new(name, None);
        let buffer = buffer.clone();
        action.connect_activate(move |_, _| {
            buffer.begin_user_action();
            buffer.delete_selection(true, true);
            buffer.insert_at_cursor(token);
            buffer.end_user_action();
        });
        actions.add_action(&action);
        patterns.append(Some(&label), Some(&format!("pattern.{name}")));
    }
    model.append_section(None, &patterns);
    let tools = gtk::gio::Menu::new();
    tools.append(
        Some(&gettext("Custom Date/Time Format…")),
        Some("pattern.custom"),
    );
    let custom = gtk::gio::SimpleAction::new("custom", None);
    let weak = parent.downgrade();
    let buffer = buffer.clone();
    custom.connect_activate(move |_, _| {
        if let Some(parent) = weak.upgrade() {
            show_format(&parent, &buffer);
        }
    });
    actions.add_action(&custom);
    model.append_section(None, &tools);
    let help = gtk::gio::Menu::new();
    help.append(
        Some(&gettext("Pattern Reference")),
        Some("pattern.reference"),
    );
    let reference = gtk::gio::SimpleAction::new("reference", None);
    let weak = parent.downgrade();
    reference.connect_activate(move |_, _| {
        if let Some(parent) = weak.upgrade() {
            show_reference(&parent);
        }
    });
    actions.add_action(&reference);
    model.append_section(None, &help);
    menu.insert_action_group("pattern", Some(&actions));
    menu.set_menu_model(Some(&model));
    menu
}

fn shell(title: &str, name: &str) -> (adw::Dialog, gtk::Box, adw::HeaderBar) {
    let dialog = adw::Dialog::builder()
        .title(title)
        .content_width(480)
        .build();
    dialog.set_widget_name(name);
    let toolbar = adw::ToolbarView::new();
    let header = adw::HeaderBar::new();
    toolbar.add_top_bar(&header);
    let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
    super::set_margins(&content);
    let scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .child(&content)
        .build();
    toolbar.set_content(Some(&scroll));
    dialog.set_child(Some(&toolbar));
    (dialog, content, header)
}
fn show_reference(parent: &adw::Dialog) {
    let (dialog, content, _) = shell(
        &gettext("Pattern Reference"),
        "template-pattern-reference-dialog",
    );
    let group = adw::PreferencesGroup::new();
    for (token, description) in [
        ("{{date}}", gettext("Current local date: YYYY-MM-DD")),
        ("{{time}}", gettext("Current local time: HH:MM")),
        (
            "{{datetime}}",
            gettext("Current local date and time, including the UTC offset"),
        ),
        ("{{category}}", gettext("The destination category’s name")),
        (
            "{{date:%d/%m/%Y}}",
            gettext("Custom date format: day/month/year"),
        ),
        (
            "{{time:%H:%M:%S}}",
            gettext("Custom time format including seconds"),
        ),
    ] {
        let row = adw::ActionRow::builder()
            .title(token)
            .subtitle(&description)
            .build();
        row.set_use_markup(false);
        row.set_subtitle_lines(0);
        group.add(&row);
    }
    content.append(&group);
    content.append(&super::help_label(&gettext("Use patterns in note content or quoted property values. All patterns use one timestamp. The saved template stays unchanged.")));
    content.append(&super::help_label(&gettext("Formats use %Y (year), %m (month), %d (day), %H (hour), %M (minute), and %S (second). Use a backslash before {{ to keep a pattern literal; in YAML or JSON double-quoted values, escape the backslash too.")));
    dialog.present(Some(parent));
}
fn show_format(parent: &adw::Dialog, buffer: &gtk::TextBuffer) {
    let (dialog, content, header) = shell(
        &gettext("Custom Date/Time Format"),
        "template-pattern-format-dialog",
    );
    let group = adw::PreferencesGroup::new();
    let presets = adw::ComboRow::builder()
        .title(gettext("Format"))
        .model(&gtk::StringList::new(&[
            "%Y-%m-%d", "%d/%m/%Y", "%H:%M", "%H:%M:%S",
        ]))
        .build();
    let format = adw::EntryRow::builder()
        .title(gettext("Custom format"))
        .text("%Y-%m-%d")
        .build();
    format.set_widget_name("template-pattern-format-entry");
    group.add(&presets);
    group.add(&format);
    content.append(&group);
    let example = gtk::Label::builder().wrap(true).xalign(0.0).build();
    example.set_widget_name("template-pattern-format-example");
    content.append(&example);
    content.append(&super::help_label(&gettext(
        "%Y: year · %m: month · %d: day · %H: hour · %M: minute · %S: second",
    )));
    let insert = gtk::Button::with_label(&gettext("Insert"));
    insert.set_widget_name("template-pattern-format-insert");
    insert.add_css_class("suggested-action");
    header.pack_end(&insert);
    let sample = carver_sdk::template_context("");
    let update = {
        let example = example.clone();
        let insert = insert.clone();
        move |entry: &adw::EntryRow| {
            let token = format!("{{{{datetime:{}}}}}", entry.text());
            let result = if entry.text().contains("}}") || entry.text().contains("{{") {
                Err(carver_domain::TemplateError::Pattern(
                    entry.text().to_string(),
                ))
            } else {
                carver_domain::expand_template_source(&token, &sample)
            };
            if let Ok(value) = result {
                example.set_text(&value);
                example.remove_css_class("error");
                insert.set_sensitive(true);
            } else {
                example.set_text(&gettext("Invalid date/time format."));
                example.add_css_class("error");
                insert.set_sensitive(false);
            }
        }
    };
    update(&format);
    format.connect_changed(update);
    let entry = format.clone();
    presets.connect_selected_notify(move |row| {
        if let Some(value) = row.selected_item().and_downcast::<gtk::StringObject>() {
            entry.set_text(&value.string());
        }
    });
    let buffer = buffer.clone();
    let weak = dialog.downgrade();
    insert.connect_clicked(move |_| {
        buffer.begin_user_action();
        buffer.delete_selection(true, true);
        buffer.insert_at_cursor(&format!("{{{{datetime:{}}}}}", format.text()));
        buffer.end_user_action();
        if let Some(dialog) = weak.upgrade() {
            dialog.close();
        }
    });
    dialog.present(Some(parent));
}
