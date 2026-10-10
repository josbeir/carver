//! Localized command metadata at the GTK boundary.

use crate::mvu::{
    FormatCommand as F,
    palette::{CommandId as C, CommandLabel},
};
use carver_config::EditorMode;
use gettextrs::gettext;

fn label(command: C, title: String, shortcut: &str, icon: &str) -> CommandLabel {
    CommandLabel {
        command,
        title,
        aliases: String::new(),
        icon: icon.into(),
        shortcut: shortcut.into(),
    }
}

pub(super) fn labels() -> Vec<CommandLabel> {
    let mut labels = Vec::new();
    labels.extend(formatting());
    labels.extend(insertion());
    labels.extend(editor());
    labels.extend(note());
    labels.extend(browser());
    labels.extend(base());
    labels.extend(creation());
    labels.extend(application());
    labels.extend(tabs());
    labels.push(label(
        C::Heading(0),
        gettext("Paragraph"),
        "",
        "format-text-rich-symbolic",
    ));
    for level in 1..=6 {
        labels.push(label(
            C::Heading(level),
            tr_fmt!(gettext("Heading {level}"), level = level),
            "",
            "format-text-rich-symbolic",
        ));
    }
    labels.push(label(
        C::ImageWidth(None),
        gettext("Original size"),
        "",
        "image-x-generic-symbolic",
    ));
    for width in [25, 50, 75, 100] {
        labels.push(label(
            C::ImageWidth(Some(width)),
            tr_fmt!(gettext("Image width: {width}%"), width = width),
            "",
            "image-x-generic-symbolic",
        ));
    }
    labels
}

fn formatting() -> Vec<CommandLabel> {
    vec![
        label(
            C::Format(F::Bold),
            gettext("Bold"),
            "<Control>b",
            "format-text-bold-symbolic",
        ),
        label(
            C::Format(F::Italic),
            gettext("Italic"),
            "<Control>i",
            "format-text-italic-symbolic",
        ),
        label(
            C::Format(F::Underline),
            gettext("Underline"),
            "<Control>u",
            "format-text-underline-symbolic",
        ),
        label(
            C::Format(F::Strike),
            gettext("Strikethrough"),
            "<Control><Shift>x",
            "format-text-strikethrough-symbolic",
        ),
        label(
            C::Format(F::Highlight),
            gettext("Highlight"),
            "<Control><Shift>h",
            "format-text-highlight-symbolic",
        ),
        label(
            C::Format(F::Superscript),
            gettext("Superscript"),
            "<Control><Shift>period",
            "format-text-superscript-symbolic",
        ),
        label(
            C::Format(F::Subscript),
            gettext("Subscript"),
            "<Control><Shift>comma",
            "format-text-subscript-symbolic",
        ),
        label(
            C::Format(F::InlineCode),
            gettext("Inline code"),
            "",
            "text-editor-symbolic",
        ),
        label(
            C::Format(F::CodeBlock),
            gettext("Code block"),
            "",
            "utilities-terminal-symbolic",
        ),
        label(
            C::Format(F::BlockQuote),
            gettext("Block quote"),
            "",
            "format-text-quote-symbolic",
        ),
        label(
            C::Format(F::BulletList),
            gettext("Bulleted list"),
            "<Control><Shift>8",
            "view-list-bullet-symbolic",
        ),
        label(
            C::Format(F::OrderedList),
            gettext("Numbered list"),
            "<Control><Shift>7",
            "view-list-ordered-symbolic",
        ),
        label(
            C::Format(F::TaskList),
            gettext("Task list"),
            "",
            "carver-list-todo-symbolic",
        ),
    ]
}

fn insertion() -> Vec<CommandLabel> {
    vec![
        label(C::Link, gettext("Insert link"), "", "insert-link-symbolic"),
        label(C::Table, gettext("Insert table"), "", "view-grid-symbolic"),
        label(
            C::Image,
            gettext("Insert image…"),
            "",
            "image-x-generic-symbolic",
        ),
        label(
            C::Attachment,
            gettext("Add files"),
            "",
            "mail-attachment-symbolic",
        ),
        label(
            C::InsertTemplate,
            gettext("Insert Template…"),
            "",
            "document-new-symbolic",
        ),
    ]
}

fn editor() -> Vec<CommandLabel> {
    vec![
        label(
            C::Find,
            gettext("Find in note"),
            "<Control>f",
            "system-search-symbolic",
        ),
        label(
            C::Mode(EditorMode::Rich),
            gettext("Edit"),
            "",
            "document-edit-symbolic",
        ),
        label(
            C::Mode(EditorMode::Source),
            gettext("Source"),
            "",
            "text-x-generic-symbolic",
        ),
        label(
            C::Mode(EditorMode::Rendered),
            gettext("Preview"),
            "",
            "view-reveal-symbolic",
        ),
        label(
            C::Sidebar,
            gettext("Toggle document sidebar"),
            "<Control><Shift>l",
            "sidebar-show-symbolic",
        ),
        label(
            C::SplitPreview,
            gettext("Show rendered preview"),
            "",
            "view-dual-symbolic",
        ),
    ]
}

fn note() -> Vec<CommandLabel> {
    vec![
        label(
            C::Properties,
            gettext("Document properties…"),
            "",
            "document-properties-symbolic",
        ),
        label(
            C::Favorite,
            gettext("Toggle favorite"),
            "<Control><Shift>f",
            "starred-symbolic",
        ),
        label(C::Copy, gettext("Copy note"), "", "edit-copy-symbolic"),
        label(C::Move, gettext("Move note…"), "", "folder-symbolic"),
        label(
            C::Export,
            gettext("Export note…"),
            "<Control>e",
            "document-save-as-symbolic",
        ),
        label(
            C::Print,
            gettext("Print…"),
            "<Control>p",
            "document-print-symbolic",
        ),
        label(
            C::SaveTemplate,
            gettext("Save as Template…"),
            "",
            "document-save-as-symbolic",
        ),
        label(
            C::TrashNote,
            gettext("Move note to Trash"),
            "<Control>d",
            "user-trash-symbolic",
        ),
        label(
            C::RetrySave,
            gettext("Retry save"),
            "",
            "view-refresh-symbolic",
        ),
    ]
}

fn browser() -> Vec<CommandLabel> {
    vec![
        label(
            C::SearchNotes,
            gettext("Search notes"),
            "<Control>f",
            "system-search-symbolic",
        ),
        label(
            C::RefreshBrowser,
            gettext("Refresh notes"),
            "",
            "view-refresh-symbolic",
        ),
        label(
            C::Import,
            gettext("Import note"),
            "<Control>o",
            "document-open-symbolic",
        ),
        label(
            C::Clipboard,
            gettext("New from clipboard"),
            "<Control><Alt>n",
            "edit-paste-symbolic",
        ),
        label(
            C::MarkdownClipboard,
            gettext("New from Markdown clipboard"),
            "",
            "edit-paste-symbolic",
        ),
    ]
}

fn base() -> Vec<CommandLabel> {
    vec![
        label(
            C::SearchBase,
            gettext("Search Base rows"),
            "<Control>f",
            "system-search-symbolic",
        ),
        label(
            C::ConfigureBase,
            gettext("Configure Base"),
            "",
            "emblem-system-symbolic",
        ),
        label(
            C::RefreshBase,
            gettext("Refresh Base"),
            "",
            "view-refresh-symbolic",
        ),
        label(
            C::CloseBaseTabs,
            gettext("Close tabs"),
            "",
            "window-close-symbolic",
        ),
        label(
            C::DeleteBase,
            gettext("Delete Base"),
            "",
            "user-trash-symbolic",
        ),
    ]
}

fn creation() -> Vec<CommandLabel> {
    vec![
        label(
            C::NewNote,
            gettext("New note"),
            "<Control>n",
            "document-new-symbolic",
        ),
        label(
            C::BlankNote,
            gettext("New blank note"),
            "",
            "document-new-symbolic",
        ),
        label(
            C::TemplateNote,
            gettext("New from Template…"),
            "<Control><Shift>n",
            "document-new-symbolic",
        ),
        label(
            C::NewCategory,
            gettext("New category"),
            "",
            "folder-new-symbolic",
        ),
        label(C::NewBase, gettext("New Base"), "", "view-grid-symbolic"),
    ]
}

fn application() -> Vec<CommandLabel> {
    vec![
        label(
            C::RefreshTrash,
            gettext("Refresh Trash"),
            "",
            "view-refresh-symbolic",
        ),
        label(
            C::UndoTrash,
            gettext("Undo move to Trash"),
            "",
            "edit-undo-symbolic",
        ),
        label(C::Notes, gettext("Notes"), "", "view-list-symbolic"),
        label(C::Trash, gettext("Trash"), "", "user-trash-symbolic"),
        label(
            C::Templates,
            gettext("Templates…"),
            "",
            "document-open-symbolic",
        ),
        label(
            C::Preferences,
            gettext("Preferences"),
            "",
            "preferences-system-symbolic",
        ),
        label(
            C::Shortcuts,
            gettext("Keyboard Shortcuts"),
            "<Control>question",
            "preferences-desktop-keyboard-shortcuts-symbolic",
        ),
        label(
            C::Agent,
            gettext("Connect an agent"),
            "",
            "system-run-symbolic",
        ),
        label(C::About, gettext("About Carver"), "", "help-about-symbolic"),
    ]
}

fn tabs() -> Vec<CommandLabel> {
    vec![
        label(
            C::NextTab,
            gettext("Next tab"),
            "<Control>Tab",
            "go-next-symbolic",
        ),
        label(
            C::PreviousTab,
            gettext("Previous tab"),
            "<Control><Shift>Tab",
            "go-previous-symbolic",
        ),
        label(
            C::CloseTab,
            gettext("Close tab"),
            "<Control>w",
            "window-close-symbolic",
        ),
    ]
}
