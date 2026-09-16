//! Interface plein écran de bruno-tui, architecture Elm-like.
//!
//! Toutes les entrées arrivent sous forme d'`AppEvent` sur un unique canal.
//! La boucle les traduit en `Message`, applique `update` au `Model`, puis
//! redessine avec `view`. Le chargement de la collection s'exécute hors de
//! la boucle, qui ne bloque jamais.

pub mod cli;
pub mod clipboard;
pub mod diagnostics;
pub mod event;
pub mod filter;
pub mod message;
pub mod model;
pub mod search;
pub mod update;
pub mod view;

use std::ffi::OsString;
use std::io;
use std::path::PathBuf;
use std::sync::Arc;

use ratatui::Terminal;
use ratatui::backend::Backend;
use tokio::sync::mpsc;

use crate::collection::CollectionLoader;
use crate::runner::BruRunner;
use crate::writer::{self, RequestWriter};
use clipboard::Clipboard;
use event::{AppEvent, EVENT_BUFFER};
use message::{Message, to_message};
use model::{Exit, Model};
use update::{Command, update};

/// Fait tourner l'interface jusqu'à la demande de sortie.
///
/// Le chargement est lancé par `spawn_blocking` ; `events` reçoit aussi
/// les événements du terminal, publiés par l'appelant sur `sender`.
/// `bru_program` est le programme délégué pour `Command::StartRun` : `bru`
/// en usage réel, le chemin d'un faux `bru` dans les tests. `clipboard` est
/// de même injectable, pour les mêmes raisons (une fausse implémentation
/// en test, `SystemClipboard` en usage réel). `writer` est également
/// injectable (`BruWriter` en usage réel).
#[allow(clippy::too_many_arguments)]
pub async fn run<B: Backend>(
    terminal: &mut Terminal<B>,
    loader: Arc<dyn CollectionLoader>,
    source: PathBuf,
    sender: mpsc::Sender<AppEvent>,
    mut events: mpsc::Receiver<AppEvent>,
    bru_program: OsString,
    clipboard: Arc<dyn Clipboard>,
    writer: Arc<dyn RequestWriter>,
) -> Result<Exit, B::Error> {
    let size = terminal.size()?;
    let mut model = Model::new(source.clone(), (size.width, size.height));

    let (run_tx, mut run_rx) = mpsc::channel(EVENT_BUFFER);
    let runner = BruRunner::with_program(bru_program, run_tx);
    {
        let sender = sender.clone();
        tokio::spawn(async move {
            while let Some(event) = run_rx.recv().await {
                if sender.send(AppEvent::Run(event)).await.is_err() {
                    break;
                }
            }
        });
    }

    {
        let sender = sender.clone();
        tokio::task::spawn_blocking(move || {
            let result = loader.load(&source);
            // Boucle terminée : plus personne n'attend la collection.
            let _ = sender.blocking_send(AppEvent::CollectionLoaded(result));
        });
    }

    terminal.draw(|frame| view::view(&model, frame))?;
    loop {
        let Some(event) = events.recv().await else {
            return Ok(Exit::TerminalError(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "flux d'événements fermé",
            )));
        };
        if let Some(message) = to_message(event, model.text_capture()) {
            let command = update(&mut model, message);
            match command {
                Command::None => {}
                Command::StartRun { request, target } => {
                    let recursive = request.recursive;
                    let handle = runner.start(request);
                    update(
                        &mut model,
                        Message::RunStarted {
                            id: handle.id(),
                            target,
                            recursive,
                            handle,
                        },
                    );
                }
                Command::CopyToClipboard { token, text } => {
                    let clipboard = Arc::clone(&clipboard);
                    let sender = sender.clone();
                    tokio::task::spawn_blocking(move || {
                        let result = clipboard.set_text(text);
                        let _ = sender.blocking_send(AppEvent::ClipboardResult { token, result });
                    });
                }
                Command::SaveEdit {
                    path,
                    ast,
                    stamp,
                    edits,
                } => {
                    let root = model
                        .loaded()
                        .map(|c| c.root.clone())
                        .unwrap_or_else(|| PathBuf::from("."));
                    let full_path = root.join(&path);
                    let writer = Arc::clone(&writer);
                    let sender = sender.clone();
                    tokio::task::spawn_blocking(move || {
                        let result = writer
                            .write_request(&full_path, &ast, &stamp, &edits)
                            .and_then(|new_stamp| {
                                let bytes = std::fs::read(&full_path).map_err(|source| {
                                    writer::WriteError::Io {
                                        path: full_path.clone(),
                                        source,
                                    }
                                })?;
                                let text = String::from_utf8(bytes).map_err(|source| {
                                    writer::WriteError::Io {
                                        path: full_path.clone(),
                                        source: io::Error::new(io::ErrorKind::InvalidData, source),
                                    }
                                })?;
                                let ast =
                                    crate::collection::BruFile::parse(text).map_err(|source| {
                                        writer::WriteError::Io {
                                            path: full_path.clone(),
                                            source: io::Error::new(
                                                io::ErrorKind::InvalidData,
                                                source,
                                            ),
                                        }
                                    })?;
                                let view = crate::collection::RequestView::from_ast(&ast).map_err(
                                    |source| writer::WriteError::Io {
                                        path: full_path.clone(),
                                        source: io::Error::new(io::ErrorKind::InvalidData, source),
                                    },
                                )?;
                                Ok(model::SavedEdit {
                                    stamp: new_stamp,
                                    ast,
                                    view,
                                })
                            });
                        let _ = sender.blocking_send(AppEvent::EditSaved { path, result });
                    });
                }
            }
        }
        if let Some(exit) = model.exit.take() {
            return Ok(exit);
        }
        terminal.draw(|frame| view::view(&model, frame))?;
    }
}

#[cfg(test)]
pub(crate) mod test_support {
    //! Outils communs aux tests unitaires du module.

    use std::path::{Path, PathBuf};

    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use super::message::Message;
    use super::model::Model;
    use super::update::{refresh_rows, scroll_tree_into_view, update};
    use super::view::{tree::display_name, view};
    use crate::collection::{BruLoader, CollectionLoader};

    pub fn fixture() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/collections/parser-cases")
    }

    /// Modèle avec `parser-cases` chargée.
    pub fn loaded_model(size: (u16, u16)) -> Model {
        let mut model = Model::new(fixture(), size);
        update(
            &mut model,
            Message::CollectionLoaded(BruLoader.load(&fixture())),
        );
        model
    }

    /// Modèle avec `runner-probe` chargée et le rapport `mixed.json` appliqué.
    pub fn runner_probe_model() -> Model {
        let probe_path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/collections/runner-probe");
        let mut model = Model::new(probe_path.clone(), (100, 30));
        update(
            &mut model,
            Message::CollectionLoaded(crate::collection::BruLoader.load(&probe_path)),
        );
        let mixed_report: crate::runner::report::Report =
            serde_json::from_str(include_str!("../../tests/fixtures/reports/mixed.json"))
                .expect("mixed.json doit se désérialiser");
        for result in mixed_report.0.into_iter().flat_map(|it| it.results) {
            let key = std::path::PathBuf::from(&result.test.filename);
            model.run.outcomes.insert(
                key,
                super::model::RequestOutcome {
                    result,
                    exit_code: Some(0),
                },
            );
        }
        model
    }

    /// Déplie les dossiers parents et sélectionne le nœud de ce chemin.
    pub fn select(model: &mut Model, path: &str) {
        let target = Path::new(path);
        for ancestor in target.ancestors().skip(1) {
            if !ancestor.as_os_str().is_empty() {
                model.tree.expanded.insert(ancestor.to_path_buf());
            }
        }
        refresh_rows(model);
        let index = model
            .tree
            .rows
            .iter()
            .position(|row| {
                model
                    .node_at(&row.address)
                    .is_some_and(|node| node.path() == target)
            })
            .unwrap_or_else(|| panic!("`{path}` introuvable"));
        model.tree.selected = index;
        model.detail_scroll = 0;
        scroll_tree_into_view(model);
    }

    pub fn selected_name(model: &Model) -> String {
        model.selected_node().map(display_name).unwrap_or_default()
    }

    /// Rend le modèle et retourne les lignes de l'écran.
    pub fn render(model: &Model, width: u16, height: u16) -> Vec<String> {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("terminal");
        terminal.draw(|frame| view(model, frame)).expect("rendu");
        screen_lines(terminal.backend())
    }

    pub fn screen_lines(backend: &TestBackend) -> Vec<String> {
        let buffer = backend.buffer();
        (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect()
            })
            .collect()
    }
}
