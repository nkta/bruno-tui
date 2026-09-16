//! Accès au presse-papiers du système.
//!
//! `arboard` n'est ni asynchrone ni garanti instantané : la connexion au
//! serveur d'affichage peut prendre plusieurs secondes si `DISPLAY` pointe
//! vers un hôte injoignable. L'appel passe donc toujours par un
//! `spawn_blocking`, jamais directement dans la boucle d'événements.

use thiserror::Error;

/// Écrit du texte dans le presse-papiers du système. `&self` : aucune
/// implémentation ne conserve de ressource ouverte entre deux appels.
pub trait Clipboard: Send + Sync + 'static {
    fn set_text(&self, text: String) -> Result<(), ClipboardError>;
}

/// Presse-papiers inaccessible. Ne porte jamais le texte copié, seulement
/// le message d'`arboard`.
#[derive(Debug, Error)]
#[error("presse-papiers inaccessible : {0}")]
pub struct ClipboardError(String);

/// Presse-papiers du système, via `arboard`. Reconstruit une connexion à
/// chaque appel plutôt que d'en garder une ouverte.
///
/// Vérifié manuellement (`cargo run` d'une sonde isolée) : `DISPLAY=:1`
/// disponible → succès en 0,57 ms ; `DISPLAY`/`WAYLAND_DISPLAY` absents →
/// `Clipboard::new()` échoue en 24 µs (« X11 server connection timed out »),
/// sans blocage perceptible.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemClipboard;

impl Clipboard for SystemClipboard {
    fn set_text(&self, text: String) -> Result<(), ClipboardError> {
        let mut clipboard =
            arboard::Clipboard::new().map_err(|error| ClipboardError(error.to_string()))?;
        clipboard
            .set_text(text)
            .map_err(|error| ClipboardError(error.to_string()))
    }
}

#[cfg(test)]
pub(crate) mod test_support {
    //! Presse-papiers de test : enregistre le texte reçu, ou échoue à la
    //! demande, sans jamais toucher un vrai serveur d'affichage.

    use std::sync::Mutex;

    use super::{Clipboard, ClipboardError};

    #[derive(Debug, Default)]
    pub struct FakeClipboard {
        copied: Mutex<Vec<String>>,
        fail: bool,
    }

    impl FakeClipboard {
        pub fn new() -> Self {
            Self::default()
        }

        pub fn failing() -> Self {
            Self {
                copied: Mutex::new(Vec::new()),
                fail: true,
            }
        }

        pub fn copied(&self) -> Vec<String> {
            self.copied.lock().expect("verrou de test").clone()
        }
    }

    impl Clipboard for FakeClipboard {
        fn set_text(&self, text: String) -> Result<(), ClipboardError> {
            if self.fail {
                return Err(ClipboardError("échec simulé".into()));
            }
            self.copied.lock().expect("verrou de test").push(text);
            Ok(())
        }
    }

    /// Construit une `ClipboardError` de test, pour les modules qui ont
    /// seulement besoin d'un `Err` à faire circuler (`Model::last_status`),
    /// sans passer par une véritable tentative de copie.
    pub fn error_for_test() -> ClipboardError {
        ClipboardError("échec simulé".into())
    }

    #[test]
    fn satisfies_the_trait() {
        let clipboard = FakeClipboard::new();
        clipboard.set_text("bonjour".into()).expect("copie réussie");
        assert_eq!(clipboard.copied(), ["bonjour"]);

        let failing = FakeClipboard::failing();
        assert!(failing.set_text("x".into()).is_err());
        assert!(failing.copied().is_empty());
    }
}
