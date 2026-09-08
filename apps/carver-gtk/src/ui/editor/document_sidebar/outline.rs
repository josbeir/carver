//! Native GTK tree projection of the canonical heading hierarchy.
use crate::mvu::{AppDispatcher, AppMsg, EditorDocument, EditorMsg, EditorSessionId};
use carver_domain::source_analysis::HeadingOccurrence;
use carver_editor_protocol::DocumentTarget;
use gtk::{gio, prelude::*};

const OUTLINE_INDENT: i32 = 12;

struct HeadingRow {
    label: String,
    level: u8,
    occurrence: usize,
    subtree_end: usize,
    children: gio::ListStore,
    session: EditorSessionId,
    generation: u64,
}

pub(super) struct Outline {
    pub view: gtk::ListView,
    roots: gio::ListStore,
    tree: gtk::TreeListModel,
    selection: gtk::SingleSelection,
}

impl Outline {
    pub fn new(dispatcher: &AppDispatcher) -> Self {
        let roots = gio::ListStore::new::<glib::BoxedAnyObject>();
        let tree = gtk::TreeListModel::new(roots.clone(), false, true, |object| {
            let item = object.downcast_ref::<glib::BoxedAnyObject>()?;
            let row = item.borrow::<HeadingRow>();
            (row.children.n_items() > 0).then(|| row.children.clone().upcast())
        });
        let selection = gtk::SingleSelection::new(Some(tree.clone()));
        selection.set_autoselect(false);
        selection.set_can_unselect(true);
        let factory = row_factory(dispatcher);
        let view = gtk::ListView::new(Some(selection.clone()), Some(factory));
        view.set_widget_name("editor-outline-list");
        view.add_css_class("document-outline");
        // GTK's single-click mode also selects rows on hover. Document selection must
        // follow explicit navigation or the editor caret, never pointer motion.
        view.set_single_click_activate(false);
        let dispatcher = dispatcher.clone();
        view.connect_activate(move |view, position| {
            let Some(row) = view
                .model()
                .and_then(|model| model.item(position))
                .and_downcast::<gtk::TreeListRow>()
            else {
                return;
            };
            activate_heading(&row, &dispatcher);
        });
        Self {
            view,
            roots,
            tree,
            selection,
        }
    }

    pub fn rebuild(&self, document: &EditorDocument) {
        let pending_roots = gio::ListStore::new::<glib::BoxedAnyObject>();
        let headings = document.analysis.headings();
        let mut parents: Vec<(u8, gio::ListStore)> = Vec::new();
        for (occurrence, heading) in headings.iter().enumerate() {
            while parents
                .last()
                .is_some_and(|(level, _)| *level >= heading.level)
            {
                parents.pop();
            }
            let children = gio::ListStore::new::<glib::BoxedAnyObject>();
            let store = parents.last().map_or(&pending_roots, |(_, store)| store);
            store.append(&glib::BoxedAnyObject::new(HeadingRow {
                label: heading.label.clone(),
                level: heading.level,
                occurrence,
                subtree_end: subtree_end(headings, occurrence),
                children: children.clone(),
                session: document.session,
                generation: document.source_generation,
            }));
            parents.push((heading.level, children));
        }
        // Publish a complete immutable projection so GTK never observes partial child lists.
        let roots: Vec<glib::Object> = (0..pending_roots.n_items())
            .filter_map(|index| pending_roots.item(index))
            .collect();
        self.roots.splice(0, self.roots.n_items(), &roots);
    }

    pub fn select(&self, occurrence: Option<usize>, reveal: bool) {
        let mut position = 0;
        let mut selected = gtk::INVALID_LIST_POSITION;
        while position < self.tree.n_items() {
            let Some(row) = self.tree.row(position) else {
                break;
            };
            if let Some(object) = row.item().and_downcast::<glib::BoxedAnyObject>() {
                let (index, end) = {
                    let item = object.borrow::<HeadingRow>();
                    (item.occurrence, item.subtree_end)
                };
                if occurrence == Some(index) {
                    selected = position;
                    break;
                }
                if occurrence.is_some_and(|target| index < target && target < end) {
                    row.set_expanded(true);
                }
            }
            position += 1;
        }
        let changed = self.selection.selected() != selected;
        self.selection.set_selected(selected);
        if reveal && changed && selected != gtk::INVALID_LIST_POSITION {
            self.view
                .scroll_to(selected, gtk::ListScrollFlags::empty(), None);
        }
    }
}

fn subtree_end(headings: &[HeadingOccurrence], occurrence: usize) -> usize {
    headings[occurrence + 1..]
        .iter()
        .position(|heading| heading.level <= headings[occurrence].level)
        .map_or(headings.len(), |offset| occurrence + 1 + offset)
}

fn row_factory(dispatcher: &AppDispatcher) -> gtk::SignalListItemFactory {
    let factory = gtk::SignalListItemFactory::new();
    let dispatcher = dispatcher.clone();
    factory.connect_setup(move |_, item| {
        let Some(item) = item.downcast_ref::<gtk::ListItem>() else {
            return;
        };
        let label = gtk::Label::new(None);
        label.set_xalign(0.0);
        label.set_hexpand(true);
        label.set_ellipsize(gtk::pango::EllipsizeMode::End);
        label.add_css_class("outline-heading");
        let expander = gtk::TreeExpander::new();
        expander.set_widget_name("editor-outline-item");
        expander.set_hide_expander(true);
        expander.set_indent_for_depth(false);
        expander.set_indent_for_icon(false);
        expander.action_set_enabled("listitem.collapse", false);
        expander.action_set_enabled("listitem.toggle-expand", false);
        let content = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        let level = gtk::Label::new(None);
        level.set_widget_name("editor-outline-level");
        level.add_css_class("outline-level");
        level.set_accessible_role(gtk::AccessibleRole::Presentation);
        content.append(&level);
        content.append(&label);
        expander.set_child(Some(&content));
        install_heading_click(&expander, item, &dispatcher);
        item.set_child(Some(&expander));
    });
    factory.connect_bind(|_, item| {
        let Some(item) = item.downcast_ref::<gtk::ListItem>() else {
            return;
        };
        let Some(row) = item.item().and_downcast::<gtk::TreeListRow>() else {
            return;
        };
        let Some(expander) = item.child().and_downcast::<gtk::TreeExpander>() else {
            return;
        };
        let Some(content) = expander.child().and_downcast::<gtk::Box>() else {
            return;
        };
        let Some(level_label) = content.first_child().and_downcast::<gtk::Label>() else {
            return;
        };
        let Some(label) = content.last_child().and_downcast::<gtk::Label>() else {
            return;
        };
        let Some(object) = row.item().and_downcast::<glib::BoxedAnyObject>() else {
            return;
        };
        let (text, level) = {
            let heading = object.borrow::<HeadingRow>();
            (heading.label.clone(), heading.level)
        };
        expander.set_list_row(Some(&row));
        content.set_margin_start(i32::try_from(row.depth()).unwrap_or(0) * OUTLINE_INDENT);
        level_label.set_label(&format!("H{level}"));
        label.set_label(&text);
        label.set_tooltip_text(Some(&text));
    });
    factory.connect_unbind(|_, item| {
        if let Some(expander) = item
            .downcast_ref::<gtk::ListItem>()
            .and_then(gtk::prelude::ListItemExt::child)
            .and_downcast::<gtk::TreeExpander>()
        {
            expander.set_list_row(None::<&gtk::TreeListRow>);
        }
    });
    factory
}

fn activate_heading(row: &gtk::TreeListRow, dispatcher: &AppDispatcher) {
    let Some(object) = row.item().and_downcast::<glib::BoxedAnyObject>() else {
        return;
    };
    let message = {
        let row = object.borrow::<HeadingRow>();
        EditorMsg::FocusDocumentTarget {
            session: row.session,
            generation: row.generation,
            target: DocumentTarget::Heading {
                occurrence: row.occurrence,
            },
        }
    };
    let _ = dispatcher.dispatch(AppMsg::Editor(message));
}

fn install_heading_click(
    expander: &gtk::TreeExpander,
    item: &gtk::ListItem,
    dispatcher: &AppDispatcher,
) {
    let click = gtk::GestureClick::new();
    click.set_name(Some("outline-heading-click"));
    click.set_button(gtk::gdk::BUTTON_PRIMARY);
    click.set_propagation_phase(gtk::PropagationPhase::Capture);
    click.connect_pressed(|click, _, _, _| {
        click.set_state(gtk::EventSequenceState::Claimed);
    });
    let item = item.downgrade();
    let dispatcher = dispatcher.clone();
    click.connect_released(move |click, count, x, y| {
        if count != 1 || !click.widget().is_some_and(|widget| widget.contains(x, y)) {
            return;
        }
        if let Some(row) = item
            .upgrade()
            .and_then(|item| item.item())
            .and_downcast::<gtk::TreeListRow>()
        {
            activate_heading(&row, &dispatcher);
        }
    });
    expander.add_controller(click);
}
