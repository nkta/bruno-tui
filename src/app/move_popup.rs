//! Popup « Déplacer vers » (`docs/edition-phase4.md` § 4.2) : choisir le
//! dossier de la même collection où déplacer la ligne sélectionnée.
//!
//! État et logique purs, sans I/O. La liste des destinations est construite
//! par `update` à partir de l'arbre chargé ; la saisie est un filtre
//! (sous-chaîne, sans casse), `↑↓` choisissent. L'opération elle-même
//! ([`crate::writer::files::FileOp::Move`]) est montée à la validation, sur
//! l'arbre alors chargé.

use std::path::PathBuf;

use super::message::InputKey;
use super::text_input::TextInput;

/// Libellé de la racine de la collection dans la liste.
pub const ROOT_LABEL: &str = "(racine de la collection)";

/// Dossier cible possible.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MoveDestination {
    /// Chemin relatif à la racine ; vide : la racine de la collection.
    pub dir: PathBuf,
    /// Nom affiché du dossier.
    pub label: String,
    /// Profondeur d'indentation dans l'arbre.
    pub depth: usize,
    /// Nom que prendrait l'élément dans ce dossier si le sien y est déjà
    /// pris (suffixe ` 1`, ` 2`… comme Bruno) ; `None` sans collision.
    pub renamed: Option<String>,
}

impl MoveDestination {
    fn matches(&self, filter: &str) -> bool {
        let filter = filter.to_lowercase();
        self.label.to_lowercase().contains(&filter)
            || self.dir.to_string_lossy().to_lowercase().contains(&filter)
    }
}

/// Popup ouvert sur l'arbre.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MovePopup {
    /// Élément déplacé, relatif à la racine.
    pub item: PathBuf,
    pub is_folder: bool,
    /// Nom du fichier ou du répertoire, pour le titre.
    pub context: String,
    pub filter: TextInput,
    destinations: Vec<MoveDestination>,
    /// Indice dans la liste filtrée.
    pub selected: usize,
    /// Refus de la dernière validation, affiché dans le popup.
    pub error: Option<String>,
    /// Déplacement demandé à la boucle, résultat attendu : le popup reste
    /// ouvert pour y afficher un refus éventuel (§ 4.2).
    pub pending: bool,
}

impl MovePopup {
    pub fn new(
        item: PathBuf,
        is_folder: bool,
        context: String,
        destinations: Vec<MoveDestination>,
    ) -> Self {
        Self {
            item,
            is_folder,
            context,
            filter: TextInput::new("", false),
            destinations,
            selected: 0,
            error: None,
            pending: false,
        }
    }

    pub fn title(&self) -> String {
        format!(" Déplacer — {} ", self.context)
    }

    /// Destinations qui passent le filtre, dans l'ordre de l'arbre.
    pub fn visible(&self) -> Vec<&MoveDestination> {
        let filter = self.filter.text().trim();
        self.destinations
            .iter()
            .filter(|destination| destination.matches(filter))
            .collect()
    }

    /// Destination choisie, s'il en reste une après le filtre.
    pub fn selected_destination(&self) -> Option<&MoveDestination> {
        self.visible().get(self.selected).copied()
    }

    pub fn next(&mut self) {
        let count = self.visible().len();
        if self.selected + 1 < count {
            self.selected += 1;
        }
    }

    pub fn previous(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    /// Touche de saisie du filtre : la sélection repart en tête.
    pub fn key(&mut self, key: InputKey) {
        let before = self.filter.text().to_owned();
        match key {
            InputKey::Char(c) => self.filter.insert(c),
            InputKey::Backspace => self.filter.backspace(),
            InputKey::Delete => self.filter.delete(),
            InputKey::Left => self.filter.left(),
            InputKey::Right => self.filter.right(),
            InputKey::Home => self.filter.home(),
            InputKey::End => self.filter.end(),
            InputKey::Up | InputKey::Down | InputKey::Enter => {}
        }
        if self.filter.text() != before {
            self.selected = 0;
            self.error = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn destination(dir: &str, label: &str, depth: usize) -> MoveDestination {
        MoveDestination {
            dir: PathBuf::from(dir),
            label: label.to_owned(),
            depth,
            renamed: None,
        }
    }

    fn popup() -> MovePopup {
        MovePopup::new(
            PathBuf::from("get-post.bru"),
            false,
            "get-post.bru".into(),
            vec![
                destination("", ROOT_LABEL, 0),
                destination("Posts", "Articles", 0),
                destination("Posts/Comments", "Commentaires", 1),
                destination("Users", "Utilisateurs", 0),
            ],
        )
    }

    fn typed(popup: &mut MovePopup, text: &str) {
        for c in text.chars() {
            popup.key(InputKey::Char(c));
        }
    }

    #[test]
    fn selection_moves_and_stops_at_the_ends() {
        let mut popup = popup();
        assert_eq!(popup.title(), " Déplacer — get-post.bru ");
        popup.previous();
        assert_eq!(popup.selected, 0);
        for _ in 0..10 {
            popup.next();
        }
        assert_eq!(popup.selected, 3);
        assert_eq!(
            popup.selected_destination().map(|d| d.dir.clone()),
            Some(PathBuf::from("Users"))
        );
    }

    #[test]
    fn filter_is_a_case_insensitive_substring_on_label_or_path() {
        let mut popup = popup();
        popup.next();
        typed(&mut popup, "ARTI");
        // Le filtre remet la sélection en tête.
        assert_eq!(popup.selected, 0);
        let dirs: Vec<_> = popup.visible().iter().map(|d| d.dir.clone()).collect();
        assert_eq!(dirs, [PathBuf::from("Posts")]);
        // Le chemin compte aussi : « posts/ » garde le sous-dossier.
        for _ in 0..4 {
            popup.key(InputKey::Backspace);
        }
        typed(&mut popup, "posts/");
        let dirs: Vec<_> = popup.visible().iter().map(|d| d.dir.clone()).collect();
        assert_eq!(dirs, [PathBuf::from("Posts/Comments")]);
        typed(&mut popup, "zzz");
        assert!(popup.visible().is_empty());
        assert!(popup.selected_destination().is_none());
    }
}
