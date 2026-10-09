//! Rendered regression coverage of the browser's pointer and keyboard focus.

use super::*;

pub(super) fn note_cards_should_keep_keyboard_focus_inside_the_card() -> TestResult {
    let fixture = window_fixture_seeded(
        "io.github.josbeir.Carver.Tests.NoteFocus",
        |client, category| {
            let note = client.create_note(category)?;
            client.set_note_favorite(note.id, note.revision, true)?;
            Ok(())
        },
        |_| {},
    )?;
    templates::dismiss_initial_dialogs(&fixture.window);
    let root = fixture.root()?;
    fixture.window.present();
    assert!(run_main_context_until(|| {
        find_widget(&root, "favorites-section").is_some()
            && fixture
                .note_list()
                .is_ok_and(|list| list.width() > 0 && list.height() > 0)
    }));
    let list = fixture.note_list()?;
    let mut cards = Vec::new();
    let mut row = list.first_child();
    while let Some(current) = row {
        if let Some(card) = current.first_child() {
            if card.has_css_class("note-card") {
                assert!(current.is_focusable());
                cards.push(card);
            } else {
                assert!(!current.is_focusable(), "headings must not take row focus");
            }
        }
        row = current.next_sibling();
    }
    assert_eq!(
        cards.len(),
        2,
        "both favorite and chronological cards render"
    );

    let style = adw::StyleManager::default();
    let original_scheme = style.color_scheme();
    for scheme in [adw::ColorScheme::ForceLight, adw::ColorScheme::ForceDark] {
        style.set_color_scheme(scheme);
        for card in &cards {
            assert_focus_stays_on_card(&fixture.window, card)?;
        }
    }
    style.set_color_scheme(original_scheme);

    let row = cards[0].parent().ok_or("note row")?;
    assert!(row.grab_focus());
    assert!(
        row.activate(),
        "keyboard activation must still open the note"
    );
    assert!(run_main_context_until(|| note_tab_is_active(&root)));
    fixture.window.close();
    Ok(())
}

fn assert_focus_stays_on_card(window: &adw::ApplicationWindow, card: &gtk::Widget) -> TestResult {
    let row = card.parent().ok_or("note row")?;
    assert!(row.grab_focus());
    // Simulate GTK's input-modality state without synthesizing backend events.
    window.set_focus_visible(false);
    let (pointer, stride) = render_row(&row)?;
    window.set_focus_visible(true);
    let (keyboard, keyboard_stride) = render_row(&row)?;
    assert_eq!(stride, keyboard_stride);
    let bounds = card.compute_bounds(&row).ok_or("card bounds")?;
    assert_focus_changes_within(&pointer, &keyboard, stride, &bounds)?;

    window.set_focus_visible(false);
    assert_eq!(
        render_row(&row)?.0,
        pointer,
        "pointer focus must hide the ring"
    );

    window.set_focus_visible(true);
    let menu = find_menu(card).ok_or("note menu")?;
    assert!(menu.grab_focus());
    assert!(run_main_context_until(|| !row.has_focus()));
    let (menu_focus, _) = render_row(&row)?;
    // Only the menu indicator paints when its child button owns focus.
    // Libadwaita's native button outline extends just beyond its allocation.
    let menu_bounds = menu
        .compute_bounds(&row)
        .ok_or("menu bounds")?
        .inset_r(-2.0, -2.0);
    assert_focus_changes_within(&pointer, &menu_focus, stride, &menu_bounds)?;
    Ok(())
}

fn assert_focus_changes_within(
    before: &[u8],
    after: &[u8],
    stride: usize,
    bounds: &gtk::graphene::Rect,
) -> TestResult {
    assert_eq!(before.len(), after.len());
    let mut changed = 0;
    for (y, (before, after)) in before
        .chunks_exact(stride)
        .zip(after.chunks_exact(stride))
        .enumerate()
    {
        for (x, (before, after)) in before
            .as_chunks::<4>()
            .0
            .iter()
            .zip(after.as_chunks::<4>().0)
            .enumerate()
        {
            if before != after {
                changed += 1;
                let point = gtk::graphene::Point::new(
                    f32::from(u16::try_from(x)?),
                    f32::from(u16::try_from(y)?),
                );
                assert!(
                    bounds.contains_point(&point),
                    "focus must not paint outside its control {bounds:?} at ({x}, {y})"
                );
            }
        }
    }
    assert!(changed > 0, "keyboard focus must remain visibly indicated");
    Ok(())
}

fn find_menu(root: &gtk::Widget) -> Option<gtk::MenuButton> {
    if let Some(menu) = root.downcast_ref::<gtk::MenuButton>() {
        return Some(menu.clone());
    }
    let mut child = root.first_child();
    while let Some(current) = child {
        if let Some(menu) = find_menu(&current) {
            return Some(menu);
        }
        child = current.next_sibling();
    }
    None
}

fn render_row(row: &gtk::Widget) -> Result<(glib::Bytes, usize), Box<dyn std::error::Error>> {
    // Allow theme transitions and GtkWidgetPaintable's frame cache to settle.
    let _ = run_main_context_until_for(Duration::from_millis(250), || false);
    let paintable = gtk::WidgetPaintable::new(Some(row));
    let snapshot = gtk::Snapshot::new();
    paintable.snapshot(&snapshot, f64::from(row.width()), f64::from(row.height()));
    let node = snapshot.to_node().ok_or("row snapshot")?;
    let renderer = row
        .native()
        .and_then(|native| native.renderer())
        .ok_or("row renderer")?;
    // A card shadow can change the node's bounds. Keep pixels in row coordinates
    // so comparisons include the gutters rather than shifting with the shadow.
    let viewport = gtk::graphene::Rect::new(
        0.0,
        0.0,
        f32::from(u16::try_from(row.width())?),
        f32::from(u16::try_from(row.height())?),
    );
    let texture = renderer.render_texture(&node, Some(&viewport));
    let mut downloader = gtk::gdk::TextureDownloader::new(&texture);
    downloader.set_format(gtk::gdk::MemoryFormat::R8g8b8a8);
    Ok(downloader.download_bytes())
}
