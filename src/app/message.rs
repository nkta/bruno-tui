//! Messages traités par `update`, et correspondance des touches.
//!
//! La correspondance ne dépend pas du modèle, à une exception près : la
//! capture de texte (recherche, et plus tard édition ou filtre). Pendant
//! une capture, les touches de navigation redeviennent du texte : `to_message`
//! reçoit alors la variante de [`TextCapture`] à servir, calculée par
//! l'appelant sur le modèle (`Model::text_capture()`), au même titre que le
//! focus l'est déjà pour `Up`/`Down`. Le reste de la correspondance ignore
//! le modèle : `Up` est interprété par `update` selon le panneau qui a le
//! focus.

use std::io;
use std::path::PathBuf;

use ratatui::crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use super::clipboard::ClipboardError;
use super::event::AppEvent;
use crate::collection::{Collection, LoadError};
use crate::runner::{RunEvent, RunHandle, RunId};

/// Ce que la boucle capture au clavier hors navigation normale. Au plus une
/// variante active à la fois, garanti par construction : tant qu'une
/// capture est active, `capture_message` intercepte tous les caractères
/// avant que la table hors-saisie (qui porte les liaisons d'ouverture) ne
/// soit jamais consultée. Cette énumération est délibérément ouverte : un
/// futur changement y ajoute sa propre variante sans toucher à celle-ci.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextCapture {
    Search,
    Insert,
}

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
    /// `/`, hors saisie.
    StartSearch,
    /// Caractère tapé pendant une saisie de recherche.
    SearchInput(char),
    /// `Retour arrière` pendant une saisie de recherche.
    SearchBackspace,
    /// `Entrée` pendant une saisie de recherche.
    ConfirmSearch,
    /// `Échap` pendant une saisie de recherche.
    CancelSearch,
    /// `n`, hors saisie.
    NextMatch,
    /// `N`, hors saisie.
    PreviousMatch,
    /// `v`, hors saisie, en focus Détail.
    ToggleVisual,
    /// `y`, hors saisie, en focus Détail.
    Yank,
    /// Renvoyé par la boucle après `Command::CopyToClipboard`.
    ClipboardResult {
        token: u64,
        result: Result<(), ClipboardError>,
    },
    /// `e`, hors saisie.
    StartEdit,
    /// `↓`/`j` (+1), `↑`/`k` (-1), en session Normal.
    MoveFieldCursor(i8),
    /// `Espace`, en session Normal.
    ToggleField,
    /// `i`, en session Normal.
    EnterInsert,
    /// `w`, en session Normal.
    SaveEdit,
    /// `Échap`, en saisie Insert.
    LeaveInsert,
    /// Caractère tapé pendant une saisie Insert.
    InsertChar(char),
    /// `Retour arrière` pendant une saisie Insert.
    InsertBackspace,
    /// Flèche gauche pendant une saisie Insert.
    InsertCursorLeft,
    /// Flèche droite pendant une saisie Insert.
    InsertCursorRight,
    /// `Entrée` pendant une saisie Insert.
    InsertEnter,
    /// Renvoyé par la boucle après `Command::SaveEdit`.
    EditSaved {
        path: PathBuf,
        result: Result<super::model::SavedEdit, crate::writer::WriteError>,
    },
    /// Réponse 'oui' à un `PendingConfirm`.
    ConfirmYes,
    /// Réponse 'non' à un `PendingConfirm`.
    ConfirmNo,
}

/// Traduit une entrée brute en message ; `None` si elle est ignorée.
/// `capture` vient de `Model::text_capture()`, calculé par l'appelant juste
/// avant, au même endroit où le modèle est déjà en portée.
pub fn to_message(event: AppEvent, capture: Option<TextCapture>) -> Option<Message> {
    match event {
        AppEvent::Terminal(Event::Key(key)) => key_message(key, capture),
        AppEvent::Terminal(Event::Resize(width, height)) => Some(Message::Resize { width, height }),
        AppEvent::Terminal(_) => None,
        AppEvent::TerminalClosed(error) => Some(Message::TerminalClosed(error)),
        AppEvent::CollectionLoaded(result) => Some(Message::CollectionLoaded(result)),
        AppEvent::Run(event) => Some(Message::RunFinished(event)),
        AppEvent::ClipboardResult { token, result } => {
            Some(Message::ClipboardResult { token, result })
        }
        AppEvent::EditSaved { path, result } => Some(Message::EditSaved { path, result }),
    }
}

fn key_message(key: KeyEvent, capture: Option<TextCapture>) -> Option<Message> {
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
    if let Some(capture) = capture {
        return capture_message(key, capture);
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
        KeyCode::Char('/') => Message::StartSearch,
        KeyCode::Char('n') => Message::NextMatch,
        KeyCode::Char('N') => Message::PreviousMatch,
        KeyCode::Char('v') => Message::ToggleVisual,
        KeyCode::Char('y') => Message::Yank,
        KeyCode::Char('e') => Message::StartEdit,
        KeyCode::Char(' ') => Message::ToggleField,
        KeyCode::Char('i') => Message::EnterInsert,
        KeyCode::Char('w') => Message::SaveEdit,
        _ => return None,
    };
    Some(message)
}

/// Distribue vers la fonction de capture propre à chaque variante. `Ctrl`
/// est déjà traité par `key_message` avant d'arriver ici : jamais dupliqué.
fn capture_message(key: KeyEvent, capture: TextCapture) -> Option<Message> {
    match capture {
        TextCapture::Search => search_capture_message(key),
        TextCapture::Insert => insert_capture_message(key),
    }
}

/// Capture des caractères en mode Insert de session d'édition.
fn insert_capture_message(key: KeyEvent) -> Option<Message> {
    match key.code {
        KeyCode::Char(c) => Some(Message::InsertChar(c)),
        KeyCode::Backspace => Some(Message::InsertBackspace),
        KeyCode::Left => Some(Message::InsertCursorLeft),
        KeyCode::Right => Some(Message::InsertCursorRight),
        KeyCode::Enter => Some(Message::InsertEnter),
        KeyCode::Esc => Some(Message::LeaveInsert),
        _ => None,
    }
}

/// Un caractère composé (`Alt` inclus) reste un caractère pendant la
/// saisie : `Alt` n'est pas rejeté ici, contrairement à la table hors
/// saisie.
fn search_capture_message(key: KeyEvent) -> Option<Message> {
    match key.code {
        KeyCode::Char(c) => Some(Message::SearchInput(c)),
        KeyCode::Backspace => Some(Message::SearchBackspace),
        KeyCode::Enter => Some(Message::ConfirmSearch),
        KeyCode::Esc => Some(Message::CancelSearch),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode, modifiers: KeyModifiers) -> AppEvent {
        AppEvent::Terminal(Event::Key(KeyEvent::new(code, modifiers)))
    }

    fn name(event: AppEvent) -> Option<String> {
        to_message(event, None).map(|m| {
            let debug = format!("{m:?}");
            debug
                .split([' ', '(', '{'])
                .next()
                .unwrap_or_default()
                .to_owned()
        })
    }

    fn name_capturing(event: AppEvent, capture: TextCapture) -> Option<String> {
        to_message(event, Some(capture)).map(|m| {
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
            (KeyCode::Char('/'), none, "StartSearch"),
            (KeyCode::Char('n'), none, "NextMatch"),
            (KeyCode::Char('N'), KeyModifiers::SHIFT, "PreviousMatch"),
            (KeyCode::Char('v'), none, "ToggleVisual"),
            (KeyCode::Char('y'), none, "Yank"),
            (KeyCode::Char('e'), none, "StartEdit"),
            (KeyCode::Char(' '), none, "ToggleField"),
            (KeyCode::Char('i'), none, "EnterInsert"),
            (KeyCode::Char('w'), none, "SaveEdit"),
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
        assert!(to_message(AppEvent::Terminal(Event::Key(release)), None).is_none());
        assert!(to_message(AppEvent::Terminal(Event::FocusGained), None).is_none());
    }

    #[test]
    fn non_key_events() {
        assert!(matches!(
            to_message(AppEvent::Terminal(Event::Resize(80, 24)), None),
            Some(Message::Resize {
                width: 80,
                height: 24
            })
        ));
        assert!(matches!(
            to_message(AppEvent::TerminalClosed(io::Error::other("x")), None),
            Some(Message::TerminalClosed(_))
        ));
        let loaded =
            AppEvent::CollectionLoaded(Err(LoadError::NotACollection { path: "/x".into() }));
        assert!(matches!(
            to_message(loaded, None),
            Some(Message::CollectionLoaded(Err(_)))
        ));

        // Aucun `bru` réel nécessaire : un `RunOutcome::Cancelled` suffit à
        // construire l'événement sans I/O.
        let run = AppEvent::Run(RunEvent {
            id: RunId(1),
            outcome: crate::runner::RunOutcome::Cancelled,
        });
        assert!(matches!(
            to_message(run, None),
            Some(Message::RunFinished(RunEvent {
                id: RunId(1),
                outcome: crate::runner::RunOutcome::Cancelled,
            }))
        ));

        let copied = AppEvent::ClipboardResult {
            token: 1,
            result: Ok(()),
        };
        assert!(matches!(
            to_message(copied, None),
            Some(Message::ClipboardResult {
                token: 1,
                result: Ok(())
            })
        ));
    }

    /// En saisie de recherche, les touches de navigation redeviennent du
    /// texte ; `Ctrl+C` reste prioritaire.
    #[test]
    fn search_capture_redirects_navigation_keys_to_input() {
        let none = KeyModifiers::NONE;
        for (code, expected) in [
            (KeyCode::Char('j'), "SearchInput"),
            (KeyCode::Char('q'), "SearchInput"),
            (KeyCode::Char('r'), "SearchInput"),
            (KeyCode::Backspace, "SearchBackspace"),
            (KeyCode::Enter, "ConfirmSearch"),
            (KeyCode::Esc, "CancelSearch"),
        ] {
            assert_eq!(
                name_capturing(key(code, none), TextCapture::Search).as_deref(),
                Some(expected),
                "{code:?}"
            );
        }
        // Un caractère composé (Alt) reste un caractère pendant la saisie,
        // contrairement à la table hors saisie qui le rejette.
        assert_eq!(
            name_capturing(
                key(KeyCode::Char('e'), KeyModifiers::ALT),
                TextCapture::Search
            )
            .as_deref(),
            Some("SearchInput")
        );
        // Ctrl+C reste prioritaire même en saisie.
        assert_eq!(
            name_capturing(
                key(KeyCode::Char('c'), KeyModifiers::CONTROL),
                TextCapture::Search
            )
            .as_deref(),
            Some("ForceQuit")
        );
        // Une touche dédiée sans équivalent caractère (flèches, Tab,
        // pagination) reste sans effet en saisie.
        assert!(name_capturing(key(KeyCode::Tab, none), TextCapture::Search).is_none());
        assert!(name_capturing(key(KeyCode::PageDown, none), TextCapture::Search).is_none());
    }

    /// En mode Insert, les caractères, flèches gauche/droite, retour arrière,
    /// Entrée et Échap sont capturés ; Ctrl+C reste prioritaire.
    #[test]
    fn insert_capture_redirects_navigation_keys_to_input() {
        let none = KeyModifiers::NONE;
        for (code, expected) in [
            (KeyCode::Char('a'), "InsertChar"),
            (KeyCode::Char('j'), "InsertChar"),
            (KeyCode::Char('q'), "InsertChar"),
            (KeyCode::Char(' '), "InsertChar"),
            (KeyCode::Backspace, "InsertBackspace"),
            (KeyCode::Left, "InsertCursorLeft"),
            (KeyCode::Right, "InsertCursorRight"),
            (KeyCode::Enter, "InsertEnter"),
            (KeyCode::Esc, "LeaveInsert"),
        ] {
            assert_eq!(
                name_capturing(key(code, none), TextCapture::Insert).as_deref(),
                Some(expected),
                "{code:?}"
            );
        }
        // Alt inclus lors de la saisie
        assert_eq!(
            name_capturing(
                key(KeyCode::Char('e'), KeyModifiers::ALT),
                TextCapture::Insert
            )
            .as_deref(),
            Some("InsertChar")
        );
        // Ctrl+C reste prioritaire
        assert_eq!(
            name_capturing(
                key(KeyCode::Char('c'), KeyModifiers::CONTROL),
                TextCapture::Insert
            )
            .as_deref(),
            Some("ForceQuit")
        );
        // Touches non liées en saisie Insert
        assert!(name_capturing(key(KeyCode::Up, none), TextCapture::Insert).is_none());
        assert!(name_capturing(key(KeyCode::Down, none), TextCapture::Insert).is_none());
        assert!(name_capturing(key(KeyCode::Tab, none), TextCapture::Insert).is_none());
        assert!(name_capturing(key(KeyCode::PageDown, none), TextCapture::Insert).is_none());
        assert!(name_capturing(key(KeyCode::Home, none), TextCapture::Insert).is_none());
        assert!(name_capturing(key(KeyCode::End, none), TextCapture::Insert).is_none());
        assert!(name_capturing(key(KeyCode::F(1), none), TextCapture::Insert).is_none());
    }
}
