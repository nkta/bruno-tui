//! Entrées de la boucle d'événements.
//!
//! Toutes les sources (terminal, chargement de la collection, et plus tard
//! le runner) publient un `AppEvent` sur un unique canal tokio.

use std::io;
use std::path::PathBuf;
use std::thread;

use ratatui::crossterm::event::{self, Event};
use tokio::sync::mpsc;

use super::clipboard::ClipboardError;
use super::model::SavedEdit;
use crate::collection::{Collection, LoadError};
use crate::runner;
use crate::writer::WriteError;

/// Taille du canal d'événements.
pub const EVENT_BUFFER: usize = 256;

/// Entrée brute de la boucle.
#[derive(Debug)]
pub enum AppEvent {
    /// Événement du terminal : touche, redimensionnement, ...
    Terminal(Event),
    /// La lecture du terminal a échoué ; plus aucune touche n'arrivera.
    TerminalClosed(io::Error),
    /// Fin du chargement de la collection.
    CollectionLoaded(Result<Collection, LoadError>),
    /// Issue d'une exécution `bru run`.
    Run(runner::RunEvent),
    /// Issue d'une copie vers le presse-papiers (`Command::CopyToClipboard`).
    /// `token` identifie l'action pour ignorer un résultat en retard sur
    /// une action plus récente.
    ClipboardResult {
        token: u64,
        result: Result<(), ClipboardError>,
    },
    /// Issue d'une écriture sur disque (`Command::SaveEdit`).
    EditSaved {
        path: PathBuf,
        result: Result<SavedEdit, WriteError>,
    },
}

/// Lance le thread qui lit le terminal et publie ses événements.
///
/// Le thread s'arrête quand la boucle a fermé le canal ou quand la lecture
/// échoue. Il n'est pas joint : bloqué dans `read`, il disparaît avec le
/// processus.
pub fn spawn_terminal_reader(events: mpsc::Sender<AppEvent>) -> io::Result<()> {
    thread::Builder::new()
        .name("terminal-reader".into())
        .spawn(move || {
            loop {
                match event::read() {
                    Ok(event) => {
                        if events.blocking_send(AppEvent::Terminal(event)).is_err() {
                            break;
                        }
                    }
                    Err(error) => {
                        let _ = events.blocking_send(AppEvent::TerminalClosed(error));
                        break;
                    }
                }
            }
        })
        .map(|_| ())
}
