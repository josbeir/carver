//! Shared display-backed window fixture and cross-scenario widget helpers.
use super::*;

/// Window, dialogs, and seed library shared by the display-backed scenario functions.
pub(crate) struct WindowFixture {
    pub directory: tempfile::TempDir,
    pub client: super::super::support::TestLibraryClient,
    pub application: adw::Application,
    pub window: adw::ApplicationWindow,
    pub config: Config,
    pub config_path: std::path::PathBuf,
    pub preferences_dialog: adw::PreferencesDialog,
    pub about_dialog: adw::AboutDialog,
    pub preferences_runtime: crate::mvu::AppRuntime<carver_storage_sqlite::SqliteLibrary>,
    pub category: carver_sdk::Category,
    pub destination: carver_sdk::Category,
    pub base: carver_sdk::BaseDefinition,
}

/// Builds the shared application window, dialogs, and seed library for one display run.
pub(crate) fn window_fixture() -> Result<WindowFixture, Box<dyn std::error::Error>> {
    let (temporary_directory, client) = test_state()?;
    let category = client.create_category_with_appearance(
        "Notes",
        carver_sdk::CategoryAppearance {
            icon: carver_sdk::CategoryIcon::Folder,
            color: carver_sdk::CategoryColor::Rose,
        },
    )?;
    let destination = client.create_category("Projects")?;
    let base = glib::MainContext::default().block_on(client.create_base_async(
        "Review base".to_owned(),
        vec![
            carver_sdk::BaseColumn::Name,
            carver_sdk::BaseColumn::Category,
            carver_sdk::BaseColumn::Updated,
            carver_sdk::BaseColumn::Property(carver_sdk::PropertyPath("/status".to_owned())),
        ],
    ))?;
    let application = adw::Application::new(
        Some("io.github.josbeir.Carver.Tests"),
        gtk::gio::ApplicationFlags::empty(),
    );
    application.register(None::<&gtk::gio::Cancellable>)?;
    let config_path = temporary_directory.path().join("config.toml");
    let mut config = Config::default();
    config.editor.autosave_delay_ms = 1;
    config.editor.source_line_numbers = true;
    config.editor.source_highlight_current_line = true;
    config.editor.source_syntax_style = SourceSyntaxStyle::WritingFocus;
    config.editor.document_font = Some("DejaVu Serif Italic 15".to_owned());
    let window =
        crate::app::build_window_for_test(&application, client.clone(), &config, &config_path)?;
    let preferences_dispatcher = crate::mvu::AppDispatcher::default();
    let preferences_stack = gtk::Stack::new();
    for name in ["browser", "editor", "trash"] {
        preferences_stack.add_named(&gtk::Box::new(gtk::Orientation::Vertical, 0), Some(name));
    }
    let preferences_runtime = crate::mvu::AppRuntime::new_with_config_path(
        client.clone(),
        crate::mvu::AppModel::new(&config),
        crate::view::ViewRefs::new(
            preferences_stack,
            adw::StatusPage::new(),
            adw::StatusPage::new(),
        ),
        Some(config_path.clone()),
    );
    preferences_runtime.bind_dispatcher(&preferences_dispatcher);
    let (preferences_dialog, about_dialog) =
        crate::ui::dialogs::present_dialogs_for_test(&window, &config, &preferences_dispatcher);
    Ok(WindowFixture {
        directory: temporary_directory,
        client,
        application,
        window,
        config,
        config_path,
        preferences_dialog,
        about_dialog,
        preferences_runtime,
        category,
        destination,
        base,
    })
}

impl WindowFixture {
    pub(crate) fn root(&self) -> Result<gtk::Widget, Box<dyn std::error::Error>> {
        Ok(self.window.child().ok_or("window content")?)
    }

    pub(crate) fn sidebar(&self) -> Result<gtk::ListBox, Box<dyn std::error::Error>> {
        Ok(widget_as::<gtk::ListBox>(&self.root()?, "category-list").ok_or("category list")?)
    }

    pub(crate) fn note_list(&self) -> Result<gtk::ListView, Box<dyn std::error::Error>> {
        Ok(widget_as::<gtk::ListView>(&self.root()?, "note-list").ok_or("note list")?)
    }

    pub(crate) fn route_stack(&self) -> Result<gtk::Stack, Box<dyn std::error::Error>> {
        Ok(widget_as::<gtk::Stack>(&self.root()?, "content-route-stack").ok_or("route stack")?)
    }

    pub(crate) fn source(&self) -> Result<gtk::TextView, Box<dyn std::error::Error>> {
        Ok(widget_as::<gtk::TextView>(&self.root()?, "source-editor").ok_or("source editor")?)
    }

    pub(crate) fn source_view(&self) -> Result<sourceview5::View, Box<dyn std::error::Error>> {
        Ok(
            widget_as::<sourceview5::View>(&self.root()?, "source-editor")
                .ok_or("GtkSourceView")?,
        )
    }

    pub(crate) fn source_buffer(&self) -> Result<sourceview5::Buffer, Box<dyn std::error::Error>> {
        Ok(self
            .source_view()?
            .buffer()
            .downcast::<sourceview5::Buffer>()
            .map_err(|_| "GtkSourceBuffer")?)
    }

    pub(crate) fn source_mode(&self) -> Result<gtk::ToggleButton, Box<dyn std::error::Error>> {
        Ok(
            widget_as::<gtk::ToggleButton>(&self.root()?, "editor-mode-source")
                .ok_or("source mode")?,
        )
    }

    pub(crate) fn rich_mode(&self) -> Result<gtk::ToggleButton, Box<dyn std::error::Error>> {
        Ok(widget_as::<gtk::ToggleButton>(&self.root()?, "editor-mode-rich").ok_or("rich mode")?)
    }

    pub(crate) fn toolbar(&self) -> Result<gtk::Box, Box<dyn std::error::Error>> {
        Ok(widget_as::<gtk::Box>(&self.root()?, "formatting-toolbar")
            .ok_or("formatting toolbar")?)
    }

    pub(crate) fn find_bar(&self) -> Result<gtk::SearchBar, Box<dyn std::error::Error>> {
        Ok(widget_as::<gtk::SearchBar>(&self.root()?, "editor-find-bar").ok_or("find bar")?)
    }

    pub(crate) fn find_entry(&self) -> Result<gtk::SearchEntry, Box<dyn std::error::Error>> {
        Ok(
            widget_as::<gtk::SearchEntry>(&self.root()?, "editor-find-entry")
                .ok_or("find entry")?,
        )
    }

    pub(crate) fn find_count(&self) -> Result<gtk::Label, Box<dyn std::error::Error>> {
        Ok(widget_as::<gtk::Label>(&self.root()?, "editor-find-count").ok_or("find count")?)
    }

    pub(crate) fn source_path(&self) -> Result<gtk::Label, Box<dyn std::error::Error>> {
        Ok(widget_as::<gtk::Label>(&self.root()?, "source-ast-path").ok_or("source AST path")?)
    }

    pub(crate) fn editor_shortcuts(
        &self,
    ) -> Result<gtk::EventControllerKey, Box<dyn std::error::Error>> {
        let editor_view =
            widget_as::<adw::ToolbarView>(&self.root()?, "editor-surface").ok_or("editor view")?;
        Ok(
            named_key_controller(&editor_view, "editor-window-shortcuts")
                .ok_or("editor shortcuts")?,
        )
    }

    pub(crate) fn sidebar_search_shortcut(
        &self,
    ) -> Result<gtk::EventControllerKey, Box<dyn std::error::Error>> {
        let sidebar_surface =
            widget_as::<gtk::Box>(&self.root()?, "sidebar-surface").ok_or("sidebar surface")?;
        Ok(
            named_key_controller(&sidebar_surface, "sidebar-search-shortcut")
                .ok_or("sidebar search shortcut")?,
        )
    }
}

fn named_key_controller<W: IsA<gtk::Widget>>(
    owner: &W,
    name: &str,
) -> Option<gtk::EventControllerKey> {
    let controllers = owner.observe_controllers();
    (0..controllers.n_items())
        .filter_map(|index| controllers.item(index))
        .filter_map(|controller| controller.downcast::<gtk::EventControllerKey>().ok())
        .find(|controller| controller.name().as_deref() == Some(name))
}

pub(crate) fn widget_is_window_focus(widget: &gtk::Widget) -> bool {
    widget
        .root()
        .and_downcast::<gtk::Window>()
        .and_then(|window| gtk::prelude::RootExt::focus(&window))
        .is_some_and(|focus| focus == *widget)
}

pub(crate) fn select_all(buffer: &gtk::TextBuffer) {
    let start = buffer.start_iter();
    let end = buffer.end_iter();
    buffer.select_range(&start, &end);
}

pub(crate) fn all_notes_row(sidebar: &gtk::ListBox) -> Option<gtk::ListBoxRow> {
    sidebar.first_child().and_downcast::<gtk::ListBoxRow>()
}

pub(crate) fn find_label(root: &gtk::Widget, text: &str) -> Option<gtk::Label> {
    if let Some(label) = root.downcast_ref::<gtk::Label>()
        && label.text() == text
    {
        return Some(label.clone());
    }
    let mut child = root.first_child();
    while let Some(widget) = child {
        if let Some(label) = find_label(&widget, text) {
            return Some(label);
        }
        child = widget.next_sibling();
    }
    None
}

pub(crate) fn activate_browser_note(list: &gtk::ListView, note_id: carver_sdk::NoteId) -> bool {
    let Some(model) = list.model() else {
        return false;
    };
    let Some(position) = (0..model.n_items()).find(|position| {
        model
            .item(*position)
            .and_downcast::<glib::BoxedAnyObject>()
            .is_some_and(|item| {
                matches!(
                    &*item.borrow::<crate::ui::browser::BrowserFeedItem>(),
                    crate::ui::browser::BrowserFeedItem::Note(current) if current.id == note_id
                )
            })
    }) else {
        return false;
    };
    list.emit_by_name::<()>("activate", &[&position]);
    true
}

pub(crate) fn assert_web_script_should_be_true(view: &webkit6::WebView, script: &str) {
    let result = Rc::new(Cell::new(false));
    let pending = Rc::new(Cell::new(false));
    assert!(
        run_main_context_until(|| {
            if result.get() {
                return true;
            }
            if !pending.replace(true) {
                let response = Rc::clone(&result);
                let pending = Rc::clone(&pending);
                view.evaluate_javascript(
                    script,
                    None,
                    None,
                    None::<&gtk::gio::Cancellable>,
                    move |value| {
                        response.set(value.is_ok_and(|value| value.to_boolean()));
                        pending.set(false);
                    },
                );
            }
            false
        }),
        "{script}"
    );
}
