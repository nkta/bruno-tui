//! Palette de styles communs à tous les panneaux (capacité `visual-theme`).
//!
//! Une constante par catégorie d'information, réutilisée partout où la
//! catégorie apparaît, pour qu'une même catégorie porte toujours le même
//! style d'un panneau à l'autre.

use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, BorderType};

use super::status::StatusClass;

// Palette inspirée du thème sombre de Bruno bureau : fond gris très
// sombre, bordures discrètes, couleur réservée à l'information.

/// Gris de surface, un cran plus clair que le fond : champs, sélection,
/// gouttière, liste déroulante.
const SURFACE: Color = Color::Rgb(45, 45, 48);
/// Couleur d'accent : focus et titres de section.
const ACCENT: Color = Color::Rgb(214, 170, 90);

/// Fond de l'application, posé une seule fois sur toute la zone du
/// terminal au début de `view()` : gris très sombre, texte gris clair.
pub const BACKGROUND: Style = Style::new()
    .bg(Color::Rgb(30, 30, 30))
    .fg(Color::Rgb(212, 212, 212));
/// Bordure d'un panneau qui n'a pas le focus : gris discret.
pub const BORDER: Style = Style::new().fg(Color::Rgb(68, 68, 72));
/// Titre d'un panneau sans focus : gris lisible, plus clair que sa
/// bordure.
pub const PANEL_TITLE: Style = Style::new().fg(Color::Rgb(150, 150, 155));
/// Ligne sélectionnée d'une liste (arbre, historique, environnements...) :
/// fond gris de surface, texte blanc en gras.
pub const SELECTION: Style = Style::new()
    .bg(Color::Rgb(58, 58, 62))
    .fg(Color::Rgb(245, 245, 245))
    .add_modifier(Modifier::BOLD);
/// Gouttière des numéros de ligne de la réponse.
pub const GUTTER: Style = Style::new()
    .bg(Color::Rgb(37, 37, 38))
    .fg(Color::Rgb(110, 110, 115));
/// Méthode HTTP (`GET`, `POST`, ...), dans l'arbre et le détail.
pub const METHOD: Style = Style::new().fg(Color::Rgb(115, 195, 115));
/// Dernier résultat connu d'une requête : succès.
pub const SUCCESS: Style = Style::new().fg(Color::Rgb(115, 195, 115));
/// Dernier résultat connu d'une requête : échec.
pub const FAILURE: Style = Style::new().fg(Color::Rgb(240, 105, 90));
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
pub const FOCUS: Style = Style::new().fg(ACCENT).add_modifier(Modifier::BOLD);
/// Titre du nœud sélectionné, dans le panneau de détail : le plus mis en
/// valeur des trois niveaux de hiérarchie du détail.
pub const TITLE: Style = Style::new().add_modifier(Modifier::BOLD);
/// Titre de section, dans le panneau de détail : second niveau, distinct
/// du titre de nœud par sa couleur d'accent orange.
pub const SECTION: Style = Style::new()
    .fg(Color::Rgb(160, 160, 165))
    .add_modifier(Modifier::BOLD);
/// Libellé d'un champ (« Chemin », « Méthode », ...), dans le panneau de
/// détail : atténué pour que la valeur qui le suit ressorte davantage.
pub const LABEL: Style = Style::new().add_modifier(Modifier::DIM);
/// Message unique d'un panneau plein corps sans entrée à lister (ex.
/// « aucune erreur »).
pub const EMPTY_MESSAGE: Style = Style::new().add_modifier(Modifier::DIM.union(Modifier::ITALIC));
/// Zone de corps éditable, dans le panneau de détail : fond légèrement
/// plus clair que [`BACKGROUND`] pour la distinguer comme zone de saisie,
/// sans dépendre d'une bordure de widget (`improve-edit-field-legibility`).
pub const EDITABLE_BODY: Style = Style::new().bg(SURFACE);

/// Clé d'un objet JSON dans le corps de réponse.
pub const JSON_KEY: Style = Style::new().fg(Color::Rgb(156, 220, 254));
/// Valeur chaîne de caractères JSON dans le corps de réponse.
pub const JSON_STRING: Style = Style::new().fg(Color::Rgb(206, 145, 120));
/// Valeur numérique JSON dans le corps de réponse.
pub const JSON_NUMBER: Style = Style::new().fg(Color::Rgb(181, 206, 168));
/// Valeur booléenne JSON (`true`, `false`) dans le corps de réponse.
pub const JSON_BOOLEAN: Style = Style::new().fg(Color::Rgb(86, 156, 214));
/// Valeur `null` JSON dans le corps de réponse.
pub const JSON_NULL: Style = Style::new().fg(Color::Rgb(197, 134, 192));
/// Ponctuation syntaxique JSON (`{`, `}`, `[`, `]`, `:`, `,`) dans le corps de réponse.
pub const JSON_PUNCTUATION: Style = Style::new().fg(Color::Rgb(212, 212, 212));

/// Fil d'Ariane du pied de page : bande légèrement plus claire que le
/// fond.
pub const BREADCRUMB: Style = Style::new().bg(SURFACE);
/// Environnement actif dans l'en-tête, en liste déroulante encadrée.
pub const ENV_CHIP: Style = Style::new()
    .bg(SURFACE)
    .fg(Color::Rgb(235, 235, 235))
    .add_modifier(Modifier::BOLD);

/// Couleur d'une méthode HTTP, comme Bruno bureau : `GET` vert, `POST`
/// violet, `PUT` orange, `PATCH` bleu, `DELETE` rouge, les autres gris.
pub fn method_style(method: &str) -> Style {
    let color = match method.to_ascii_uppercase().as_str() {
        "GET" => return METHOD,
        "POST" => Color::Rgb(185, 140, 250),
        "PUT" => Color::Rgb(230, 160, 80),
        "PATCH" => Color::Rgb(120, 170, 240),
        "DELETE" => Color::Rgb(240, 105, 90),
        _ => Color::Rgb(170, 170, 175),
    };
    Style::new().fg(color)
}

/// Cadre commun à tous les panneaux : bordure arrondie.
pub fn bordered() -> Block<'static> {
    Block::bordered().border_type(BorderType::Rounded)
}

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
            JSON_KEY,
            JSON_STRING,
            JSON_NUMBER,
            JSON_BOOLEAN,
            JSON_NULL,
            JSON_PUNCTUATION,
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
        assert!(BACKGROUND.fg.is_some());
    }

    #[test]
    fn selection_and_gutter_stand_out_from_the_background() {
        assert_ne!(SELECTION.bg, BACKGROUND.bg);
        assert_ne!(GUTTER.bg, BACKGROUND.bg);
    }

    #[test]
    fn main_methods_have_distinct_colors() {
        let methods = ["GET", "POST", "PUT", "PATCH", "DELETE"];
        for (i, a) in methods.iter().enumerate() {
            for b in &methods[i + 1..] {
                assert_ne!(method_style(a).fg, method_style(b).fg, "{a} vs {b}");
            }
        }
        assert_eq!(method_style("get"), METHOD);
    }

    #[test]
    fn editable_body_background_is_distinct_from_app_background() {
        assert_ne!(EDITABLE_BODY.bg, BACKGROUND.bg);
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

    /// Les catégories syntaxiques JSON ont toutes des couleurs de premier plan
    /// distinctes entre elles.
    #[test]
    fn json_syntax_categories_have_distinct_colors() {
        let styles = [
            ("key", JSON_KEY),
            ("string", JSON_STRING),
            ("number", JSON_NUMBER),
            ("boolean", JSON_BOOLEAN),
            ("null", JSON_NULL),
            ("punctuation", JSON_PUNCTUATION),
        ];
        for (i, (name1, s1)) in styles.iter().enumerate() {
            for (name2, s2) in styles.iter().skip(i + 1) {
                assert_ne!(
                    s1.fg, s2.fg,
                    "styles JSON non distincts : {name1} vs {name2}"
                );
            }
        }
    }
}
