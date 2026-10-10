//! Asynchronous palette matching and library searches.

use carver_sdk::{LibraryBackend, PageRequest, SearchHit};

use super::super::{AppMsg, Effect, palette::PaletteMsg};
use super::AppRuntime;

impl<B: LibraryBackend> AppRuntime<B> {
    pub(super) fn run_palette_effect(&self, effect: Effect) {
        match effect {
            Effect::MatchPalette {
                id,
                request,
                candidates,
                query,
            } => {
                let runtime = self.clone();
                glib::spawn_future_local(async move {
                    match gtk::gio::spawn_blocking(move || {
                        super::super::palette::match_rows(candidates, &query)
                    })
                    .await
                    {
                        Ok(rows) => runtime.dispatch(AppMsg::Palette(PaletteMsg::Matched {
                            id,
                            request,
                            rows,
                        })),
                        Err(_) => runtime.dispatch(AppMsg::Palette(PaletteMsg::Matched {
                            id,
                            request,
                            rows: Vec::new(),
                        })),
                    }
                });
            }
            Effect::SchedulePaletteSearch {
                id,
                request,
                recent,
            } => {
                let runtime = self.clone();
                glib::spawn_future_local(async move {
                    if !recent {
                        glib::timeout_future(std::time::Duration::from_millis(250)).await;
                    }
                    runtime.dispatch(AppMsg::Palette(PaletteMsg::SearchElapsed { id, request }));
                });
            }
            Effect::SearchPaletteNotes { id, request, query } => {
                let client = self.inner.client.clone();
                let runtime = self.clone();
                glib::spawn_future_local(async move {
                    let result = if query.trim().is_empty() {
                        client
                            .recent_notes_async(
                                None,
                                PageRequest {
                                    limit: 5,
                                    offset: 0,
                                },
                            )
                            .await
                            .map(|page| {
                                (
                                    page.items
                                        .into_iter()
                                        .map(|note| SearchHit {
                                            note,
                                            snippet: String::new(),
                                        })
                                        .collect(),
                                    false,
                                )
                            })
                    } else {
                        client
                            .search_async(
                                query,
                                None,
                                PageRequest {
                                    limit: 20,
                                    offset: 0,
                                },
                            )
                            .await
                            .map(|page| (page.items, page.has_more))
                    }
                    .map_err(super::display_error);
                    runtime.dispatch(AppMsg::Palette(PaletteMsg::NotesLoaded {
                        id,
                        request,
                        result,
                    }));
                });
            }
            effect => self.inner.view.run_palette_effect(effect, &self.model()),
        }
    }
}
