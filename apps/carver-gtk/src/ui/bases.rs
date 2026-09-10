//! Native database-style grid for saved note bases.
pub(crate) mod actions;

use carver_sdk::{BaseColumn, BaseDefinition, BaseRow};
use gtk::prelude::*;
use libadwaita as adw;

use crate::mvu::{AppDispatcher, AppMsg, NavigationMsg};
use crate::ui::sidebar::{CompactNavigation, back_to_notes_button, sidebar_toggle_button};

/// Widgets needed to render the current saved base.
pub(crate) struct BaseViewRefs {
    pub(crate) delete: gtk::Button,
    pub(crate) title: gtk::Label,
    pub(crate) grid: gtk::ColumnView,
    pub(crate) pages: gtk::Stack,
    pub(crate) scroll: gtk::ScrolledWindow,
    pub(crate) status: adw::StatusPage,
}

pub(crate) fn build_base(
    dispatcher: &AppDispatcher,
    split_view: &adw::NavigationSplitView,
    compact_navigation: &CompactNavigation,
) -> (gtk::Widget, BaseViewRefs) {
    let toolbar = adw::ToolbarView::new();
    let header = adw::HeaderBar::new();
    header.pack_start(&sidebar_toggle_button(
        split_view,
        compact_navigation,
        "base-toggle-categories-button",
    ));
    let back = back_to_notes_button(
        dispatcher,
        "back-to-notes-from-base-button",
        AppMsg::Navigation(NavigationMsg::ShowBrowser),
    );
    header.pack_start(&back);
    let title = gtk::Label::new(Some("Base"));
    title.set_widget_name("base-title");
    title.add_css_class("title");
    header.set_title_widget(Some(&title));
    let delete = gtk::Button::from_icon_name("user-trash-symbolic");
    delete.set_widget_name("delete-base-button");
    delete.set_tooltip_text(Some("Delete Base"));
    delete.set_action_name(Some("base.delete"));
    header.pack_end(&delete);
    for (label, icon) in [
        ("Columns", "view-list-symbolic"),
        ("Filter", "funnel-symbolic"),
        ("Sort", "view-sort-descending-symbolic"),
    ] {
        let button = gtk::Button::with_label(label);
        button.set_icon_name(icon);
        button.add_css_class("flat");
        button.set_sensitive(false);
        button.set_tooltip_text(Some("Coming in the next Bases iteration"));
        header.pack_end(&button);
    }
    toolbar.add_top_bar(&header);

    let model = gtk::StringList::new(&[]);
    let selection = gtk::NoSelection::new(Some(model));
    let grid = gtk::ColumnView::new(Some(selection));
    grid.set_widget_name("bases-grid");
    grid.add_css_class("data-table");
    grid.add_css_class("bases-grid");
    grid.set_show_row_separators(true);
    grid.set_show_column_separators(true);
    grid.set_hexpand(true);
    grid.set_vexpand(true);

    let scroll = gtk::ScrolledWindow::new();
    scroll.set_policy(gtk::PolicyType::Automatic, gtk::PolicyType::Automatic);
    scroll.set_child(Some(&grid));
    let status = adw::StatusPage::builder()
        .icon_name("view-grid-symbolic")
        .title("No rows")
        .description("Notes with YAML, JSON, or TOML frontmatter will appear here.")
        .build();
    status.set_widget_name("base-status");
    let pages = gtk::Stack::new();
    pages.set_widget_name("base-pages");
    pages.add_named(&scroll, Some("grid"));
    pages.add_named(&status, Some("status"));
    pages.set_visible_child_name("grid");
    toolbar.set_content(Some(&pages));
    (
        toolbar.upcast(),
        BaseViewRefs {
            delete,
            title,
            grid,
            pages,
            scroll,
            status,
        },
    )
}

pub(crate) fn render_base_status(refs: &BaseViewRefs, title: &str, description: &str) {
    refs.status.set_title(title);
    refs.status.set_description(Some(description));
    refs.pages.set_visible_child_name("status");
}

pub(crate) fn render_base(
    refs: &BaseViewRefs,
    definition: &BaseDefinition,
    rows: &[BaseRow],
    dispatcher: &AppDispatcher,
) {
    let horizontal = refs.scroll.hadjustment().value();
    let vertical = refs.scroll.vadjustment().value();
    refs.title.set_text(&definition.name);
    refs.grid.set_sensitive(true);
    while let Some(column) = refs
        .grid
        .columns()
        .item(0)
        .and_downcast::<gtk::ColumnViewColumn>()
    {
        refs.grid.remove_column(&column);
    }
    append_column(&refs.grid, "Name", None, Some(dispatcher));
    for column in &definition.columns {
        match column {
            BaseColumn::Name => {}
            BaseColumn::Category => {
                append_column(&refs.grid, "Category", Some("category"), None);
            }
            BaseColumn::Updated => {
                append_column(&refs.grid, "Updated", Some("updated"), None);
            }
            BaseColumn::Property(path) => append_column(
                &refs.grid,
                path.0.trim_start_matches('/'),
                Some(&path.0),
                None,
            ),
        }
    }
    let serialized: Vec<String> = rows
        .iter()
        .filter_map(|row| serde_json::to_string(row).ok())
        .collect();
    let borrowed: Vec<&str> = serialized.iter().map(String::as_str).collect();
    let model = gtk::StringList::new(&borrowed);
    refs.grid
        .set_model(Some(&gtk::NoSelection::new(Some(model))));
    refs.pages
        .set_visible_child_name(if rows.is_empty() { "status" } else { "grid" });
    let horizontal_adjustment = refs.scroll.hadjustment();
    let vertical_adjustment = refs.scroll.vadjustment();
    glib::idle_add_local_once(move || {
        horizontal_adjustment.set_value(horizontal);
        vertical_adjustment.set_value(vertical);
    });
}

fn append_column(
    grid: &gtk::ColumnView,
    title: &str,
    field: Option<&str>,
    dispatcher: Option<&AppDispatcher>,
) {
    let factory = gtk::SignalListItemFactory::new();
    let dispatcher_for_setup = dispatcher.cloned();
    factory.connect_setup(move |_, item| {
        let Some(item) = item.downcast_ref::<gtk::ListItem>() else {
            return;
        };
        let label = gtk::Label::new(None);
        label.set_xalign(0.0);
        label.set_margin_start(10);
        label.set_margin_end(10);
        label.set_margin_top(7);
        label.set_margin_bottom(7);
        label.set_ellipsize(gtk::pango::EllipsizeMode::End);
        if let Some(dispatcher) = &dispatcher_for_setup {
            label.add_css_class("link");
            let button = gtk::Button::new();
            button.add_css_class("flat");
            button.set_child(Some(&label));
            let dispatcher = dispatcher.clone();
            let weak_item = item.downgrade();
            button.connect_clicked(move |_| {
                let Some(item) = weak_item.upgrade() else {
                    return;
                };
                let Some(string) = item.item().and_downcast::<gtk::StringObject>() else {
                    return;
                };
                let Ok(row) = serde_json::from_str::<BaseRow>(&string.string()) else {
                    return;
                };
                let _ =
                    dispatcher.dispatch(AppMsg::Navigation(NavigationMsg::OpenNote(row.note_id)));
            });
            item.set_child(Some(&button));
        } else {
            item.set_child(Some(&label));
        }
    });
    let field_for_bind = field.map(ToOwned::to_owned);
    factory.connect_bind(move |_, item| {
        let Some(item) = item.downcast_ref::<gtk::ListItem>() else {
            return;
        };
        let Some(string) = item.item().and_downcast::<gtk::StringObject>() else {
            return;
        };
        let Some(child) = item.child() else {
            return;
        };
        let label = child.clone().downcast::<gtk::Label>().ok().or_else(|| {
            child
                .clone()
                .downcast::<gtk::Button>()
                .ok()?
                .child()?
                .downcast::<gtk::Label>()
                .ok()
        });
        let Some(label) = label else {
            return;
        };
        let Ok(row) = serde_json::from_str::<BaseRow>(&string.string()) else {
            return;
        };
        if child.is::<gtk::Button>() {
            child.set_widget_name(&format!("base-note:{}", row.note_id));
        }
        let value = row_value(&row, field_for_bind.as_deref());
        label.set_text(&value);
    });
    let column = gtk::ColumnViewColumn::new(Some(title), Some(factory));
    column.set_resizable(true);
    column.set_expand(field.is_none());
    grid.append_column(&column);
}

fn row_value(row: &BaseRow, field: Option<&str>) -> String {
    match field {
        None => row.name.clone(),
        Some("category") => row.category.clone(),
        Some("updated") => row.updated.clone(),
        Some(path) => row
            .properties
            .pointer(path)
            .map(display_value)
            .unwrap_or_default(),
    }
}

fn display_value(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Null => String::new(),
        serde_json::Value::String(value) => value.clone(),
        serde_json::Value::Array(values) => values
            .iter()
            .map(display_value)
            .collect::<Vec<_>>()
            .join(", "),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use carver_sdk::{NoteId, Revision};

    use super::*;

    fn row(properties: serde_json::Value) -> BaseRow {
        BaseRow {
            note_id: NoteId::new(),
            revision: Revision(3),
            name: "Roadmap".to_owned(),
            category: "Projects".to_owned(),
            updated: "2026-09-09T12:00:00Z".to_owned(),
            properties,
        }
    }

    #[test]
    fn row_value_should_project_every_builtin_and_property_shape() {
        let row = row(serde_json::json!({
            "owner": {"name": "Ada"},
            "tags": ["rust", 2, null],
            "done": true,
            "empty": null
        }));

        assert_eq!(row_value(&row, None), "Roadmap");
        assert_eq!(row_value(&row, Some("category")), "Projects");
        assert_eq!(row_value(&row, Some("updated")), "2026-09-09T12:00:00Z");
        assert_eq!(row_value(&row, Some("/owner/name")), "Ada");
        assert_eq!(row_value(&row, Some("/tags")), "rust, 2, ");
        assert_eq!(row_value(&row, Some("/done")), "true");
        assert_eq!(row_value(&row, Some("/empty")), "");
        assert_eq!(row_value(&row, Some("/missing")), "");
    }
}
