use carver_editor_protocol::{SelectionState, TableSelection};

use super::{SourceCommand, ToolbarCommand, ToolbarState, source_command};

#[test]
fn source_command_should_reject_commands_that_require_native_input() {
    assert!(source_command(ToolbarCommand::Link).is_none());
}

#[test]
fn task_list_command_should_use_canonical_unchecked_markers_in_source() {
    assert_eq!(
        source_command(ToolbarCommand::TaskList),
        Some(SourceCommand::ToggleList(String::from("- [ ] ")))
    );
}

#[test]
fn rich_task_selection_should_activate_the_task_list_control() {
    let selection = SelectionState {
        active: vec![String::from("task-list")],
        ..SelectionState::default()
    };
    assert!(ToolbarState::from_rich(&selection).is_active(ToolbarCommand::TaskList));
}

#[test]
fn rich_table_selection_should_expose_live_dimensions() {
    let selection = SelectionState {
        active: vec![String::from("table")],
        table: Some(TableSelection {
            rows: 3,
            columns: 4,
            header: false,
        }),
        ..SelectionState::default()
    };
    let state = ToolbarState::from_rich(&selection);
    assert!(state.in_table);
    assert_eq!(
        state.table,
        Some(TableSelection {
            rows: 3,
            columns: 4,
            header: false,
        })
    );
}
