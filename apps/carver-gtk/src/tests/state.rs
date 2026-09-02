//! State, persistence, and storage-facing GTK tests.

use carver_config::{Config, EditorMode, load};

use crate::{
    controller::{
        AppState, active_category, create_next_category, create_note_for_active_category,
        open_note, rename_category, save_current_note, store_pasted_image, trash_current_note,
    },
    dialogs::persist_window_config,
};

use super::support::{TestResult, test_state};

#[test]
fn state_actions_create_open_and_save_notes_without_reentrant_borrows() -> TestResult {
    let (_temporary_directory, state) = test_state()?;
    let first_category = active_category(&state)?;
    assert!(first_category.is_some());

    let created = create_note_for_active_category(&state)?;
    let Some(created) = created else {
        panic!("a seeded library must have a category");
    };
    assert_eq!(
        state.current_note.borrow().as_ref().map(|note| note.id),
        Some(created.id)
    );

    let saved = save_current_note(&state, "# Regression note\nNo RefCell panic")?;
    let Some(saved) = saved else {
        panic!("the newly created note must remain active");
    };
    assert_eq!(saved.revision.0, 2);

    state.current_note.take();
    let reopened = open_note(&state, created.id)?;
    assert_eq!(
        reopened.as_ref().map(|note| note.source.as_str()),
        Some("# Regression note\nNo RefCell panic")
    );
    assert_eq!(
        reopened.as_ref().map(|note| note.updated_at),
        Some(saved.updated_at)
    );
    assert_eq!(
        reopened.as_ref().map(|note| note.revision),
        Some(saved.revision)
    );
    Ok(())
}

#[test]
fn state_actions_create_numbered_categories() -> TestResult {
    let (_temporary_directory, state) = test_state()?;
    let category = create_next_category(&state)?;
    assert_eq!(category.name, "Category 2");
    assert_eq!(state.client.categories()?.len(), 2);
    Ok(())
}

#[test]
fn state_uses_the_configured_initial_editor_mode() -> TestResult {
    let (_temporary_directory, state) = test_state()?;
    let mut config = Config::default();
    config.editor.last_mode = EditorMode::Source;
    let source_state = AppState::new(state.client.clone(), config);

    assert!(source_state.source_mode.get());
    assert!(!source_state.rendered_mode.get());

    let mut config = Config::default();
    config.editor.last_mode = EditorMode::Rendered;
    let rendered_state = AppState::new(state.client.clone(), config);
    assert!(!rendered_state.source_mode.get());
    assert!(rendered_state.rendered_mode.get());
    Ok(())
}

#[test]
fn state_persists_the_last_explicit_editor_surface() -> TestResult {
    let (temporary_directory, state) = test_state()?;
    let path = temporary_directory.path().join("config/config.toml");
    let persisted_state = AppState::new_with_assets(
        state.client.clone(),
        Config::default(),
        None,
        Some(path.clone()),
    );

    persisted_state.set_last_editor_mode(EditorMode::Rendered)?;

    let loaded = load(&path)?;
    assert_eq!(loaded.editor.last_mode, EditorMode::Rendered);
    let restored = AppState::new(state.client.clone(), loaded);
    assert!(restored.rendered_mode.get());
    Ok(())
}

#[test]
fn state_actions_rename_categories_and_trash_notes() -> TestResult {
    let (_temporary_directory, state) = test_state()?;
    let category = state.client.categories()?.remove(0);
    let renamed = rename_category(&state, category.id, "Work")?;
    assert_eq!(renamed.name, "Work");

    let created = create_note_for_active_category(&state)?;
    assert!(created.is_some());
    let Some(created) = created else {
        return Ok(());
    };
    assert!(trash_current_note(&state)?);
    assert!(state.current_note.borrow().is_none());
    assert!(state.client.recent_notes(None, 10, 0)?.is_empty());
    state.client.restore_note(created.id)?;
    assert_eq!(state.client.recent_notes(None, 10, 0)?.len(), 1);
    Ok(())
}

#[test]
fn state_action_stores_pasted_images_for_the_active_note() -> TestResult {
    let (temporary_directory, state) = test_state()?;
    let created = create_note_for_active_category(&state)?;
    assert!(created.is_some());
    let image_path = store_pasted_image(&state, b"test-png-bytes")?;
    assert!(image_path.is_some());
    let Some(image_path) = image_path else {
        return Ok(());
    };
    assert!(image_path.starts_with("assets/"));
    assert!(
        temporary_directory
            .path()
            .join("data")
            .join(&image_path)
            .is_file()
    );
    Ok(())
}

#[test]
fn close_action_persists_window_configuration() -> TestResult {
    let (temporary_directory, state) = test_state()?;
    let config_path = temporary_directory
        .path()
        .join("config")
        .join("config.toml");
    persist_window_config(&state, &config_path, 900, 640, true)?;
    let persisted = load(&config_path)?;
    assert_eq!(persisted.window.width, 900);
    assert_eq!(persisted.window.height, 640);
    assert!(persisted.window.maximized);
    Ok(())
}

#[test]
fn source_split_preference_persists_to_toml() -> TestResult {
    let (temporary_directory, state) = test_state()?;
    let config_path = temporary_directory
        .path()
        .join("config")
        .join("config.toml");
    let persistent_state = AppState::new_with_assets(
        state.client.clone(),
        Config::default(),
        None,
        Some(config_path.clone()),
    );

    persistent_state.set_source_split_view(true)?;

    assert!(load(&config_path)?.editor.source_split_view);
    assert!(persistent_state.config.borrow().editor.source_split_view);
    Ok(())
}
