//! Table des raccourcis clavier et rendu du popup d'aide (`help-popup`).
//!
//! Définit l'ensemble des raccourcis documentés dans l'application sous une
//! unique table de données et produit le popup centré ouvert par `?`.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use super::{inner, panel, theme};
use crate::app::model::{HelpPopup, Model};

/// Séparateur entre les colonnes touche et effet du tableau.
pub const HELP_TABLE_SEPARATOR: &str = " │ ";

/// Entrée décrivant un raccourci clavier et son effet.
pub struct HelpEntry {
    pub key: &'static str,
    pub effect: &'static str,
}

/// Section thématique regroupant une série de raccourcis.
pub struct HelpSection {
    pub title: &'static str,
    pub entries: &'static [HelpEntry],
}

/// Table complète des raccourcis de l'application, organisée selon les
/// 7 sections attendues.
pub const HELP_SECTIONS: &[HelpSection] = &[
    HelpSection {
        title: "Global",
        entries: &[
            HelpEntry {
                key: "?",
                effect: "ouvrir / fermer l'aide",
            },
            HelpEntry {
                key: "Tab",
                effect: "faire tourner le focus (Collection, Détail, Réponse)",
            },
            HelpEntry {
                key: "z",
                effect: "basculer le mode plein écran (Collection, Détail, Réponse)",
            },
            HelpEntry {
                key: "Échap",
                effect: "ramener le focus à l'arbre",
            },
            HelpEntry {
                key: "q",
                effect: "quitter l'application",
            },
            HelpEntry {
                key: "Ctrl+C",
                effect: "quitter en toutes circonstances",
            },
            HelpEntry {
                key: "M",
                effect: "activer/désactiver la capture souris",
            },
        ],
    },
    HelpSection {
        title: "Collection",
        entries: &[
            HelpEntry {
                key: "↑/k, ↓/j",
                effect: "naviguer dans l'arbre",
            },
            HelpEntry {
                key: "→/l, ←/h",
                effect: "déplier/replier un dossier",
            },
            HelpEntry {
                key: "Début/g, Fin/G",
                effect: "début, fin de la liste",
            },
            HelpEntry {
                key: "PageUp/PageDown",
                effect: "page précédente/suivante",
            },
            HelpEntry {
                key: "r",
                effect: "lancer la requête (ou récursivement un dossier)",
            },
            HelpEntry {
                key: "Ctrl+X",
                effect: "annuler l'exécution en cours",
            },
            HelpEntry {
                key: "/",
                effect: "chercher dans la collection",
            },
            HelpEntry {
                key: "q",
                effect: "quitter",
            },
        ],
    },
    HelpSection {
        title: "Détail",
        entries: &[
            HelpEntry {
                key: "↑/k, ↓/j",
                effect: "défiler dans le détail",
            },
            HelpEntry {
                key: "Début/g, Fin/G",
                effect: "début, fin du détail",
            },
            HelpEntry {
                key: "Entrée / e",
                effect: "ouvrir une session d'édition",
            },
            HelpEntry {
                key: "/",
                effect: "chercher dans le détail",
            },
            HelpEntry {
                key: "n/N",
                effect: "occurrence suivante/précédente",
            },
            HelpEntry {
                key: "v",
                effect: "sélection visuelle de lignes",
            },
            HelpEntry {
                key: "y",
                effect: "copier la sélection dans le presse-papiers",
            },
            HelpEntry {
                key: "Échap",
                effect: "ramener le focus à l'arbre",
            },
            HelpEntry {
                key: "q",
                effect: "quitter",
            },
        ],
    },
    HelpSection {
        title: "Édition d'une requête",
        entries: &[
            HelpEntry {
                key: "↑/k, ↓/j",
                effect: "champ précédent, suivant",
            },
            HelpEntry {
                key: "Entrée",
                effect: "modifier la valeur (ou ouvrir sélecteur de méthode)",
            },
            HelpEntry {
                key: "Espace",
                effect: "activer ou désactiver l'en-tête ou le paramètre",
            },
            HelpEntry {
                key: "a",
                effect: "ajouter une entrée dans la section du champ (clé, puis valeur)",
            },
            HelpEntry {
                key: "d",
                effect: "supprimer l'en-tête ou le paramètre, sans confirmation",
            },
            HelpEntry {
                key: "c",
                effect: "renommer la clé de l'en-tête ou du paramètre",
            },
            HelpEntry {
                key: "Ctrl+S",
                effect: "enregistrer les modifications",
            },
            HelpEntry {
                key: "Échap",
                effect: "fermer la session d'édition (confirmation si modifiée)",
            },
            HelpEntry {
                key: "Entrée / Tab",
                effect: "en saisie : valider la valeur (Entrée = saut de ligne dans le corps)",
            },
            HelpEntry {
                key: "Échap (saisie)",
                effect: "en saisie : annuler la saisie en cours",
            },
        ],
    },
    HelpSection {
        title: "Réponse",
        entries: &[
            HelpEntry {
                key: "↑/k, ↓/j",
                effect: "défiler dans la réponse",
            },
            HelpEntry {
                key: "Début/g, Fin/G",
                effect: "début, fin de la réponse",
            },
            HelpEntry {
                key: "←/→ (ou Entrée)",
                effect: "onglet précédent/suivant",
            },
            HelpEntry {
                key: "/",
                effect: "chercher dans l'onglet courant",
            },
            HelpEntry {
                key: "n/N",
                effect: "occurrence suivante/précédente",
            },
            HelpEntry {
                key: "|",
                effect: "filtrer le corps avec une expression jq",
            },
            HelpEntry {
                key: "v",
                effect: "sélection visuelle de lignes",
            },
            HelpEntry {
                key: "y",
                effect: "copier la sélection dans le presse-papiers",
            },
            HelpEntry {
                key: "Ctrl+E",
                effect: "ouvrir le corps brut dans l'éditeur externe",
            },
            HelpEntry {
                key: "Échap",
                effect: "ramener le focus à l'arbre",
            },
            HelpEntry {
                key: "q",
                effect: "quitter",
            },
        ],
    },
    HelpSection {
        title: "Environnement (panneau et popup)",
        entries: &[
            HelpEntry {
                key: "E",
                effect: "donner le focus au panneau Environnement",
            },
            HelpEntry {
                key: "↑/k, ↓/j",
                effect: "naviguer dans la liste des environnements",
            },
            HelpEntry {
                key: "Entrée / →",
                effect: "activer l'environnement sous le curseur",
            },
            HelpEntry {
                key: "e",
                effect: "ouvrir le popup d'édition de ses variables",
            },
            HelpEntry {
                key: "Entrée (popup)",
                effect: "modifier la valeur (variable désactivée non éditable)",
            },
            HelpEntry {
                key: "a (popup)",
                effect: "ajouter une variable (clé, puis valeur)",
            },
            HelpEntry {
                key: "d (popup)",
                effect: "supprimer la variable sous le curseur",
            },
            HelpEntry {
                key: "Ctrl+S (popup)",
                effect: "enregistrer les variables",
            },
            HelpEntry {
                key: "Échap (popup)",
                effect: "annuler la saisie en cours, sinon fermer le popup",
            },
        ],
    },
    HelpSection {
        title: "Diagnostics / Historique / Secrets",
        entries: &[
            HelpEntry {
                key: "D",
                effect: "ouvrir / fermer le panneau Diagnostics",
            },
            HelpEntry {
                key: "→ (Diagnostics)",
                effect: "sauter au nœud concerné dans l'arbre",
            },
            HelpEntry {
                key: "H",
                effect: "ouvrir / fermer le panneau Historique",
            },
            HelpEntry {
                key: "r (Historique)",
                effect: "relancer l'entrée sélectionnée",
            },
            HelpEntry {
                key: "S",
                effect: "ouvrir / fermer le panneau Variables secrètes",
            },
            HelpEntry {
                key: "a (Secrets)",
                effect: "ajouter une variable secrète",
            },
            HelpEntry {
                key: "Entrée (Secrets)",
                effect: "saisir la valeur de la variable secrète",
            },
            HelpEntry {
                key: "d (Secrets)",
                effect: "oublier la variable secrète",
            },
            HelpEntry {
                key: "Échap",
                effect: "ramener le focus à l'arbre",
            },
        ],
    },
    HelpSection {
        title: "Campagne (TNR)",
        entries: &[
            HelpEntry {
                key: "C",
                effect: "ouvrir / fermer le bilan de la dernière campagne",
            },
            HelpEntry {
                key: "Entrée / →",
                effect: "sélectionner la requête en échec et ouvrir son détail",
            },
            HelpEntry {
                key: "] / [",
                effect: "requête en échec suivante / précédente",
            },
        ],
    },
];

/// Largeur maximale requise par la colonne touche, bornée pour garder
/// un alignement harmonieux du tableau.
pub fn help_key_column_width() -> usize {
    HELP_SECTIONS
        .iter()
        .flat_map(|s| s.entries.iter())
        .map(|e| Line::raw(e.key).width())
        .max()
        .unwrap_or(18)
        .max(16)
}

/// Génère toutes les lignes de texte du corps de l'aide (titres de sections,
/// lignes vides de séparation et raccourcis).
pub fn help_content_lines(key_width: usize) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    for (index, section) in HELP_SECTIONS.iter().enumerate() {
        if index > 0 {
            lines.push(Line::raw(""));
        }
        lines.push(Line::styled(
            format!("[ {} ]", section.title),
            theme::SECTION,
        ));
        for entry in section.entries {
            lines.push(Line::from(vec![
                Span::styled(format!("{:<key_width$}", entry.key), Style::default()),
                Span::styled(HELP_TABLE_SEPARATOR, theme::LABEL),
                Span::raw(entry.effect),
            ]));
        }
    }
    lines
}

/// Nombre total de lignes de contenu de l'aide.
pub fn help_content_lines_count() -> usize {
    let mut count = 0;
    for (index, section) in HELP_SECTIONS.iter().enumerate() {
        if index > 0 {
            count += 1; // ligne vide
        }
        count += 1; // titre
        count += section.entries.len();
    }
    count
}

/// Calcule la zone centrée du popup d'aide sur l'écran.
pub fn help_popup_area(screen: Rect) -> Rect {
    let width = (screen.width * 75 / 100).clamp(48, 76).min(screen.width);
    let height = screen.height.saturating_sub(4).max(6).min(screen.height);
    let x = screen.x + screen.width.saturating_sub(width) / 2;
    let y = screen.y + screen.height.saturating_sub(height) / 2;
    Rect::new(x, y, width, height)
}

/// Défilement vertical maximal possible dans l'aide pour l'état du modèle.
pub fn help_max_scroll(model: &Model) -> u16 {
    let screen = Rect::new(0, 0, model.size.0, model.size.1);
    let popup = help_popup_area(screen);
    let inner_area = inner(popup);
    let content_height = inner_area.height.saturating_sub(1);
    let total_lines = u16::try_from(help_content_lines_count()).unwrap_or(u16::MAX);
    total_lines.saturating_sub(content_height)
}

/// Dessine le popup d'aide centré : bordure, ligne d'en-tête (Touche │ Effet)
/// et contenu défilable par sections.
pub fn render_help_popup(help: &HelpPopup, frame: &mut Frame, area: Rect) {
    let block = panel(" Aide ", true);
    let inner_area = inner(area);
    frame.render_widget(block, area);

    if inner_area.width == 0 || inner_area.height == 0 {
        return;
    }

    let key_width = help_key_column_width();
    let header = Line::styled(
        format!("{:<key_width$}{HELP_TABLE_SEPARATOR}Effet", "Touche"),
        theme::LABEL,
    );

    if inner_area.height == 1 {
        frame.render_widget(Paragraph::new(header), inner_area);
        return;
    }

    // En-tête des colonnes sur la première ligne intérieure
    let header_area = Rect::new(inner_area.x, inner_area.y, inner_area.width, 1);
    frame.render_widget(Paragraph::new(header), header_area);

    // Contenu défilant sur les lignes restantes
    let content_area = Rect::new(
        inner_area.x,
        inner_area.y + 1,
        inner_area.width,
        inner_area.height - 1,
    );
    let lines = help_content_lines(key_width);
    frame.render_widget(Paragraph::new(lines).scroll((help.scroll, 0)), content_area);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sections_count_and_expected_titles() {
        assert_eq!(HELP_SECTIONS.len(), 8);
        let titles: Vec<&str> = HELP_SECTIONS.iter().map(|s| s.title).collect();
        assert_eq!(
            titles,
            vec![
                "Global",
                "Collection",
                "Détail",
                "Édition d'une requête",
                "Réponse",
                "Environnement (panneau et popup)",
                "Diagnostics / Historique / Secrets",
                "Campagne (TNR)",
            ]
        );
    }

    #[test]
    fn every_section_has_entries() {
        for section in HELP_SECTIONS {
            assert!(
                !section.entries.is_empty(),
                "la section {} ne doit pas être vide",
                section.title
            );
            for entry in section.entries {
                assert!(!entry.key.is_empty());
                assert!(!entry.effect.is_empty());
            }
        }
    }

    #[test]
    fn lines_count_matches_generated_lines() {
        let key_width = help_key_column_width();
        let lines = help_content_lines(key_width);
        assert_eq!(lines.len(), help_content_lines_count());
    }

    #[test]
    fn popup_area_is_centered_and_within_bounds() {
        let screen = Rect::new(0, 0, 100, 30);
        let area = help_popup_area(screen);
        assert!(area.width <= screen.width);
        assert!(area.height <= screen.height);
        assert_eq!(area.x, (screen.width - area.width) / 2);
        assert_eq!(area.y, (screen.height - area.height) / 2);
    }
}
