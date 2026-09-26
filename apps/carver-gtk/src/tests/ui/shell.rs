//! Display-backed window shell, sidebar, and responsive navigation coverage.
use super::*;

pub(super) fn assert_sidebar_reload_preserves_rows() -> TestResult {
    let split_view = adw::NavigationSplitView::new();
    let sidebar =
        crate::ui::sidebar::build_sidebar(&crate::mvu::AppDispatcher::default(), &split_view);
    let sidebar_for_render = sidebar.clone();
    let render_count = Rc::new(Cell::new(0));
    let render_count_for_renderer = Rc::clone(&render_count);
    let route_stack = gtk::Stack::new();
    for route in ["browser", "editor", "trash"] {
        route_stack.add_named(&gtk::Box::new(gtk::Orientation::Vertical, 0), Some(route));
    }
    let view =
        crate::view::ViewRefs::new(route_stack, adw::StatusPage::new(), adw::StatusPage::new())
            .with_sidebar_renderer(move |model| {
                render_count_for_renderer.set(render_count_for_renderer.get() + 1);
                sidebar_for_render.render(model);
            });
    let mut model = crate::mvu::AppModel::new(&Config::default());
    model.sidebar.state = crate::mvu::LoadState::Ready(Vec::new());
    view.render(&model);
    let initial_row = sidebar.list.first_child();

    model.sidebar.state = crate::mvu::LoadState::Loading(crate::mvu::RequestId(1));
    view.render(&model);
    if sidebar.list.first_child() != initial_row {
        return Err("sidebar cleared its existing rows while reloading".into());
    }

    model.sidebar.state = crate::mvu::LoadState::Ready(Vec::new());
    view.render(&model);
    if render_count.get() != 1 || sidebar.list.first_child() != initial_row {
        return Err("sidebar rebuilt after receiving an unchanged reload result".into());
    }

    model.sidebar.state = crate::mvu::LoadState::Loading(crate::mvu::RequestId(2));
    view.render(&model);
    model.sidebar.state = crate::mvu::LoadState::Failed(crate::mvu::UiError::new("offline"));
    view.render(&model);
    if sidebar.list.first_child().is_some() {
        return Err("sidebar retained stale rows after a reload failed".into());
    }
    Ok(())
}

pub(super) fn window_shell_should_expose_sidebar_and_base_presentation(
    fixture: &WindowFixture,
) -> TestResult {
    let root = fixture.root()?;
    let source_font_filter = crate::ui::dialogs::source_font_filter_for_test();
    let monospace_family = root
        .pango_context()
        .list_families()
        .into_iter()
        .find(gtk::pango::prelude::FontFamilyExt::is_monospace)
        .ok_or("installed monospace font family")?;
    let monospace_face = monospace_family
        .list_faces()
        .into_iter()
        .next()
        .ok_or("monospace font face")?;
    assert!(source_font_filter.match_(&monospace_family));
    assert!(source_font_filter.match_(&monospace_face));
    assert!(widget_as::<gtk::Button>(&root, "sidebar-add-button").is_some());
    assert!(find_widget(&root, "new-base-button").is_none());
    let bases_divider =
        widget_as::<gtk::Separator>(&root, "bases-divider").ok_or("bases divider")?;
    assert!(bases_divider.next_sibling().is_some());
    let sidebar_scroll = widget_as::<gtk::ScrolledWindow>(&root, "sidebar-navigation-scroll")
        .ok_or("sidebar navigation scroll")?;
    assert!(bases_divider.is_ancestor(&sidebar_scroll));
    let bases_grid = widget_as::<gtk::ColumnView>(&root, "bases-grid").ok_or("bases grid")?;
    assert!(widget_as::<gtk::Button>(&root, "back-to-notes-from-base-button").is_some());
    assert!(widget_as::<gtk::ToggleButton>(&root, "base-toggle-categories-button").is_some());
    assert!(widget_as::<gtk::SearchBar>(&root, "base-search-bar").is_some());
    assert!(widget_as::<gtk::SearchEntry>(&root, "base-search-entry").is_some());
    assert!(widget_as::<gtk::ToggleButton>(&root, "base-search-toggle").is_some());
    assert!(bases_grid.shows_row_separators());
    assert!(bases_grid.shows_column_separators());
    let base_status = widget_as::<adw::StatusPage>(&root, "base-status").ok_or("base status")?;
    let base_pages = widget_as::<gtk::Stack>(&root, "base-pages").ok_or("base pages")?;
    crate::ui::bases::render_base_status(
        &crate::ui::bases::BaseViewRefs {
            configuration: std::cell::RefCell::new(None),
            configure: widget_as::<gtk::Button>(&root, "configure-base-button")
                .ok_or("configure base button")?,
            delete: widget_as::<gtk::Button>(&root, "delete-base-button")
                .ok_or("delete base button")?,
            title: widget_as::<gtk::Label>(&root, "base-title").ok_or("base title")?,
            search_bar: widget_as::<gtk::SearchBar>(&root, "base-search-bar")
                .ok_or("base search bar")?,
            search_entry: widget_as::<gtk::SearchEntry>(&root, "base-search-entry")
                .ok_or("base search entry")?,
            search_toggle: widget_as::<gtk::ToggleButton>(&root, "base-search-toggle")
                .ok_or("base search toggle")?,
            last_search_open: std::cell::Cell::new(false),
            grid: bases_grid.clone(),
            pages: base_pages.clone(),
            scroll: widget_as::<gtk::ScrolledWindow>(&root, "base-scroll").ok_or("base scroll")?,
            status: base_status.clone(),
            load_more: widget_as::<gtk::Button>(&root, "base-load-more").ok_or("base load more")?,
            rows: gtk::gio::ListStore::new::<glib::BoxedAnyObject>(),
            syncing_header_sort: std::rc::Rc::new(std::cell::Cell::new(false)),
            rendered_definition: std::cell::RefCell::new(None),
            rendered_rows: std::cell::RefCell::new(Vec::new()),
        },
        "Couldn’t load rows",
        "Test failure",
    );
    assert_eq!(base_pages.visible_child_name().as_deref(), Some("status"));
    assert_eq!(base_status.title(), "Couldn’t load rows");
    assert_eq!(base_status.description().as_deref(), Some("Test failure"));
    Ok(())
}

pub(super) fn window_shortcuts_should_open_dialogs(fixture: &WindowFixture) -> TestResult {
    let window = fixture.window.clone();
    let category = &fixture.category;
    let root = fixture.root()?;
    let sidebar = fixture.sidebar()?;
    let settings_menu = widget_as::<gtk::MenuButton>(&root, "sidebar-settings-menu-button")
        .ok_or("sidebar settings menu")?;
    let settings_model = settings_menu
        .menu_model()
        .ok_or("sidebar settings menu model")?;
    assert_eq!(settings_model.n_items(), 2);
    assert!(
        settings_model
            .item_link(1, gtk::gio::MENU_LINK_SECTION)
            .is_some()
    );
    assert!(widget_as::<gtk::MenuButton>(&root, "app-menu-button").is_none());
    assert!(window.lookup_action("keyboard-shortcuts").is_some());
    let window_controllers = window.observe_controllers();
    let window_shortcuts = (0..window_controllers.n_items())
        .filter_map(|index| window_controllers.item(index))
        .filter_map(|controller| controller.downcast::<gtk::EventControllerKey>().ok())
        .find(|controller| controller.name().as_deref() == Some("window-shortcuts"))
        .ok_or("window shortcuts")?;
    assert_eq!(
        window_shortcuts.propagation_phase(),
        gtk::PropagationPhase::Capture
    );
    let keyboard_shortcuts = crate::ui::dialogs::show_keyboard_shortcuts_dialog(&window);
    assert_eq!(
        keyboard_shortcuts.widget_name(),
        "keyboard-shortcuts-dialog"
    );
    keyboard_shortcuts.close();
    assert!(run_main_context_until(|| {
        find_widget(sidebar.upcast_ref(), &format!("category:{}", category.id)).is_some()
    }));
    let sidebar_surface =
        widget_as::<gtk::Box>(&root, "sidebar-surface").ok_or("sidebar surface")?;
    let sidebar_controllers = sidebar_surface.observe_controllers();
    let sidebar_search_shortcut = (0..sidebar_controllers.n_items())
        .filter_map(|index| sidebar_controllers.item(index))
        .filter_map(|controller| controller.downcast::<gtk::EventControllerKey>().ok())
        .find(|controller| controller.name().as_deref() == Some("sidebar-search-shortcut"))
        .ok_or("sidebar search shortcut")?;
    assert_eq!(
        sidebar_search_shortcut.propagation_phase(),
        gtk::PropagationPhase::Capture
    );
    Ok(())
}

pub(super) fn responsive_navigation_should_switch_sidebar_and_content(
    fixture: &WindowFixture,
) -> TestResult {
    let window = fixture.window.clone();
    let category = &fixture.category;
    let base = &fixture.base;
    let root = fixture.root()?;
    let sidebar = fixture.sidebar()?;
    let all_notes = all_notes_row(&sidebar).ok_or("all notes row")?;
    let navigation_container =
        widget_as::<adw::BreakpointBin>(&root, "responsive-navigation-container")
            .ok_or("responsive navigation container")?;
    let navigation = navigation_container
        .child()
        .and_downcast::<adw::NavigationSplitView>()
        .ok_or("responsive navigation split")?;
    window.set_default_size(360, 640);
    assert!(run_main_context_until(|| navigation.is_collapsed()));
    assert!(
        root.width() >= 360,
        "window content width: {}",
        root.width()
    );
    let sidebar_toggle = widget_as::<gtk::ToggleButton>(&root, "toggle-categories-button")
        .ok_or("responsive sidebar toggle")?;
    sidebar_toggle.set_active(true);
    assert!(run_main_context_until(|| !navigation.shows_content()));
    let category_row = find_widget(sidebar.upcast_ref(), &format!("category:{}", category.id))
        .and_downcast::<gtk::ListBoxRow>()
        .ok_or("responsive category row")?;
    sidebar.select_row(Some(&category_row));
    assert!(run_main_context_until(|| navigation.shows_content()));
    sidebar_toggle.set_active(true);
    assert!(run_main_context_until(|| !navigation.shows_content()));
    let base_button = widget_as::<gtk::Button>(&root, &format!("base:{}", base.id))
        .ok_or("responsive base button")?;
    base_button.emit_clicked();
    assert!(run_main_context_until(|| {
        navigation.shows_content()
            && widget_as::<gtk::Label>(&root, "base-title")
                .is_some_and(|title| title.text() == "Review base")
    }));
    sidebar_toggle.set_active(true);
    assert!(run_main_context_until(|| !navigation.shows_content()));
    sidebar.select_row(Some(&all_notes));
    assert!(run_main_context_until(|| navigation.shows_content()));
    window.set_default_size(1120, 760);
    assert!(run_main_context_until(|| !navigation.is_collapsed()));
    Ok(())
}
