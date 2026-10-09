//! Display-backed task-list toolbar visibility and canonical source coverage.
use super::*;

pub(super) fn task_button_should_toggle_source_lists_at_each_width(
    fixture: &WindowFixture,
) -> TestResult {
    let root = fixture.root()?;
    let source = fixture.source()?;
    let buffer = source.buffer();
    let modes = fixture.editor_mode_stack()?;
    modes.set_visible_child_name("source");
    for (width, name) in [
        (360, "formatting-toolbar-compact"),
        (1120, "formatting-toolbar-desktop"),
    ] {
        fixture.window.set_default_size(width, 760);
        let toolbar = widget_as::<gtk::Box>(&root, name).ok_or(name)?;
        assert!(run_main_context_until(|| toolbar.is_mapped()));
        let task = widget_as::<gtk::ToggleButton>(toolbar.upcast_ref(), "format-task-button")
            .ok_or("visible task-list button")?;
        assert!(task.is_mapped());
        assert_eq!(
            task.icon_name().as_deref(),
            Some("carver-list-todo-symbolic")
        );
        if width == 360 {
            assert_eq!(task.parent().as_ref(), Some(toolbar.upcast_ref()));
            assert!(
                toolbar.width() <= 360,
                "compact toolbar should fit at 360px"
            );
            let more =
                widget_as::<gtk::MenuButton>(toolbar.upcast_ref(), "formatting-toolbar-more")
                    .and_then(|button| button.popover())
                    .and_then(|popover| popover.child())
                    .ok_or("more formatting controls")?;
            assert!(widget_as::<gtk::ToggleButton>(&more, "format-task-button").is_none());
        }
        buffer.set_text("First\nSecond");
        select_all(&buffer);
        task.emit_clicked();
        assert!(run_main_context_until(|| {
            buffer.text(&buffer.start_iter(), &buffer.end_iter(), false)
                == "- [ ] First\n- [ ] Second"
        }));
        buffer.place_cursor(&buffer.iter_at_offset(7));
        assert!(run_main_context_until(|| task.is_active()));
        assert_source_focus_restored(&source, "task list")?;
        select_all(&buffer);
        task.emit_clicked();
        assert!(run_main_context_until(|| {
            buffer.text(&buffer.start_iter(), &buffer.end_iter(), false) == "First\nSecond"
                && !task.is_active()
        }));
    }
    Ok(())
}

pub(super) fn task_button_should_toggle_rich_lists_and_preserve_checked_items(
    fixture: &WindowFixture,
) -> TestResult {
    let root = fixture.root()?;
    let source = fixture.source()?;
    let buffer = source.buffer();
    let modes = fixture.editor_mode_stack()?;
    let rich = widget_as::<webkit6::WebView>(&root, "rich-editor").ok_or("rich editor")?;
    for (width, name) in [
        (360, "formatting-toolbar-compact"),
        (1120, "formatting-toolbar-desktop"),
    ] {
        fixture.window.set_default_size(width, 760);
        let toolbar = widget_as::<gtk::Box>(&root, name).ok_or(name)?;
        assert!(run_main_context_until(|| toolbar.is_mapped()));
        let task = widget_as::<gtk::ToggleButton>(toolbar.upcast_ref(), "format-task-button")
            .ok_or("visible task-list button")?;
        modes.set_visible_child_name("source");
        buffer.set_text("Todo");
        modes.set_visible_child_name("rich");
        assert_web_script_should_be_true(&rich, "window.carverEditor?.source() === 'Todo'");
        assert_web_script_should_be_true(
            &rich,
            "window.carverEditor?.editor?.commands.setTextSelection(1) === true",
        );
        task.emit_clicked();
        assert!(run_main_context_until(|| {
            buffer
                .text(&buffer.start_iter(), &buffer.end_iter(), false)
                .trim()
                == "- [ ] Todo"
                && task.is_active()
                && widget_is_window_focus(rich.upcast_ref())
        }));
        task.emit_clicked();
        assert!(run_main_context_until(|| {
            buffer
                .text(&buffer.start_iter(), &buffer.end_iter(), false)
                .trim()
                == "Todo"
                && !task.is_active()
        }));
        task.emit_clicked();
        assert_web_script_should_be_true(
            &rich,
            "(() => { const checkbox = document.querySelector('ul[data-type=taskList] input[type=checkbox]'); if (!checkbox) return false; if (!checkbox.checked) checkbox.click(); return checkbox.checked; })()",
        );
        assert!(run_main_context_until(|| {
            buffer
                .text(&buffer.start_iter(), &buffer.end_iter(), false)
                .trim()
                == "- [x] Todo"
        }));
        let canonical = buffer.text(&buffer.start_iter(), &buffer.end_iter(), false);
        modes.set_visible_child_name("source");
        assert_eq!(
            buffer.text(&buffer.start_iter(), &buffer.end_iter(), false),
            canonical
        );
        modes.set_visible_child_name("rendered");
        assert!(!toolbar.is_sensitive());
        assert_eq!(
            buffer.text(&buffer.start_iter(), &buffer.end_iter(), false),
            canonical
        );
        modes.set_visible_child_name("rich");
        assert_web_script_should_be_true(
            &rich,
            "document.querySelector('ul[data-type=taskList] input[type=checkbox]')?.checked === true",
        );
        assert_eq!(
            buffer.text(&buffer.start_iter(), &buffer.end_iter(), false),
            canonical
        );
    }
    modes.set_visible_child_name("source");
    Ok(())
}
