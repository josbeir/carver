use super::*;

type TestResult = Result<(), String>;

fn server(allow_write: bool) -> Result<(tempfile::TempDir, CarverServer), String> {
    let directory = tempfile::tempdir().map_err(|error| error.to_string())?;
    let client = carver_sdk::open_local_library(
        &directory.path().join("library.sqlite3"),
        &directory.path().join("assets"),
    )
    .map_err(|error| error.to_string())?;
    Ok((directory, CarverServer::new(client, allow_write)))
}

fn id(result: &str) -> Result<String, String> {
    let value =
        serde_json::from_str::<serde_json::Value>(result).map_err(|error| error.to_string())?;
    value["id"]
        .as_str()
        .map(ToOwned::to_owned)
        .ok_or_else(|| "tool output did not include an id".to_owned())
}

async fn create_and_read_note(server: &CarverServer) -> Result<(String, String), String> {
    let category = server
        .create_category(Parameters(CreateCategoryRequest {
            name: "Journal".to_owned(),
            appearance: None,
        }))
        .await
        .map_err(|error| error.to_string())?;
    let category_id = id(&category)?;
    let renamed = server
        .rename_category(Parameters(RenameCategoryRequest {
            category_id: category_id.clone(),
            name: "Work".to_owned(),
        }))
        .await
        .map_err(|error| error.to_string())?;
    assert!(renamed.contains("Work"));

    let created = server
        .create_note(Parameters(CreateNoteRequest {
            category_id: category_id.clone(),
            source: "# Planning\n\nPrepare the launch.".to_owned(),
            markdown: Some(true),
        }))
        .await
        .map_err(|error| error.to_string())?;
    let note_id = id(&created)?;
    let listed = server
        .list_notes(Parameters(ListNotesRequest {
            category_id: Some(category_id.clone()),
            limit: Some(1),
            offset: None,
        }))
        .await
        .map_err(|error| error.to_string())?;
    assert!(listed.contains("Planning"));
    let search = server
        .search_notes(Parameters(SearchRequest {
            query: "launch".to_owned(),
            category_id: None,
            limit: None,
        }))
        .await
        .map_err(|error| error.to_string())?;
    assert!(search.contains("Planning"));
    let note = server
        .get_note(Parameters(GetNoteRequest {
            note_id: note_id.clone(),
            markdown: None,
        }))
        .await
        .map_err(|error| error.to_string())?;
    assert!(note.contains("Planning"));
    let markdown = server
        .get_note(Parameters(GetNoteRequest {
            note_id: note_id.clone(),
            markdown: Some(true),
        }))
        .await
        .map_err(|error| error.to_string())?;
    let markdown =
        serde_json::from_str::<serde_json::Value>(&markdown).map_err(|error| error.to_string())?;
    assert_eq!(markdown["source"], "# Planning\n\nPrepare the launch.\n");
    Ok((category_id, note_id))
}

async fn save_move_and_restore_note(
    server: &CarverServer,
    note_id: String,
) -> Result<String, String> {
    let parsed_id = parse_note(&note_id).map_err(|error| error.to_string())?;
    let note = server
        .client
        .note_async(parsed_id)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "created note should exist".to_owned())?;
    let saved = server
        .save_note(Parameters(SaveNoteRequest {
            note_id: note_id.clone(),
            revision: note.revision.0,
            source: "Saved as Carve".to_owned(),
            markdown: Some(false),
        }))
        .await
        .map_err(|error| error.to_string())?;
    assert!(saved.contains("Saved as Carve"));

    let category = server
        .create_category(Parameters(CreateCategoryRequest {
            name: "Archive".to_owned(),
            appearance: None,
        }))
        .await
        .map_err(|error| error.to_string())?;
    let destination_id = id(&category)?;
    let moved = server
        .move_note(Parameters(MoveNoteRequest {
            note_id: note_id.clone(),
            category_id: destination_id.clone(),
        }))
        .await
        .map_err(|error| error.to_string())?;
    let moved =
        serde_json::from_str::<serde_json::Value>(&moved).map_err(|error| error.to_string())?;
    assert_eq!(moved["category_id"], destination_id);

    let trashed = server
        .trash_note(Parameters(NoteRequest {
            note_id: note_id.clone(),
        }))
        .await
        .map_err(|error| error.to_string())?;
    assert_eq!(trashed, "note moved to trash");
    assert!(
        server
            .get_note(Parameters(GetNoteRequest {
                note_id: note_id.clone(),
                markdown: None,
            }))
            .await
            .is_err()
    );
    let trash = server
        .list_trash()
        .await
        .map_err(|error| error.to_string())?;
    assert!(trash.contains("Saved as Carve"));
    let restored = server
        .restore_note(Parameters(NoteRequest { note_id }))
        .await
        .map_err(|error| error.to_string())?;
    assert_eq!(restored, "note restored");
    Ok(destination_id)
}

#[tokio::test]
async fn write_tools_should_manage_a_note_lifecycle() -> TestResult {
    let (_directory, server) = server(true)?;
    let (_category_id, note_id) = create_and_read_note(&server).await?;
    let destination_id = save_move_and_restore_note(&server, note_id).await?;

    let trashed = server
        .trash_category(Parameters(CategoryRequest {
            category_id: destination_id.clone(),
        }))
        .await
        .map_err(|error| error.to_string())?;
    assert_eq!(trashed, "category moved to trash");
    let restored = server
        .restore_category(Parameters(CategoryRequest {
            category_id: destination_id,
        }))
        .await
        .map_err(|error| error.to_string())?;
    assert_eq!(restored, "category restored");
    let categories = server
        .list_categories()
        .await
        .map_err(|error| error.to_string())?;
    assert!(categories.contains("Archive"));
    Ok(())
}

#[tokio::test]
async fn create_category_should_return_requested_appearance() -> TestResult {
    let (_directory, server) = server(true)?;
    let created = server
        .create_category(Parameters(CreateCategoryRequest {
            name: "Ideas".to_owned(),
            appearance: Some(CategoryAppearanceRequest {
                icon: CategoryIconRequest::Lightbulb,
                color: CategoryColorRequest::Yellow,
            }),
        }))
        .await
        .map_err(|error| error.to_string())?;
    let created =
        serde_json::from_str::<serde_json::Value>(&created).map_err(|error| error.to_string())?;

    assert_eq!(
        created["appearance"],
        serde_json::json!({ "icon": "Lightbulb", "color": "Yellow" })
    );
    Ok(())
}

#[tokio::test]
async fn update_category_should_return_requested_appearance() -> TestResult {
    let (_directory, server) = server(true)?;
    let created = server
        .create_category(Parameters(CreateCategoryRequest {
            name: "Ideas".to_owned(),
            appearance: None,
        }))
        .await
        .map_err(|error| error.to_string())?;
    let category_id = id(&created)?;

    let updated = server
        .update_category(Parameters(UpdateCategoryRequest {
            category_id,
            name: "Personal ideas".to_owned(),
            appearance: CategoryAppearanceRequest {
                icon: CategoryIconRequest::Heart,
                color: CategoryColorRequest::Rose,
            },
        }))
        .await
        .map_err(|error| error.to_string())?;
    let updated =
        serde_json::from_str::<serde_json::Value>(&updated).map_err(|error| error.to_string())?;

    assert_eq!(
        updated["appearance"],
        serde_json::json!({ "icon": "Heart", "color": "Rose" })
    );
    Ok(())
}

#[tokio::test]
async fn update_note_timestamps_should_return_requested_dates() -> TestResult {
    let (_directory, server) = server(true)?;
    let category = server
        .create_category(Parameters(CreateCategoryRequest {
            name: "Journal".to_owned(),
            appearance: None,
        }))
        .await
        .map_err(|error| error.to_string())?;
    let created = server
        .create_note(Parameters(CreateNoteRequest {
            category_id: id(&category)?,
            source: "# Dated entry".to_owned(),
            markdown: None,
        }))
        .await
        .map_err(|error| error.to_string())?;
    let note_id = id(&created)?;
    let created =
        serde_json::from_str::<serde_json::Value>(&created).map_err(|error| error.to_string())?;
    let revision = created["revision"]
        .as_i64()
        .ok_or_else(|| "created note did not include a revision".to_owned())?;

    let updated = server
        .update_note_timestamps(Parameters(UpdateNoteTimestampsRequest {
            note_id,
            revision,
            created_at: "2020-01-02T03:04:05Z".to_owned(),
            updated_at: "2021-02-03T04:05:06Z".to_owned(),
        }))
        .await
        .map_err(|error| error.to_string())?;
    let updated =
        serde_json::from_str::<serde_json::Value>(&updated).map_err(|error| error.to_string())?;

    assert_eq!(
        (updated["created_at"].clone(), updated["updated_at"].clone()),
        (
            serde_json::json!("2020-01-02T03:04:05Z"),
            serde_json::json!("2021-02-03T04:05:06Z"),
        )
    );
    Ok(())
}

#[tokio::test]
async fn update_note_timestamps_should_reject_non_rfc3339_dates() -> TestResult {
    let (_directory, server) = server(true)?;

    let error = server
        .update_note_timestamps(Parameters(UpdateNoteTimestampsRequest {
            note_id: uuid::Uuid::new_v4().to_string(),
            revision: 1,
            created_at: "yesterday".to_owned(),
            updated_at: "2021-02-03T04:05:06Z".to_owned(),
        }))
        .await
        .err()
        .ok_or_else(|| "an invalid timestamp should be rejected".to_owned())?;

    assert!(error.message.contains("created_at"));
    Ok(())
}

#[tokio::test]
async fn read_only_server_should_reject_writes_and_validate_requests() -> TestResult {
    let (_directory, server) = server(false)?;
    let error = server
        .create_category(Parameters(CreateCategoryRequest {
            name: "Blocked".to_owned(),
            appearance: None,
        }))
        .await
        .err()
        .ok_or_else(|| "read-only server should reject category creation".to_owned())?;
    assert!(error.message.contains("--allow-write"));
    let error = server
        .list_notes(Parameters(ListNotesRequest {
            category_id: None,
            limit: Some(0),
            offset: None,
        }))
        .await
        .err()
        .ok_or_else(|| "zero limit should be rejected".to_owned())?;
    assert!(error.message.contains("between 1 and 100"));
    let error = server
        .search_notes(Parameters(SearchRequest {
            query: "anything".to_owned(),
            category_id: Some("not-a-uuid".to_owned()),
            limit: Some(101),
        }))
        .await
        .err()
        .ok_or_else(|| "invalid category should be rejected".to_owned())?;
    assert!(error.message.contains("category_id"));
    assert!(parse_note("not-a-uuid").is_err());
    assert_eq!(document_format(None), DocumentImportFormat::Carve);
    assert_eq!(document_format(Some(true)), DocumentImportFormat::Markdown);
    assert_eq!(prompt("Read this").len(), 1);
    assert!(server.get_info().capabilities.tools.is_some());
    assert_eq!(server.capture_note().await.len(), 1);
    assert_eq!(server.summarize_notes().await.len(), 1);
    assert_eq!(server.organize_notes().await.len(), 1);
    assert_eq!(print_setup(&["codex".to_owned()]), ExitCode::SUCCESS);
    assert_eq!(print_setup(&["claude-code".to_owned()]), ExitCode::SUCCESS);
    assert_eq!(print_setup(&["copilot".to_owned()]), ExitCode::SUCCESS);
    assert_eq!(print_setup(&["vscode".to_owned()]), ExitCode::SUCCESS);
    assert_eq!(print_setup(&["generic".to_owned()]), ExitCode::SUCCESS);
    assert_eq!(print_setup(&["unknown".to_owned()]), ExitCode::FAILURE);
    Ok(())
}
