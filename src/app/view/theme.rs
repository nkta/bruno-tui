//! Palette de styles communs à tous les panneaux (capacité `visual-theme`).
//!
//! Une constante par catégorie d'information, réutilisée partout où la
//! catégorie apparaît, pour qu'une même catégorie porte toujours le même
//! style d'un panneau à l'autre.

use ratatui::style::{Color, Modifier, Style};

use super::status::StatusClass;

/// Fond de l'application, posé une seule fois sur toute la zone du
/// terminal au début de `view()`.
pub const BACKGROUND: Style = Style::new().bg(Color::Rgb(18, 22, 40));
/// Bordure d'un panneau qui n'a pas le focus.
pub const BORDER: Style = Style::new().fg(Color::Rgb(90, 150, 110));
/// Méthode HTTP (`GET`, `POST`, ...), dans l'arbre et le détail.
pub const METHOD: Style = Style::new().fg(Color::Rgb(120, 200, 150));
/// Dernier résultat connu d'une requête : succès.
pub const SUCCESS: Style = Style::new().fg(Color::Green);
/// Dernier résultat connu d'une requête : échec.
pub const FAILURE: Style = Style::new().fg(Color::Red);
/// Requête dont l'exécution est en cours.
pub const RUNNING: Style = Style::new().fg(Color::Blue);
/// Réponse de redirection (3xx), dans le panneau Statut. Distinct de
/// [`RUNNING`] et de [`METHOD`].
pub const REDIRECT: Style = Style::new().fg(Color::Cyan);
/// Réponse d'erreur client (4xx), dans le panneau Statut. Distinct de
/// [`FOCUS`] et de [`FAILURE`].
pub const CLIENT_ERROR: Style = Style::new().fg(Color::Yellow);
/// Nœud ou fichier en erreur de chargement, distinct de [`FAILURE`] :
/// un fichier invalide n'a pas la même cause qu'une requête qui échoue à
/// l'exécution.
pub const LOAD_ERROR: Style = Style::new().fg(Color::Magenta);
/// Bordure du panneau ayant le focus, distinct de [`RUNNING`].
pub const FOCUS: Style = Style::new()
    .fg(Color::Rgb(224, 138, 60))
    .add_modifier(Modifier::BOLD);
/// Titre du nœud sélectionné, dans le panneau de détail : le plus mis en
/// valeur des trois niveaux de hiérarchie du détail.
pub const TITLE: Style = Style::new().add_modifier(Modifier::BOLD);
/// Titre de section, dans le panneau de détail : second niveau, distinct
/// du titre de nœud par la couleur en plus du soulignement. Même teinte
/// que [`METHOD`] : aucune exigence de `visual-theme` n'impose qu'elles
/// soient distinctes, seule la distinction entre les trois niveaux du
/// détail (titre/section/libellé) est exigée.
pub const SECTION: Style = Style::new()
    .fg(Color::Rgb(120, 200, 150))
    .add_modifier(Modifier::BOLD.union(Modifier::UNDERLINED));
/// Libellé d'un champ (« Chemin », « Méthode », ...), dans le panneau de
/// détail : atténué pour que la valeur qui le suit ressorte davantage.
pub const LABEL: Style = Style::new().add_modifier(Modifier::DIM);
/// Message unique d'un panneau plein corps sans entrée à lister (ex.
/// « aucune erreur »).
pub const EMPTY_MESSAGE: Style = Style::new().add_modifier(Modifier::DIM.union(Modifier::ITALIC));

/// Style du badge de statut du panneau Statut : texte gras de la couleur
/// du fond sur fond de la couleur de catégorie ; style neutre sans fond
/// pour une classe sans catégorie (`visual-theme`).
pub fn status_badge(class: StatusClass) -> Style {
    let category = match class {
        StatusClass::Success => SUCCESS,
        StatusClass::Redirect => REDIRECT,
        StatusClass::ClientError => CLIENT_ERROR,
        StatusClass::Failure => FAILURE,
        StatusClass::Neutral => return LABEL,
    };
    category.fg.map_or(category, badge_on)
}

/// Badge sur fond `color` : texte gras de la couleur du fond de
/// l'application, pour un contraste maximal.
pub fn badge_on(color: Color) -> Style {
    let style = Style::new().bg(color).add_modifier(Modifier::BOLD);
    match BACKGROUND.bg {
        Some(background) => style.fg(background),
        None => style,
    }
}

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
        for other in [RUNNING, METHOD, SUCCESS, CLIENT_ERROR] {
            assert_ne!(REDIRECT.fg, other.fg, "redirection vs {other:?}");
        }
        for other in [FOCUS, FAILURE, LOAD_ERROR] {
            assert_ne!(CLIENT_ERROR.fg, other.fg, "erreur client vs {other:?}");
        }
    }

    #[test]
    fn every_category_has_a_foreground_color_except_pure_text_styles() {
        for style in [
            METHOD,
            SUCCESS,
            FAILURE,
            RUNNING,
            LOAD_ERROR,
            FOCUS,
            SECTION,
            BORDER,
            REDIRECT,
            CLIENT_ERROR,
        ] {
            assert!(style.fg.is_some());
        }
        // TITLE, LABEL et EMPTY_MESSAGE se distinguent par le modificateur,
        // pas par la couleur : pas de fg attendu.
        for style in [TITLE, LABEL, EMPTY_MESSAGE] {
            assert!(style.fg.is_none());
        }
    }

    #[test]
    fn background_has_a_color() {
        assert!(BACKGROUND.bg.is_some());
    }

    #[test]
    fn status_badge_uses_category_color_as_background() {
        for (class, category) in [
            (StatusClass::Success, SUCCESS),
            (StatusClass::Redirect, REDIRECT),
            (StatusClass::ClientError, CLIENT_ERROR),
            (StatusClass::Failure, FAILURE),
        ] {
            let badge = status_badge(class);
            assert_eq!(badge.bg, category.fg, "{class:?}");
            assert_eq!(badge.fg, BACKGROUND.bg, "{class:?}");
            assert!(badge.add_modifier.contains(Modifier::BOLD), "{class:?}");
        }
        let neutral = status_badge(StatusClass::Neutral);
        assert_eq!(neutral, LABEL);
        assert!(neutral.bg.is_none());
    }
}
