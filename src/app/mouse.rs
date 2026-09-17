//! Capture souris du terminal (`mouse-support`, design D7).
//!
//! Activer ou désactiver la capture écrit quelques octets de séquence de
//! contrôle sur la sortie du terminal : c'est du pilotage du terminal, au
//! même titre que le mode brut. L'accès passe par un trait pour que la
//! boucle reste testable sans vrai terminal.

use std::io;
use std::sync::Arc;

use ratatui::crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use ratatui::crossterm::execute;

/// Active ou désactive la capture souris du terminal.
pub trait MouseCapture: Send + Sync + 'static {
    fn set(&self, enabled: bool) -> io::Result<()>;
}

/// Capture souris du terminal réel, sur la sortie standard.
#[derive(Debug, Default, Clone, Copy)]
pub struct TerminalMouseCapture;

impl MouseCapture for TerminalMouseCapture {
    fn set(&self, enabled: bool) -> io::Result<()> {
        let mut stdout = io::stdout();
        if enabled {
            execute!(stdout, EnableMouseCapture)
        } else {
            execute!(stdout, DisableMouseCapture)
        }
    }
}

/// Réglage souris transmis à la boucle : le moyen de piloter la capture,
/// et l'état effectivement appliqué au terminal au démarrage.
#[derive(Clone)]
pub struct MouseSetup {
    pub capture: Arc<dyn MouseCapture>,
    pub enabled: bool,
    /// Échec de l'activation au démarrage, signalé dans la barre d'état.
    pub startup_error: Option<String>,
}

impl MouseSetup {
    /// Capture du terminal réel, active ou non selon `enabled`.
    pub fn terminal(enabled: bool) -> Self {
        Self {
            capture: Arc::new(TerminalMouseCapture),
            enabled,
            startup_error: None,
        }
    }
}

#[cfg(test)]
pub(crate) mod test_support {
    //! Capture de test : enregistre les demandes, ou échoue à la demande,
    //! sans jamais écrire sur le terminal.

    use std::io;
    use std::sync::Mutex;

    use super::MouseCapture;

    #[derive(Debug, Default)]
    pub struct FakeMouseCapture {
        calls: Mutex<Vec<bool>>,
        fail: bool,
    }

    impl FakeMouseCapture {
        pub fn failing() -> Self {
            Self {
                calls: Mutex::new(Vec::new()),
                fail: true,
            }
        }

        pub fn calls(&self) -> Vec<bool> {
            self.calls.lock().expect("verrou de test").clone()
        }
    }

    impl MouseCapture for FakeMouseCapture {
        fn set(&self, enabled: bool) -> io::Result<()> {
            self.calls.lock().expect("verrou de test").push(enabled);
            if self.fail {
                return Err(io::Error::other("échec simulé"));
            }
            Ok(())
        }
    }

    #[test]
    fn records_calls_and_fails_on_demand() {
        let capture = FakeMouseCapture::default();
        capture.set(false).expect("succès");
        capture.set(true).expect("succès");
        assert_eq!(capture.calls(), [false, true]);
        assert!(FakeMouseCapture::failing().set(true).is_err());
    }
}
