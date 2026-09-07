use super::*;
use carver_sdk::{CategoryColor, CategoryIcon};

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

fn id<T: serde::de::DeserializeOwned>(result: &str) -> Result<T, String> {
    let value: serde_json::Value =
        serde_json::from_str(result).map_err(|error| error.to_string())?;
    serde_json::from_value(value["id"].clone()).map_err(|error| error.to_string())
}

async fn create_and_read_note(server: &CarverServer) -> Result<(CategoryId, NoteId), String> {
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
            category_id,
            name: "Work".to_owned(),
        }))
        .await
        .map_err(|error| error.to_string())?;
    assert!(renamed.contains("Work"));

    let created = server
        .create_note(Parameters(CreateNoteRequest {
            category_id,
            source: "# Planning\n\nPrepare the launch.".to_owned(),
            markdown: Some(true),
        }))
        .await
        .map_err(|error| error.to_string())?;
    let note_id = id(&created)?;
    let listed = server
        .list_notes(Parameters(ListNotesRequest {
            category_id: Some(category_id),
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
            note_id,
            markdown: None,
        }))
        .await
        .map_err(|error| error.to_string())?;
    assert!(note.contains("Planning"));
    let markdown = server
        .get_note(Parameters(GetNoteRequest {
            note_id,
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
    note_id: NoteId,
) -> Result<CategoryId, String> {
    let note = server
        .client
        .note_async(note_id)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "created note should exist".to_owned())?;
    let saved = server
        .save_note(Parameters(SaveNoteRequest {
            note_id,
            revision: note.revision,
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
            note_id,
            category_id: destination_id,
        }))
        .await
        .map_err(|error| error.to_string())?;
    let moved =
        serde_json::from_str::<serde_json::Value>(&moved).map_err(|error| error.to_string())?;
    assert_eq!(moved["category_id"], serde_json::json!(destination_id));

    let trashed = server
        .trash_note(Parameters(NoteRequest { note_id }))
        .await
        .map_err(|error| error.to_string())?;
    assert_eq!(trashed, "note moved to trash");
    assert!(
        server
            .get_note(Parameters(GetNoteRequest {
                note_id,
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
            category_id: destination_id,
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
async fn favorite_tools_should_list_and_update_notes_when_writes_are_enabled() -> TestResult {
    let (_directory, server) = server(true)?;
    let (category_id, note_id) = create_and_read_note(&server).await?;
    let note = server
        .get_note(Parameters(GetNoteRequest {
            note_id,
            markdown: None,
        }))
        .await
        .map_err(|error| error.to_string())?;
    let note =
        serde_json::from_str::<serde_json::Value>(&note).map_err(|error| error.to_string())?;
    let updated = server
        .set_note_favorite(Parameters(SetNoteFavoriteRequest {
            note_id,
            revision: serde_json::from_value(note["revision"].clone())
                .map_err(|error| error.to_string())?,
            favorite: true,
        }))
        .await
        .map_err(|error| error.to_string())?;
    let updated =
        serde_json::from_str::<serde_json::Value>(&updated).map_err(|error| error.to_string())?;
    assert_eq!(updated["is_favorite"], true);
    let favorites = server
        .list_favorite_notes(Parameters(ListNotesRequest {
            category_id: None,
            limit: Some(10),
            offset: None,
        }))
        .await
        .map_err(|error| error.to_string())?;
    assert!(favorites.contains("Planning"));
    let category_favorites = server
        .list_favorite_notes(Parameters(ListNotesRequest {
            category_id: Some(category_id),
            limit: Some(10),
            offset: None,
        }))
        .await
        .map_err(|error| error.to_string())?;
    assert!(category_favorites.contains("Planning"));
    Ok(())
}

#[tokio::test]
async fn create_category_should_return_requested_appearance() -> TestResult {
    let (_directory, server) = server(true)?;
    let created = server
        .create_category(Parameters(CreateCategoryRequest {
            name: "Ideas".to_owned(),
            appearance: Some(CategoryAppearance {
                icon: CategoryIcon::Lightbulb,
                color: CategoryColor::Yellow,
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
            appearance: CategoryAppearance {
                icon: CategoryIcon::Heart,
                color: CategoryColor::Rose,
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
            revision: Revision(revision),
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
            note_id: NoteId::new(),
            revision: Revision(1),
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
        .set_note_favorite(Parameters(SetNoteFavoriteRequest {
            note_id: NoteId::new(),
            revision: Revision(1),
            favorite: true,
        }))
        .await
        .err()
        .ok_or_else(|| "read-only server should reject favorite writes".to_owned())?;
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

#[test]
fn category_request_should_preserve_appearance_wire_format() -> TestResult {
    let request: CreateCategoryRequest = serde_json::from_value(serde_json::json!({
        "name": "Ideas",
        "appearance": { "icon": "Lightbulb", "color": "Yellow" }
    }))
    .map_err(|error| error.to_string())?;
    assert_eq!(
        request.appearance,
        Some(CategoryAppearance {
            icon: CategoryIcon::Lightbulb,
            color: CategoryColor::Yellow,
        })
    );
    Ok(())
}

#[test]
fn category_request_schema_should_expose_shared_appearance_choices() -> TestResult {
    let schema = serde_json::to_value(schemars::schema_for!(CreateCategoryRequest))
        .map_err(|error| error.to_string())?;
    let definitions = &schema["$defs"];
    assert_eq!(
        definitions["CategoryAppearance"]["properties"]["icon"]["$ref"],
        "#/$defs/CategoryIcon"
    );
    assert_eq!(
        definitions["CategoryAppearance"]["properties"]["color"]["$ref"],
        "#/$defs/CategoryColor"
    );
    for (name, expected) in [("CategoryIcon", "Lightbulb"), ("CategoryColor", "Yellow")] {
        let variants = definitions[name]["oneOf"]
            .as_array()
            .ok_or_else(|| format!("{name} schema did not expose variants"))?;
        assert!(variants.iter().any(|variant| variant["const"] == expected));
    }
    Ok(())
}

#[test]
fn save_request_should_preserve_id_and_revision_wire_format() -> TestResult {
    let note_id = NoteId::new();
    let request: SaveNoteRequest = serde_json::from_value(serde_json::json!({
        "note_id": note_id.to_string(),
        "revision": 7,
        "source": "Hello"
    }))
    .map_err(|error| error.to_string())?;
    assert_eq!((request.note_id, request.revision), (note_id, Revision(7)));
    Ok(())
}

#[test]
fn list_request_should_accept_optional_category_uuid() -> TestResult {
    let category_id = CategoryId::new();
    let request: ListNotesRequest = serde_json::from_value(serde_json::json!({
        "category_id": category_id.to_string()
    }))
    .map_err(|error| error.to_string())?;
    assert_eq!(request.category_id, Some(category_id));
    let request: ListNotesRequest =
        serde_json::from_str("{}").map_err(|error| error.to_string())?;
    assert_eq!(request.category_id, None);
    Ok(())
}

#[test]
fn note_request_should_reject_invalid_uuid() {
    assert!(
        serde_json::from_value::<NoteRequest>(serde_json::json!({
            "note_id": "not-a-uuid"
        }))
        .is_err()
    );
}

#[test]
fn search_request_should_reject_invalid_category_uuid() {
    assert!(
        serde_json::from_value::<SearchRequest>(serde_json::json!({
            "query": "anything",
            "category_id": "not-a-uuid"
        }))
        .is_err()
    );
}

#[test]
fn request_schemas_should_describe_uuid_ids_and_integer_revisions() -> TestResult {
    let save = serde_json::to_value(schemars::schema_for!(SaveNoteRequest))
        .map_err(|error| error.to_string())?;
    assert_eq!(save["$defs"]["NoteId"]["type"], "string");
    assert_eq!(save["$defs"]["NoteId"]["format"], "uuid");
    assert_eq!(save["$defs"]["Revision"]["type"], "integer");
    let list = serde_json::to_value(schemars::schema_for!(ListNotesRequest))
        .map_err(|error| error.to_string())?;
    assert_eq!(list["$defs"]["CategoryId"]["type"], "string");
    assert_eq!(list["$defs"]["CategoryId"]["format"], "uuid");
    Ok(())
}
