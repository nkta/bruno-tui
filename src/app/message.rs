//! Messages traités par `update`, et correspondance des touches.
//!
//! La correspondance ne dépend pas du modèle : `Up` est interprété par
//! `update` selon le panneau qui a le focus. Les touches sont ainsi
//! définies en un seul endroit.

use std::io;

use std::path::PathBuf;

use ratatui::crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use super::event::AppEvent;
use crate::collection::{Collection, LoadError};
use crate::runner::{RunEvent, RunHandle, RunId};

#[derive(Debug)]
pub enum Message {
    Up,
    Down,
    PageUp,
    PageDown,
    Home,
    End,
    Left,
    /// `→`, `l` ou `Entrée`.
    Right,
    /// `Tab` : bascule le focus.
    NextFocus,
    /// `Échap` : rend le focus à l'arbre.
    FocusTree,
    /// `q`.
    Quit,
    /// `Ctrl+C`, effectif en toutes circonstances.
    ForceQuit,
    Resize {
        width: u16,
        height: u16,
    },
    CollectionLoaded(Result<Collection, LoadError>),
    TerminalClosed(io::Error),
    /// `r`, hors navigation : agit sur `model.tree.selected`.
    RunSelected,
    /// `Ctrl+X`.
    CancelRun,
    /// Renvoyé par la boucle après avoir exécuté `Command::StartRun`.
    RunStarted {
        id: RunId,
        target: PathBuf,
        recursive: bool,
        handle: RunHandle,
    },
    /// Issue reçue sur le canal d'événements.
    RunFinished(RunEvent),
}

/// Traduit une entrée brute en message ; `None` si elle est ignorée.
pub fn to_message(event: AppEvent) -> Option<Message> {
    match event {
        AppEvent::Terminal(Event::Key(key)) => key_message(key),
        AppEvent::Terminal(Event::Resize(width, height)) => Some(Message::Resize { width, height }),
        AppEvent::Terminal(_) => None,
        AppEvent::TerminalClosed(error) => Some(Message::TerminalClosed(error)),
        AppEvent::CollectionLoaded(result) => Some(Message::CollectionLoaded(result)),
        AppEvent::Run(event) => Some(Message::RunFinished(event)),
    }
}

fn key_message(key: KeyEvent) -> Option<Message> {
    if key.kind != KeyEventKind::Press {
        return None;
    }
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        return match key.code {
            KeyCode::Char('c') => Some(Message::ForceQuit),
            KeyCode::Char('x') => Some(Message::CancelRun),
            _ => None,
        };
    }
    if key.modifiers.contains(KeyModifiers::ALT) {
        return None;
    }
    let message = match key.code {
        KeyCode::Up | KeyCode::Char('k') => Message::Up,
        KeyCode::Down | KeyCode::Char('j') => Message::Down,
        KeyCode::Left | KeyCode::Char('h') => Message::Left,
        KeyCode::Right | KeyCode::Char('l') | KeyCode::Enter => Message::Right,
        KeyCode::Home | KeyCode::Char('g') => Message::Home,
        KeyCode::End | KeyCode::Char('G') => Message::End,
        KeyCode::PageUp => Message::PageUp,
        KeyCode::PageDown => Message::PageDown,
        KeyCode::Tab => Message::NextFocus,
        KeyCode::Esc => Message::FocusTree,
        KeyCode::Char('q') => Message::Quit,
        KeyCode::Char('r') => Message::RunSelected,
        _ => return None,
    };
    Some(message)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode, modifiers: KeyModifiers) -> AppEvent {
        AppEvent::Terminal(Event::Key(KeyEvent::new(code, modifiers)))
    }

    fn name(event: AppEvent) -> Option<String> {
        to_message(event).map(|m| {
            let debug = format!("{m:?}");
            debug
                .split([' ', '(', '{'])
                .next()
                .unwrap_or_default()
                .to_owned()
        })
    }

    #[test]
    fn every_key_binding() {
        let none = KeyModifiers::NONE;
        let cases = [
            (KeyCode::Up, none, "Up"),
            (KeyCode::Char('k'), none, "Up"),
            (KeyCode::Down, none, "Down"),
            (KeyCode::Char('j'), none, "Down"),
            (KeyCode::Left, none, "Left"),
            (KeyCode::Char('h'), none, "Left"),
            (KeyCode::Right, none, "Right"),
            (KeyCode::Char('l'), none, "Right"),
            (KeyCode::Enter, none, "Right"),
            (KeyCode::Home, none, "Home"),
            (KeyCode::Char('g'), none, "Home"),
            (KeyCode::End, none, "End"),
            (KeyCode::Char('G'), KeyModifiers::SHIFT, "End"),
            (KeyCode::PageUp, none, "PageUp"),
            (KeyCode::PageDown, none, "PageDown"),
            (KeyCode::Tab, none, "NextFocus"),
            (KeyCode::Esc, none, "FocusTree"),
            (KeyCode::Char('q'), none, "Quit"),
            (KeyCode::Char('r'), none, "RunSelected"),
            (KeyCode::Char('c'), KeyModifiers::CONTROL, "ForceQuit"),
            (KeyCode::Char('x'), KeyModifiers::CONTROL, "CancelRun"),
        ];
        for (code, modifiers, expected) in cases {
            assert_eq!(
                name(key(code, modifiers)).as_deref(),
                Some(expected),
                "{code:?}"
            );
        }
    }

    #[test]
    fn ignored_keys() {
        assert!(name(key(KeyCode::Char('c'), KeyModifiers::NONE)).is_none());
        assert!(name(key(KeyCode::Char('q'), KeyModifiers::CONTROL)).is_none());
        assert!(name(key(KeyCode::Char('j'), KeyModifiers::ALT)).is_none());
        assert!(name(key(KeyCode::F(1), KeyModifiers::NONE)).is_none());
        // Toute autre combinaison `Ctrl+<lettre>` reste ignorée.
        assert!(name(key(KeyCode::Char('r'), KeyModifiers::CONTROL)).is_none());
        assert!(name(key(KeyCode::Char('v'), KeyModifiers::CONTROL)).is_none());
        let mut release = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE);
        release.kind = KeyEventKind::Release;
        assert!(to_message(AppEvent::Terminal(Event::Key(release))).is_none());
        assert!(to_message(AppEvent::Terminal(Event::FocusGained)).is_none());
    }

    #[test]
    fn non_key_events() {
        assert!(matches!(
            to_message(AppEvent::Terminal(Event::Resize(80, 24))),
            Some(Message::Resize {
                width: 80,
                height: 24
            })
        ));
        assert!(matches!(
            to_message(AppEvent::TerminalClosed(io::Error::other("x"))),
            Some(Message::TerminalClosed(_))
        ));
        let loaded =
            AppEvent::CollectionLoaded(Err(LoadError::NotACollection { path: "/x".into() }));
        assert!(matches!(
            to_message(loaded),
            Some(Message::CollectionLoaded(Err(_)))
        ));

        // Aucun `bru` réel nécessaire : un `RunOutcome::Cancelled` suffit à
        // construire l'événement sans I/O.
        let run = AppEvent::Run(RunEvent {
            id: RunId(1),
            outcome: crate::runner::RunOutcome::Cancelled,
        });
        assert!(matches!(
            to_message(run),
            Some(Message::RunFinished(RunEvent {
                id: RunId(1),
                outcome: crate::runner::RunOutcome::Cancelled,
            }))
        ));
    }
}
