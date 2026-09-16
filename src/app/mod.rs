//! Interface plein écran de bruno-tui, architecture Elm-like.
//!
//! Toutes les entrées arrivent sous forme d'`AppEvent` sur un unique canal.
//! La boucle les traduit en `Message`, applique `update` au `Model`, puis
//! redessine avec `view`. Le chargement de la collection s'exécute hors de
//! la boucle, qui ne bloque jamais.

pub mod cli;
pub mod event;
pub mod message;
pub mod model;
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
use event::{AppEvent, EVENT_BUFFER};
use message::{Message, to_message};
use model::{Exit, Model};
use update::{Command, update};

/// Fait tourner l'interface jusqu'à la demande de sortie.
///
/// Le chargement est lancé par `spawn_blocking` ; `events` reçoit aussi
/// les événements du terminal, publiés par l'appelant sur `sender`.
/// `bru_program` est le programme délégué pour `Command::StartRun` : `bru`
/// en usage réel, le chemin d'un faux `bru` dans les tests.
pub async fn run<B: Backend>(
    terminal: &mut Terminal<B>,
    loader: Arc<dyn CollectionLoader>,
    source: PathBuf,
    sender: mpsc::Sender<AppEvent>,
    mut events: mpsc::Receiver<AppEvent>,
    bru_program: OsString,
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

    tokio::task::spawn_blocking(move || {
        let result = loader.load(&source);
        // Boucle terminée : plus personne n'attend la collection.
        let _ = sender.blocking_send(AppEvent::CollectionLoaded(result));
    });

    terminal.draw(|frame| view::view(&model, frame))?;
    loop {
        let Some(event) = events.recv().await else {
            return Ok(Exit::TerminalError(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "flux d'événements fermé",
            )));
        };
        if let Some(message) = to_message(event) {
            let command = update(&mut model, message);
            if let Command::StartRun { request, target } = command {
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
