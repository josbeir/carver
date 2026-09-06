//! One mode-neutral formatting toolbar for Rich and Source editing.

use std::{
    cell::{Cell, RefCell},
    collections::BTreeSet,
    rc::Rc,
};

use carver_config::EditorMode;
use carver_domain::source_analysis::SourceContext;
use carver_editor_protocol::{EditorCommand, SelectionState};
use gtk::prelude::*;
use libadwaita as adw;

use crate::{
    formatting,
    mvu::{AppDispatcher, AppMsg, EditorMsg, SourceCommand},
};

use super::{RichEditor, focus::EditorFocusRestorer, source_commands};

/// A formatting operation understood by both editor projections.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) enum ToolbarCommand {
    Bold,
    Italic,
    Strike,
    Underline,
    Highlight,
    Superscript,
    Subscript,
    InlineCode,
    CodeBlock,
    BulletList,
    OrderedList,
    TaskList,
    Link,
}

impl ToolbarCommand {
    fn rich_name(self) -> &'static str {
        match self {
            Self::Bold => "bold",
            Self::Italic => "italic",
            Self::Strike => "strike",
            Self::Underline => "underline",
            Self::Highlight => "highlight",
            Self::Superscript => "superscript",
            Self::Subscript => "subscript",
            Self::InlineCode => "inline-code",
            Self::CodeBlock => "code-block",
            Self::BulletList => "bullet-list",
            Self::OrderedList => "ordered-list",
            Self::TaskList => "task-list",
            Self::Link => "link",
        }
    }
}

/// Mode-neutral visual state for the shared controls.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct ToolbarState {
    active: BTreeSet<ToolbarCommand>,
    heading: u8,
    in_table: bool,
    image: ImageState,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum ImageState {
    #[default]
    None,
    Original,
    Width(u8),
}

impl ToolbarState {
    pub(crate) fn activate(&mut self, command: ToolbarCommand) {
        self.active.insert(command);
    }

    pub(crate) fn set_heading(&mut self, heading: u8) {
        self.heading = heading;
    }

    pub(crate) fn set_table(&mut self, in_table: bool) {
        self.in_table = in_table;
    }

    pub(crate) fn set_image_width(&mut self, image_width: Option<u8>) {
        self.image = image_width.map_or(ImageState::Original, ImageState::Width);
    }

    pub(crate) fn is_active(&self, command: ToolbarCommand) -> bool {
        self.active.contains(&command)
    }

    #[cfg(test)]
    pub(crate) fn heading(&self) -> u8 {
        self.heading
    }

    #[cfg(test)]
    pub(crate) fn image_width(&self) -> Option<u8> {
        match self.image {
            ImageState::None | ImageState::Original => None,
            ImageState::Width(width) => Some(width),
        }
    }

    fn from_rich(selection: &SelectionState) -> Self {
        let mut state = Self::default();
        for command in COMMANDS {
            if selection
                .active
                .iter()
                .any(|name| name == command.command.rich_name())
            {
                state.activate(command.command);
            }
        }
        state.set_heading(selection.heading);
        state.set_table(selection.active.iter().any(|name| name == "table"));
        if selection.active.iter().any(|name| name == "image") {
            state.set_image_width(
                selection
                    .image_width
                    .and_then(|width| (width != 0).then_some(width)),
            );
        }
        state
    }
}

#[derive(Clone, Copy)]
struct CommandSpec {
    command: ToolbarCommand,
    id: &'static str,
    icon: &'static str,
    tooltip: &'static str,
}

const COMMANDS: [CommandSpec; 13] = [
    CommandSpec {
        command: ToolbarCommand::Bold,
        id: "format-bold-button",
        icon: "format-text-bold-symbolic",
        tooltip: "Bold (Ctrl+B)",
    },
    CommandSpec {
        command: ToolbarCommand::Italic,
        id: "format-italic-button",
        icon: "format-text-italic-symbolic",
        tooltip: "Italic (Ctrl+I)",
    },
    CommandSpec {
        command: ToolbarCommand::Strike,
        id: "format-strike-button",
        icon: "format-text-strikethrough-symbolic",
        tooltip: "Strikethrough (Ctrl+Shift+X)",
    },
    CommandSpec {
        command: ToolbarCommand::Underline,
        id: "format-underline-button",
        icon: "format-text-underline-symbolic",
        tooltip: "Underline (Ctrl+U)",
    },
    CommandSpec {
        command: ToolbarCommand::Highlight,
        id: "format-highlight-button",
        icon: "format-text-highlight-symbolic",
        tooltip: "Highlight (Ctrl+Shift+H)",
    },
    CommandSpec {
        command: ToolbarCommand::Superscript,
        id: "format-superscript-button",
        icon: "format-text-superscript-symbolic",
        tooltip: "Superscript (Ctrl+Shift+.)",
    },
    CommandSpec {
        command: ToolbarCommand::Subscript,
        id: "format-subscript-button",
        icon: "format-text-subscript-symbolic",
        tooltip: "Subscript (Ctrl+Shift+,)",
    },
    CommandSpec {
        command: ToolbarCommand::InlineCode,
        id: "format-code-button",
        icon: "text-editor-symbolic",
        tooltip: "Inline code",
    },
    CommandSpec {
        command: ToolbarCommand::CodeBlock,
        id: "format-code-block-button",
        icon: "utilities-terminal-symbolic",
        tooltip: "Code block",
    },
    CommandSpec {
        command: ToolbarCommand::BulletList,
        id: "format-bullet-button",
        icon: "view-list-bullet-symbolic",
        tooltip: "Bulleted list (Ctrl+Shift+8)",
    },
    CommandSpec {
        command: ToolbarCommand::OrderedList,
        id: "format-ordered-button",
        icon: "view-list-ordered-symbolic",
        tooltip: "Numbered list (Ctrl+Shift+7)",
    },
    CommandSpec {
        command: ToolbarCommand::TaskList,
        id: "format-task-button",
        icon: "object-select-symbolic",
        tooltip: "Task list",
    },
    CommandSpec {
        command: ToolbarCommand::Link,
        id: "format-link-button",
        icon: "insert-link-symbolic",
        tooltip: "Insert link",
    },
];

#[derive(Clone)]
struct CommandRouter {
    mode: Rc<Cell<EditorMode>>,
    source: gtk::TextBuffer,
    rich: RichEditor,
    dispatcher: AppDispatcher,
    toast_overlay: adw::ToastOverlay,
    focus: EditorFocusRestorer,
}

impl CommandRouter {
    fn execute(&self, command: ToolbarCommand, anchor: &gtk::Widget) {
        match self.mode.get() {
            EditorMode::Source => self.execute_source(command, anchor),
            EditorMode::Rich => self.execute_rich(command, anchor),
            EditorMode::Rendered => {}
        }
    }

    fn execute_source(&self, command: ToolbarCommand, anchor: &gtk::Widget) {
        match command {
            ToolbarCommand::Link => {
                formatting::show_source_link_dialog(
                    anchor,
                    &self.source,
                    &self.dispatcher,
                    &self.focus,
                );
                return;
            }
            _ => self.dispatch_source_command(source_command(command)),
        }
        self.focus.restore_later();
    }

    fn dispatch_source_command(&self, command: SourceCommand) {
        let selection = source_commands::selection_from_buffer(&self.source);
        let _ = self
            .dispatcher
            .dispatch(AppMsg::Editor(EditorMsg::ApplySourceCommand {
                command,
                selection,
            }));
    }

    fn execute_rich(&self, command: ToolbarCommand, anchor: &gtk::Widget) {
        if command == ToolbarCommand::Link {
            self.rich
                .show_link_dialog(anchor, &self.dispatcher, &self.focus);
        } else {
            let _ = self
                .dispatcher
                .dispatch(AppMsg::Editor(EditorMsg::ApplyRichCommand(
                    EditorCommand::Named(command.rich_name().to_owned()),
                )));
            self.focus.restore_later();
        }
    }

    fn set_heading(&self, level: u8) {
        match self.mode.get() {
            EditorMode::Source => self.dispatch_source_command(SourceCommand::SetHeading(level)),
            EditorMode::Rich => {
                let _ = self
                    .dispatcher
                    .dispatch(AppMsg::Editor(EditorMsg::ApplyRichCommand(
                        EditorCommand::Heading(level),
                    )));
            }
            EditorMode::Rendered => {}
        }
        self.focus.restore_later();
    }

    fn insert_table(&self, rows: u8, columns: u8, header: bool) {
        match self.mode.get() {
            EditorMode::Source => self.dispatch_source_command(SourceCommand::InsertTable {
                rows,
                columns,
                header,
            }),
            EditorMode::Rich => {
                let _ = self
                    .dispatcher
                    .dispatch(AppMsg::Editor(EditorMsg::ApplyRichCommand(
                        EditorCommand::InsertTable {
                            rows,
                            columns,
                            header,
                        },
                    )));
            }
            EditorMode::Rendered => {}
        }
        self.focus.restore_later();
    }

    fn image_width(&self, width: Option<u8>) {
        match self.mode.get() {
            EditorMode::Source => self.dispatch_source_command(SourceCommand::SetImageWidth(width)),
            EditorMode::Rich => {
                let _ = self
                    .dispatcher
                    .dispatch(AppMsg::Editor(EditorMsg::ApplyRichCommand(
                        EditorCommand::ImageWidth(width),
                    )));
            }
            EditorMode::Rendered => {}
        }
        self.focus.restore_later();
    }

    fn choose_image(&self, button: &gtk::Button) {
        let source_target = (self.mode.get() == EditorMode::Source)
            .then(|| source_commands::image_target_from_buffer(&self.source));
        formatting::choose_managed_image(
            button,
            &self.dispatcher,
            &self.toast_overlay,
            source_target,
            &self.focus,
        );
    }
}

fn source_command(command: ToolbarCommand) -> SourceCommand {
    match command {
        ToolbarCommand::Bold => SourceCommand::ToggleInline {
            opening: String::from("*"),
            closing: String::from("*"),
        },
        ToolbarCommand::Italic => SourceCommand::ToggleInline {
            opening: String::from("/"),
            closing: String::from("/"),
        },
        ToolbarCommand::Strike => SourceCommand::ToggleInline {
            opening: String::from("~"),
            closing: String::from("~"),
        },
        ToolbarCommand::Underline => SourceCommand::ToggleInline {
            opening: String::from("_"),
            closing: String::from("_"),
        },
        ToolbarCommand::Highlight => SourceCommand::ToggleInline {
            opening: String::from("="),
            closing: String::from("="),
        },
        ToolbarCommand::Superscript => SourceCommand::ToggleInline {
            opening: String::from("{^"),
            closing: String::from("^}"),
        },
        ToolbarCommand::Subscript => SourceCommand::ToggleInline {
            opening: String::from("{,"),
            closing: String::from(",}"),
        },
        ToolbarCommand::InlineCode => SourceCommand::ToggleInline {
            opening: String::from("`"),
            closing: String::from("`"),
        },
        ToolbarCommand::CodeBlock => SourceCommand::ToggleCodeBlock,
        ToolbarCommand::BulletList => SourceCommand::ToggleList(String::from("- ")),
        ToolbarCommand::OrderedList => SourceCommand::ToggleOrderedList,
        ToolbarCommand::TaskList => SourceCommand::ToggleList(String::from("- [ ] ")),
        ToolbarCommand::Link => unreachable!("link input is collected by its native dialog"),
    }
}

/// The one mounted formatting toolbar and its mode router.
#[derive(Clone)]
pub(crate) struct Toolbar {
    widget: gtk::Box,
    router: CommandRouter,
    command_buttons: Vec<(ToolbarCommand, gtk::ToggleButton)>,
    heading: gtk::MenuButton,
    heading_choices: Vec<(u8, gtk::ToggleButton)>,
    table: gtk::MenuButton,
    image: gtk::MenuButton,
    image_width_choices: Vec<(Option<u8>, gtk::ToggleButton)>,
    source_path: gtk::Label,
    source_context: Rc<RefCell<Option<SourceContext>>>,
}

impl Toolbar {
    pub(crate) fn new(
        source: &gtk::TextView,
        rich: &RichEditor,
        dispatcher: &AppDispatcher,
        toast_overlay: &adw::ToastOverlay,
    ) -> Self {
        let widget = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        widget.set_widget_name("formatting-toolbar");
        widget.add_css_class("toolbar");
        widget.set_margin_start(12);
        widget.set_margin_end(12);
        widget.set_margin_top(6);
        widget.set_margin_bottom(6);
        let mode = Rc::new(Cell::new(EditorMode::Rich));
        let router = CommandRouter {
            mode: Rc::clone(&mode),
            source: source.buffer(),
            rich: rich.clone(),
            dispatcher: dispatcher.clone(),
            toast_overlay: toast_overlay.clone(),
            focus: EditorFocusRestorer::new(mode, source, rich.view()),
        };
        let mut command_buttons = Vec::new();
        for spec in COMMANDS {
            let button = gtk::ToggleButton::new();
            set_toolbar_icon(&button, spec.icon);
            button.set_widget_name(spec.id);
            button.set_tooltip_text(Some(spec.tooltip));
            button.add_css_class("flat");
            let router = router.clone();
            button.connect_clicked(move |button| router.execute(spec.command, button.upcast_ref()));
            widget.append(&button);
            command_buttons.push((spec.command, button));
        }
        let (heading, heading_choices) = append_heading_menu(&widget, &router);
        let table = append_table_menu(&widget, &router);
        let (image, image_width_choices) = append_image_menu(&widget, &router);
        let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        spacer.set_hexpand(true);
        widget.append(&spacer);
        let source_path = gtk::Label::new(None);
        source_path.set_widget_name("source-ast-path");
        source_path.add_css_class("dim-label");
        source_path.set_tooltip_text(Some("Carve AST context"));
        source_path.set_visible(false);
        widget.append(&source_path);
        Self {
            widget,
            router,
            command_buttons,
            heading,
            heading_choices,
            table,
            image,
            image_width_choices,
            source_path,
            source_context: Rc::new(RefCell::new(None)),
        }
    }

    pub(crate) fn widget(&self) -> &gtk::Box {
        &self.widget
    }

    pub(crate) fn set_mode(&self, mode: EditorMode) {
        self.router.mode.set(mode);
        self.widget.set_sensitive(mode != EditorMode::Rendered);
        if mode == EditorMode::Source {
            self.apply_source_context(self.source_context.borrow().as_ref());
        } else {
            self.source_path.set_visible(false);
        }
    }

    pub(crate) fn set_rich_selection(&self, selection: &SelectionState) {
        if self.router.mode.get() == EditorMode::Rich {
            self.set_state(&ToolbarState::from_rich(selection));
        }
    }

    /// Routes a Source-mode shortcut through the same command adapter as a toolbar click.
    pub(crate) fn execute_source_shortcut(&self, command: ToolbarCommand, anchor: &gtk::Widget) {
        if self.router.mode.get() == EditorMode::Source {
            self.router.execute(command, anchor);
        }
    }

    /// Routes a source-only editing command through the MVU reducer.
    pub(crate) fn execute_source_command(&self, command: SourceCommand) {
        if self.router.mode.get() == EditorMode::Source {
            self.router.dispatch_source_command(command);
        }
    }

    /// Applies source analysis produced from the shared cached Carve parse.
    pub(crate) fn set_source_context(&self, context: Option<SourceContext>) {
        self.source_context.replace(context);
        if self.router.mode.get() == EditorMode::Source {
            self.apply_source_context(self.source_context.borrow().as_ref());
        }
    }

    fn apply_source_context(&self, context: Option<&SourceContext>) {
        self.set_state(&source_commands::toolbar_state_from_context(
            context.cloned(),
        ));
        let breadcrumb = context.map(SourceContext::breadcrumb);
        self.source_path
            .set_text(breadcrumb.as_deref().unwrap_or_default());
        self.source_path.set_visible(breadcrumb.is_some());
    }

    fn set_state(&self, state: &ToolbarState) {
        for (command, button) in &self.command_buttons {
            button.set_active(state.is_active(*command));
        }
        set_context_active(&self.heading, state.heading != 0);
        for (level, choice) in &self.heading_choices {
            choice.set_active(*level == state.heading);
        }
        set_context_active(&self.table, state.in_table);
        set_context_active(&self.image, state.image != ImageState::None);
        for (width, choice) in &self.image_width_choices {
            choice.set_active(match (state.image, width) {
                (ImageState::Original, None) => true,
                (ImageState::Width(current), Some(width)) => current == *width,
                _ => false,
            });
        }
    }
}

fn append_heading_menu(
    toolbar: &gtk::Box,
    router: &CommandRouter,
) -> (gtk::MenuButton, Vec<(u8, gtk::ToggleButton)>) {
    let menu = gtk::MenuButton::new();
    menu.set_widget_name("format-heading-button");
    menu.set_icon_name("format-text-rich-symbolic");
    menu.set_tooltip_text(Some("Text style"));
    menu.add_css_class("flat");
    let choices = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let mut active_choices = Vec::new();
    for (label, level) in [
        ("Normal text", 0),
        ("Heading 1", 1),
        ("Heading 2", 2),
        ("Heading 3", 3),
        ("Heading 4", 4),
        ("Heading 5", 5),
        ("Heading 6", 6),
    ] {
        let choice = gtk::ToggleButton::with_label(label);
        choice.add_css_class("flat");
        let router = router.clone();
        choice.connect_clicked(move |_| router.set_heading(level));
        choices.append(&choice);
        active_choices.push((level, choice));
    }
    let popover = gtk::Popover::new();
    popover.set_child(Some(&choices));
    menu.set_popover(Some(&popover));
    toolbar.append(&menu);
    (menu, active_choices)
}

fn append_table_menu(toolbar: &gtk::Box, router: &CommandRouter) -> gtk::MenuButton {
    let router = router.clone();
    formatting::append_table_picker(
        toolbar,
        "format-table-button",
        move |rows, columns, header| {
            router.insert_table(rows, columns, header);
        },
    )
}

fn append_image_menu(
    toolbar: &gtk::Box,
    router: &CommandRouter,
) -> (gtk::MenuButton, Vec<(Option<u8>, gtk::ToggleButton)>) {
    let menu = gtk::MenuButton::new();
    menu.set_widget_name("format-image-button");
    menu.set_icon_name("image-x-generic-symbolic");
    menu.set_tooltip_text(Some("Insert or resize image"));
    menu.add_css_class("flat");
    let choices = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let insert = gtk::Button::with_label("Insert image…");
    insert.add_css_class("flat");
    let router_for_insert = router.clone();
    insert.connect_clicked(move |button| router_for_insert.choose_image(button));
    choices.append(&insert);
    choices.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
    let mut active_choices = Vec::new();
    for (label, width) in [
        ("Original size", None),
        ("25%", Some(25)),
        ("50%", Some(50)),
        ("75%", Some(75)),
        ("100%", Some(100)),
    ] {
        let choice = gtk::ToggleButton::with_label(label);
        choice.add_css_class("flat");
        let router = router.clone();
        choice.connect_clicked(move |_| router.image_width(width));
        choices.append(&choice);
        active_choices.push((width, choice));
    }
    let popover = gtk::Popover::new();
    popover.set_child(Some(&choices));
    menu.set_popover(Some(&popover));
    toolbar.append(&menu);
    (menu, active_choices)
}

fn set_toolbar_icon(button: &gtk::ToggleButton, icon_name: &str) {
    let has_icon = gtk::gdk::Display::default()
        .is_some_and(|display| gtk::IconTheme::for_display(&display).has_icon(icon_name));
    if has_icon {
        button.set_icon_name(icon_name);
    } else if let Some(glyph) = toolbar_fallback_glyph(icon_name) {
        let label = gtk::Label::new(Some(glyph));
        label.set_width_chars(2);
        label.set_max_width_chars(2);
        label.add_css_class("format-fallback-glyph");
        button.set_child(Some(&label));
        button.add_css_class("format-fallback-button");
        button.add_css_class("image-button");
    } else {
        button.set_icon_name(icon_name);
    }
}

fn toolbar_fallback_glyph(icon_name: &str) -> Option<&'static str> {
    match icon_name {
        "format-text-highlight-symbolic" => Some("H"),
        "format-text-superscript-symbolic" => Some("Aˣ"),
        "format-text-subscript-symbolic" => Some("Aₓ"),
        _ => None,
    }
}

fn set_context_active(menu: &gtk::MenuButton, active: bool) {
    if active {
        menu.add_css_class("context-active");
    } else {
        menu.remove_css_class("context-active");
    }
}
