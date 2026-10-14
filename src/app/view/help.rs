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
                key: "Ctrl+K",
                effect: "recherche globale (nom, méthode, chemin, URL)",
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
                effect: "lancer la requête (ou récursivement un dossier / la collection) ; refusé pour gRPC/WebSocket",
            },
            HelpEntry {
                key: "c",
                effect: "sélectionner la collection racine (affiche collection.bru dans le Détail)",
            },
            HelpEntry {
                key: "→/l, ←/h (en-tête)",
                effect: "workspace : déplier/replier une collection",
            },
            HelpEntry {
                key: "↑/↓ (autre collection)",
                effect: "workspace : la rend active (environnement, secrets, historique, campagne) ; refusé pendant une exécution",
            },
            HelpEntry {
                key: "O",
                effect: "options de campagne (tags, TLS, données, sandbox, rapports)",
            },
            HelpEntry {
                key: "Ctrl+X",
                effect: "annuler l'exécution en cours",
            },
            HelpEntry {
                key: "f",
                effect: "filtrer l'arbre",
            },
            HelpEntry {
                key: "a",
                effect: "nouvelle requête dans le dossier sélectionné (ou celui de la requête) ; sur l'en-tête : à la racine",
            },
            HelpEntry {
                key: "A",
                effect: "nouveau dossier, au même endroit",
            },
            HelpEntry {
                key: "F2",
                effect: "renommer la ligne (nom affiché ; fichier ou dossier si la case est cochée)",
            },
            HelpEntry {
                key: "p",
                effect: "cloner la ligne (« <nom> copie » ; un dossier est copié en entier)",
            },
            HelpEntry {
                key: "Maj+↑, Maj+↓",
                effect: "monter, descendre la ligne parmi ses frères (seq réécrits comme Bruno)",
            },
            HelpEntry {
                key: "m",
                effect: "déplacer la ligne vers un autre dossier de la collection (popup « Déplacer vers » ; dans le Détail, m ouvre le popup Requête)",
            },
            HelpEntry {
                key: "Popup Déplacer : saisie, ↑↓, Entrée, Échap",
                effect: "filtrer les dossiers ; choisir ; déplacer (un seul rename, seq inchangé, frères de la source renumérotés) ; fermer",
            },
            HelpEntry {
                key: "d",
                effect: "mettre à la corbeille, après confirmation (y/n), puis renuméroter les frères comme Bruno ; jamais d'effacement définitif",
            },
            HelpEntry {
                key: "B",
                effect: "réglages de la collection (bruno.json : presets, proxy, certificats) ; pas en YAML, refusé pendant une exécution",
            },
            HelpEntry {
                key: "Réglages : Tab, Maj+Tab",
                effect: "partie suivante, précédente (Presets, Proxy, Certificats)",
            },
            HelpEntry {
                key: "Réglages : ↑↓, ←→",
                effect: "ligne précédente, suivante ; valeur précédente, suivante d'un choix",
            },
            HelpEntry {
                key: "Réglages : Entrée, Espace",
                effect: "modifier la valeur (Entrée valide, Échap annule) ; cocher la case ; activer / désactiver un certificat",
            },
            HelpEntry {
                key: "Réglages : a, d",
                effect: "Certificats : ajouter ; supprimer après confirmation (y/n)",
            },
            HelpEntry {
                key: "Réglages : Ctrl+R",
                effect: "afficher une variable {{…}} masquée ; un secret littéral reste masqué",
            },
            HelpEntry {
                key: "Réglages : Ctrl+S, Échap",
                effect: "enregistrer (seule la clé modifiée de bruno.json change) ; fermer (confirmation si modifié)",
            },
            HelpEntry {
                key: "Popup fichier : ↑↓, Tab",
                effect: "ligne précédente, suivante (la frappe va au champ sous le curseur)",
            },
            HelpEntry {
                key: "Popup fichier : ←→, Espace",
                effect: "changer le type, la méthode ; cocher ou décocher la case",
            },
            HelpEntry {
                key: "Popup fichier : Entrée",
                effect: "ligne suivante ; sur la dernière ligne, créer ou appliquer",
            },
            HelpEntry {
                key: "Popup fichier : Ctrl+S, Échap",
                effect: "créer ou appliquer ; fermer sans rien écrire",
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
                key: "←/h, →/l",
                effect: "onglet précédent/suivant (requête, dossier ou collection)",
            },
            HelpEntry {
                key: "Entrée / e",
                effect: "ouvrir le popup d'édition de l'onglet affiché (refusé en YAML et pour gRPC/WebSocket)",
            },
            HelpEntry {
                key: "m",
                effect: "ouvrir le popup Requête (méthode et URL), depuis n'importe quel onglet",
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
        title: "Popups d'édition",
        entries: &[
            HelpEntry {
                key: "↑/k, ↓/j",
                effect: "ligne précédente, suivante",
            },
            HelpEntry {
                key: "Tab, Maj+Tab",
                effect: "partie suivante, précédente (ex. pré-requête / post-réponse)",
            },
            HelpEntry {
                key: "Début/g, Fin/G",
                effect: "première, dernière ligne du popup",
            },
            HelpEntry {
                key: "Entrée",
                effect: "modifier la valeur ; liste d'un choix ; éditeur externe pour scripts, tests, docs",
            },
            HelpEntry {
                key: "←/h, →/l",
                effect: "sur un choix (méthode, mode, placement) : valeur précédente, suivante",
            },
            HelpEntry {
                key: "a",
                effect: "ajouter une ligne au tableau (clé, puis valeur)",
            },
            HelpEntry {
                key: "c",
                effect: "renommer la clé (hors @file)",
            },
            HelpEntry {
                key: "d",
                effect: "supprimer la ligne, sans confirmation",
            },
            HelpEntry {
                key: "Espace",
                effect: "activer ou désactiver la ligne",
            },
            HelpEntry {
                key: "Ctrl+R",
                effect: "afficher / masquer un secret d'authentification en saisie",
            },
            HelpEntry {
                key: "Ctrl+S",
                effect: "enregistrer puis fermer (confirmation si un bloc auth non vide est supprimé)",
            },
            HelpEntry {
                key: "Échap",
                effect: "fermer le popup (confirmation y/n s'il est modifié)",
            },
            HelpEntry {
                key: "Entrée (saisie)",
                effect: "valider ; saut de ligne dans une zone de texte",
            },
            HelpEntry {
                key: "Tab (saisie)",
                effect: "valider ; sortir d'une zone de texte",
            },
            HelpEntry {
                key: "Échap (saisie)",
                effect: "annuler la saisie en cours",
            },
            HelpEntry {
                key: "Clic",
                effect: "sélectionner une ligne ; un clic hors du popup ne fait rien",
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
                key: "c",
                effect: "copier en commande curl (en-têtes d'auth masqués)",
            },
            HelpEntry {
                key: "u",
                effect: "copier en commande curl complète (avec secrets)",
            },
            HelpEntry {
                key: "Ctrl+E",
                effect: "ouvrir le corps brut dans l'éditeur externe",
            },
            HelpEntry {
                key: "x",
                effect: "exporter le corps brut dans un fichier",
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
                effect: "ouvrir / fermer le panneau Environnement (aussi : clic sur la liste en haut à droite)",
            },
            HelpEntry {
                key: "↑/k, ↓/j",
                effect: "naviguer dans la liste des environnements",
            },
            HelpEntry {
                key: "Début/g, Fin/G",
                effect: "début, fin de la liste",
            },
            HelpEntry {
                key: "Entrée / →",
                effect: "activer l'environnement sous le curseur",
            },
            HelpEntry {
                key: "a",
                effect: "ouvrir le popup « Nouvel environnement » (pas en YAML)",
            },
            HelpEntry {
                key: "F2",
                effect: "renommer l'environnement sous le curseur",
            },
            HelpEntry {
                key: "p",
                effect: "cloner l'environnement sous le curseur (« <nom> copie »)",
            },
            HelpEntry {
                key: "d",
                effect: "mettre l'environnement à la corbeille, après confirmation (y/n)",
            },
            HelpEntry {
                key: "e",
                effect: "ouvrir le popup d'édition de ses variables (pas en YAML)",
            },
            HelpEntry {
                key: "Entrée (popup)",
                effect: "modifier la valeur (variable désactivée non éditable)",
            },
            HelpEntry {
                key: "Espace (popup)",
                effect: "basculer la variable entre activée et désactivée (~)",
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
                effect: "sélectionner la requête et ouvrir son détail",
            },
            HelpEntry {
                key: "] / [",
                effect: "requête suivante / précédente dans la liste",
            },
            HelpEntry {
                key: "f",
                effect: "changer le filtre (Échecs / Réussis / Ignorés / Tous)",
            },
            HelpEntry {
                key: "R",
                effect: "relancer uniquement les requêtes en échec",
            },
            HelpEntry {
                key: "r",
                effect: "relancer la campagne avec les options courantes",
            },
            HelpEntry {
                key: "O",
                effect: "options de campagne et export des rapports",
            },
            HelpEntry {
                key: "t",
                effect: "basculer liste / tableau comparatif (campagne multi-environnements)",
            },
            HelpEntry {
                key: "←→ (tableau)",
                effect: "changer d'environnement ; Entrée affiche son résultat dans Réponse",
            },
            HelpEntry {
                key: "f (tableau)",
                effect: "filtre Tous / Divergences / Échecs / Réussis / Ignorés",
            },
            HelpEntry {
                key: "O puis Espace",
                effect: "cocher des environnements : un bru run par environnement",
            },
            HelpEntry {
                key: "Ctrl+X",
                effect: "annuler l'exécution en cours et la suite de la série",
            },
            HelpEntry {
                key: "O (Environnements)",
                effect: "--env-file, --global-env et --workspace-path de bru",
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

/// Largeur d'affichage de la plus longue ligne de l'aide, en colonnes.
pub fn help_content_width() -> usize {
    help_content_lines(help_key_column_width())
        .iter()
        .map(Line::width)
        .max()
        .unwrap_or(0)
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
    // Assez large pour la plus longue description (bordures comprises),
    // sans dépasser l'écran moins une marge : une ligne par raccourci, le
    // calcul du défilement en dépend.
    let needed = u16::try_from(help_content_width() + 2).unwrap_or(u16::MAX);
    let width = needed
        .min(screen.width.saturating_sub(4))
        .max(48)
        .min(screen.width);
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
                "Popups d'édition",
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

    /// `Début` et `Fin` des popups d'édition sont documentés.
    #[test]
    fn edit_popups_document_home_and_end() {
        let section = HELP_SECTIONS
            .iter()
            .find(|s| s.title == "Popups d'édition")
            .expect("section");
        assert!(section.entries.iter().any(|e| e.key == "Début/g, Fin/G"));
    }

    /// Les opérations de fichiers de l'arbre sont documentées
    /// (`docs/edition-phase3.md` § 3).
    #[test]
    fn collection_documents_file_operations() {
        let section = HELP_SECTIONS
            .iter()
            .find(|s| s.title == "Collection")
            .expect("section");
        for key in ["a", "A", "F2", "p", "Maj+↑, Maj+↓", "m", "d"] {
            assert!(section.entries.iter().any(|e| e.key == key), "{key}");
        }
    }

    /// `B` ouvre les réglages de la collection (`docs/reglages-collection.md` § 3).
    #[test]
    fn collection_documents_the_settings_popup() {
        let section = HELP_SECTIONS
            .iter()
            .find(|s| s.title == "Collection")
            .expect("section");
        assert!(section.entries.iter().any(|e| e.key == "B"));
        for key in ["Réglages : a, d", "Réglages : Ctrl+S, Échap"] {
            assert!(section.entries.iter().any(|e| e.key == key), "{key}");
        }
    }

    /// Les opérations de fichiers du panneau environnement sont documentées.
    #[test]
    fn environment_documents_file_operations_and_toggle() {
        let section = HELP_SECTIONS
            .iter()
            .find(|s| s.title == "Environnement (panneau et popup)")
            .expect("section");
        for key in ["Début/g, Fin/G", "a", "F2", "p", "d", "Espace (popup)"] {
            assert!(section.entries.iter().any(|e| e.key == key), "{key}");
        }
    }
}
