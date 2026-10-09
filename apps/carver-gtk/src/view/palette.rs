//! Palette presentation and shared native command adapters.

use super::ViewRefs;
use crate::mvu::{
    AppModel, AppMsg, BasesMsg, BrowserMsg, EditorMsg, Effect, LoadState, NavigationMsg,
    PreferencesMsg, TabsMsg, TemplatesMsg, TrashMsg, palette::CommandId as C,
};
use gtk::prelude::*;
use libadwaita as adw;

impl ViewRefs {
    pub(super) fn render_palette(&self, model: &AppModel) {
        if let Some(parent) = self
            .route_stack
            .root()
            .and_downcast::<adw::ApplicationWindow>()
            && let Some(dispatcher) = &self.dispatcher
        {
            self.palette.render(&parent, dispatcher, model);
        }
    }

    pub(crate) fn run_palette_effect(&self, effect: Effect, model: &AppModel) {
        let Some(dispatcher) = &self.dispatcher else {
            return;
        };
        let Some(parent) = self
            .route_stack
            .root()
            .and_downcast::<adw::ApplicationWindow>()
        else {
            return;
        };
        match effect {
            Effect::OpenPalette => {
                let editor = self.editor.borrow().clone();
                let selection = editor.map_or(0..0, |editor| editor.source_selection());
                self.palette.capture(&parent, dispatcher, selection);
            }
            Effect::FinishPalette { activation } => self.palette.finish(dispatcher, activation),
            Effect::ExecutePaletteCommand {
                command,
                source_selection,
            } => self.execute_palette(&parent, model, command, source_selection),
            _ => {}
        }
    }

    // CONTEXT: Typed native command routing shares existing dialogs and mutation safeguards.
    #[expect(clippy::too_many_lines)]
    fn execute_palette(
        &self,
        parent: &adw::ApplicationWindow,
        model: &AppModel,
        command: C,
        selection: std::ops::Range<usize>,
    ) {
        let Some(dispatcher) = &self.dispatcher else {
            return;
        };
        let message = match command {
            C::Format(_)
            | C::Heading(_)
            | C::ImageWidth(_)
            | C::Link
            | C::Table
            | C::Image
            | C::Attachment
            | C::Find => {
                let editor = self.editor.borrow().clone();
                if let Some(editor) = editor
                    && let Some(document) = &model.editor
                {
                    editor.execute_palette(command, document.session, selection);
                }
                return;
            }
            C::Mode(mode) => AppMsg::Preferences(PreferencesMsg::SetEditorMode(mode)),
            C::SplitPreview => AppMsg::Preferences(PreferencesMsg::SetSourceSplitView(
                !model.preferences.source_split_view,
            )),
            C::Sidebar => AppMsg::Editor(EditorMsg::ToggleDocumentSidebar),
            C::Properties => AppMsg::Editor(EditorMsg::PropertiesDialogRequested),
            C::Favorite => AppMsg::Editor(EditorMsg::ToggleFavorite),
            C::Copy => AppMsg::Editor(EditorMsg::CopyRequested),
            C::Export => AppMsg::Editor(EditorMsg::ExportDialogRequested),
            C::Print => AppMsg::Editor(EditorMsg::PrintRequested),
            C::TrashNote => AppMsg::Editor(EditorMsg::TrashRequested),
            C::RetrySave => AppMsg::Editor(EditorMsg::RetrySave),
            C::SearchNotes => AppMsg::Browser(BrowserMsg::SearchOpened),
            C::RefreshBrowser => AppMsg::Browser(BrowserMsg::Reload),
            C::SearchBase => AppMsg::Bases(BasesMsg::SearchOpened),
            C::ConfigureBase => AppMsg::Bases(BasesMsg::Configure),
            C::RefreshBase => AppMsg::Bases(BasesMsg::Reload),
            C::CloseBaseTabs => {
                if let Some(id) = model.bases.selected {
                    let _ = dispatcher.dispatch(AppMsg::Tabs(TabsMsg::CloseOrigin(
                        crate::mvu::TabOrigin::Base(id),
                    )));
                }
                return;
            }
            C::DeleteBase => {
                if let Some(base) = &self.base {
                    let _ = base.delete.activate_action("base.delete", None);
                }
                return;
            }
            C::Move => {
                if let Some(doc) = &model.editor
                    && let LoadState::Ready(categories) = &model.sidebar.state
                {
                    let _ = crate::ui::dialogs::show_move_note_dialog(
                        Some(parent.upcast_ref()),
                        dispatcher,
                        doc.note_id,
                        doc.category_id,
                        doc.analysis.title(),
                        categories,
                    );
                }
                return;
            }
            C::NewCategory | C::NewBase => {
                if let Some(slot) = &self.add_dialog {
                    crate::ui::add::present(
                        parent.upcast_ref(),
                        dispatcher,
                        slot,
                        command == C::NewBase,
                    );
                }
                return;
            }
            C::RefreshTrash => AppMsg::Trash(TrashMsg::Reload),
            C::UndoTrash => {
                if let Some(id) = model.undo_trash_note {
                    let _ = dispatcher.dispatch(AppMsg::Trash(TrashMsg::RestoreNote(id)));
                }
                return;
            }
            C::Notes => AppMsg::Navigation(NavigationMsg::SelectCategory(None)),
            C::Trash => AppMsg::Navigation(NavigationMsg::ShowTrash),
            C::NewNote => AppMsg::Navigation(NavigationMsg::CreateNote),
            C::BlankNote => AppMsg::Navigation(NavigationMsg::CreateBlankNote),
            C::TemplateNote => AppMsg::Templates(TemplatesMsg::Pick),
            C::Templates => AppMsg::Templates(TemplatesMsg::Manage),
            C::InsertTemplate => {
                let editor = self.editor.borrow().clone();
                if let Some(editor) = editor
                    && let Some(doc) = &model.editor
                    && doc.mode == carver_config::EditorMode::Source
                {
                    editor.select_source_range(doc.session, selection);
                }
                AppMsg::Templates(TemplatesMsg::Insert)
            }
            C::SaveTemplate => AppMsg::Templates(TemplatesMsg::FromEditor),
            C::Import
            | C::Clipboard
            | C::MarkdownClipboard
            | C::Preferences
            | C::Shortcuts
            | C::Agent
            | C::About => {
                let action = match command {
                    C::Import => "win.import-note",
                    C::Clipboard => "win.new-note-from-clipboard",
                    C::MarkdownClipboard => "win.new-note-from-markdown-clipboard",
                    C::Preferences => "win.preferences",
                    C::Shortcuts => "win.keyboard-shortcuts",
                    C::Agent => "win.connect-agent",
                    _ => "win.about",
                };
                let _ = gtk::prelude::WidgetExt::activate_action(parent, action, None);
                return;
            }
            C::NextTab => AppMsg::Tabs(TabsMsg::ActivateNext),
            C::PreviousTab => AppMsg::Tabs(TabsMsg::ActivatePrevious),
            C::CloseTab => AppMsg::Tabs(TabsMsg::CloseActive),
        };
        let _ = dispatcher.dispatch(message);
    }
}
