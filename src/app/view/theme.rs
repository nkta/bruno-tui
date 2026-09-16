//! Palette de styles communs à tous les panneaux (capacité `visual-theme`).
//!
//! Une constante par catégorie d'information, réutilisée partout où la
//! catégorie apparaît, pour qu'une même catégorie porte toujours le même
//! style d'un panneau à l'autre.

use ratatui::style::{Color, Modifier, Style};

/// Méthode HTTP (`GET`, `POST`, ...), dans l'arbre et le détail.
pub const METHOD: Style = Style::new().fg(Color::Cyan);
/// Dernier résultat connu d'une requête : succès.
pub const SUCCESS: Style = Style::new().fg(Color::Green);
/// Dernier résultat connu d'une requête : échec.
pub const FAILURE: Style = Style::new().fg(Color::Red);
/// Requête dont l'exécution est en cours.
pub const RUNNING: Style = Style::new().fg(Color::Blue);
/// Nœud ou fichier en erreur de chargement, distinct de [`FAILURE`] :
/// un fichier invalide n'a pas la même cause qu'une requête qui échoue à
/// l'exécution.
pub const LOAD_ERROR: Style = Style::new().fg(Color::Magenta);
/// Bordure du panneau ayant le focus, distinct de [`RUNNING`].
pub const FOCUS: Style = Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD);
/// Titre du nœud sélectionné, dans le panneau de détail : le plus mis en
/// valeur des trois niveaux de hiérarchie du détail.
pub const TITLE: Style = Style::new().add_modifier(Modifier::BOLD);
/// Titre de section, dans le panneau de détail : second niveau, distinct
/// du titre de nœud par la couleur en plus du soulignement.
pub const SECTION: Style = Style::new()
    .fg(Color::Cyan)
    .add_modifier(Modifier::BOLD.union(Modifier::UNDERLINED));
/// Libellé d'un champ (« Chemin », « Méthode », ...), dans le panneau de
/// détail : atténué pour que la valeur qui le suit ressorte davantage.
pub const LABEL: Style = Style::new().add_modifier(Modifier::DIM);
/// Message unique d'un panneau plein corps sans entrée à lister (ex.
/// « aucune erreur »).
pub const EMPTY_MESSAGE: Style = Style::new().add_modifier(Modifier::DIM.union(Modifier::ITALIC));

#[cfg(test)]
mod tests {
    use super::*;

    /// Les deux confusions identifiées en design.md : un échec
    /// d'exécution ne doit pas se confondre avec une erreur de
    /// chargement, et une exécution en cours ne doit pas se confondre
    /// avec la bordure du panneau qui a le focus.
    #[test]
    fn categories_that_could_be_confused_have_distinct_colors() {
        assert_ne!(FAILURE.fg, LOAD_ERROR.fg, "échec vs erreur de chargement");
        assert_ne!(RUNNING.fg, FOCUS.fg, "en cours vs focus actif");
    }

    #[test]
    fn every_category_has_a_foreground_color_except_pure_text_styles() {
        for style in [
            METHOD, SUCCESS, FAILURE, RUNNING, LOAD_ERROR, FOCUS, SECTION,
        ] {
            assert!(style.fg.is_some());
        }
        // TITLE, LABEL et EMPTY_MESSAGE se distinguent par le modificateur,
        // pas par la couleur : pas de fg attendu.
        for style in [TITLE, LABEL, EMPTY_MESSAGE] {
            assert!(style.fg.is_none());
        }
    }
}
