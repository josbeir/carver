//! Native database-style grid for saved note bases.

use carver_sdk::{BaseColumn, BaseDefinition, BaseRow};
use gtk::prelude::*;
use libadwaita as adw;

use crate::mvu::{AppDispatcher, AppMsg, NavigationMsg};
use crate::ui::sidebar::{CompactNavigation, back_to_notes_button, sidebar_toggle_button};

/// Widgets needed to render the current saved base.
pub(crate) struct BaseViewRefs {
    pub(crate) title: gtk::Label,
    pub(crate) grid: gtk::ColumnView,
    pub(crate) pages: gtk::Stack,
    pub(crate) scroll: gtk::ScrolledWindow,
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
        .description("Notes with JSON or TOML frontmatter will appear here.")
        .build();
    let pages = gtk::Stack::new();
    pages.add_named(&scroll, Some("grid"));
    pages.add_named(&status, Some("status"));
    pages.set_visible_child_name("status");
    toolbar.set_content(Some(&pages));
    (
        toolbar.upcast(),
        BaseViewRefs {
            title,
            grid,
            pages,
            scroll,
        },
    )
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
            let click = gtk::GestureClick::new();
            let dispatcher = dispatcher.clone();
            let item = item.clone();
            click.connect_released(move |_, _, _, _| {
                let Some(string) = item.item().and_downcast::<gtk::StringObject>() else {
                    return;
                };
                let Ok(row) = serde_json::from_str::<BaseRow>(&string.string()) else {
                    return;
                };
                let _ =
                    dispatcher.dispatch(AppMsg::Navigation(NavigationMsg::OpenNote(row.note_id)));
            });
            label.add_controller(click);
        }
        item.set_child(Some(&label));
    });
    let field_for_bind = field.map(ToOwned::to_owned);
    factory.connect_bind(move |_, item| {
        let Some(item) = item.downcast_ref::<gtk::ListItem>() else {
            return;
        };
        let Some(string) = item.item().and_downcast::<gtk::StringObject>() else {
            return;
        };
        let Some(label) = item.child().and_downcast::<gtk::Label>() else {
            return;
        };
        let Ok(row) = serde_json::from_str::<BaseRow>(&string.string()) else {
            return;
        };
        let value = match field_for_bind.as_deref() {
            None => row.name.clone(),
            Some("category") => row.category.clone(),
            Some("updated") => row.updated.clone(),
            Some(path) => row
                .properties
                .pointer(path)
                .map(display_value)
                .unwrap_or_default(),
        };
        label.set_text(&value);
    });
    let column = gtk::ColumnViewColumn::new(Some(title), Some(factory));
    column.set_resizable(true);
    column.set_expand(field.is_none());
    grid.append_column(&column);
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
