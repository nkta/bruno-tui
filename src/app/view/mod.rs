//! Rendu de l'interface.
//!
//! `view` est pur : il lit le modèle et dessine, sans I/O ni modification
//! d'état. `layout` est partagé avec `update`, qui en déduit la hauteur des
//! panneaux pour garder la sélection visible et borner le défilement.

pub mod detail;
pub mod tree;

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, List, ListItem, ListState, Paragraph, Wrap};

use super::model::{CollectionState, Focus, Model, RunFailure, StatusMessage};
use crate::collection::TreeNode;

/// Largeur minimale du terminal.
pub const MIN_WIDTH: u16 = 40;
/// Hauteur minimale du terminal.
pub const MIN_HEIGHT: u16 = 10;
/// Largeur minimale du panneau de l'arbre.
const MIN_TREE_WIDTH: u16 = 24;

const TOO_SMALL: &str = "Terminal trop petit : agrandir à 40×10 au moins.";

/// Zones de l'écran.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Areas {
    pub title: Rect,
    /// Arbre et détail réunis.
    pub body: Rect,
    pub tree: Rect,
    pub detail: Rect,
    pub status: Rect,
}

/// Découpe l'écran ; `None` sous la taille minimale.
pub fn layout(area: Rect) -> Option<Areas> {
    if area.width < MIN_WIDTH || area.height < MIN_HEIGHT {
        return None;
    }
    let body_height = area.height - 2;
    let tree_width = (area.width * 35 / 100).max(MIN_TREE_WIDTH);
    let body = Rect::new(area.x, area.y + 1, area.width, body_height);
    Some(Areas {
        title: Rect::new(area.x, area.y, area.width, 1),
        body,
        tree: Rect::new(body.x, body.y, tree_width, body_height),
        detail: Rect::new(
            body.x + tree_width,
            body.y,
            area.width - tree_width,
            body_height,
        ),
        status: Rect::new(area.x, area.y + area.height - 1, area.width, 1),
    })
}

/// Zones de l'écran pour une taille de terminal.
pub fn layout_for(size: (u16, u16)) -> Option<Areas> {
    layout(Rect::new(0, 0, size.0, size.1))
}

/// Intérieur d'un panneau bordé.
pub fn inner(area: Rect) -> Rect {
    Block::bordered().inner(area)
}

/// Dessine l'interface.
pub fn view(model: &Model, frame: &mut Frame) {
    let Some(areas) = layout(frame.area()) else {
        frame.render_widget(
            Paragraph::new(TOO_SMALL).wrap(Wrap { trim: true }),
            frame.area(),
        );
        return;
    };

    frame.render_widget(Paragraph::new(title_line(model)), areas.title);

    match &model.collection {
        CollectionState::Loading => {
            frame.render_widget(
                Paragraph::new("Chargement…").block(panel(" Collection ", true)),
                areas.tree,
            );
            frame.render_widget(panel(" Détail ", false), areas.detail);
        }
        CollectionState::Failed(error) => {
            let lines = vec![
                Line::styled(
                    "Impossible de charger la collection",
                    Style::new().fg(Color::Red).add_modifier(Modifier::BOLD),
                ),
                Line::default(),
                Line::raw(error.to_string()),
            ];
            frame.render_widget(
                Paragraph::new(lines)
                    .wrap(Wrap { trim: false })
                    .block(panel(" Erreur ", true)),
                areas.body,
            );
        }
        CollectionState::Loaded(_) => {
            render_tree(model, frame, areas.tree);
            frame.render_widget(
                Paragraph::new(detail::render_text(model))
                    .wrap(Wrap { trim: false })
                    .scroll((model.detail_scroll, 0))
                    .block(panel(" Détail ", model.focus == Focus::Detail)),
                areas.detail,
            );
        }
    }

    frame.render_widget(
        Paragraph::new(status_line(model)).style(Style::new().add_modifier(Modifier::DIM)),
        areas.status,
    );
}

fn title_line(model: &Model) -> Line<'static> {
    let title = match &model.collection {
        CollectionState::Loading => format!("Chargement de {}…", model.source.display()),
        CollectionState::Loaded(collection) => collection.name.clone(),
        CollectionState::Failed(_) => "Erreur de chargement".to_owned(),
    };
    Line::from(vec![
        Span::styled("bruno-tui", Style::new().add_modifier(Modifier::BOLD)),
        Span::raw(" · "),
        Span::raw(title),
    ])
}

/// Statut d'exécution d'un nœud, pour l'indicateur de la ligne de l'arbre.
fn run_status(model: &Model, node: &TreeNode) -> tree::RunStatus {
    let TreeNode::Request(request) = node else {
        return tree::RunStatus::None;
    };
    if model
        .run
        .active
        .as_ref()
        .is_some_and(|active| active.target == request.path)
    {
        return tree::RunStatus::Running;
    }
    match model.run.outcomes.get(&request.path) {
        Some(outcome) if outcome.result.is_failure() => tree::RunStatus::Failure,
        Some(_) => tree::RunStatus::Success,
        None => tree::RunStatus::None,
    }
}

/// Message décrivant l'exécution en cours, s'il y en a une.
fn active_run_message(model: &Model) -> Option<String> {
    let active = model.run.active.as_ref()?;
    Some(format!(
        "Exécution de {} en cours…  Ctrl+X annuler",
        active.target.display()
    ))
}

/// Message décrivant la dernière exécution n'ayant produit aucun résultat
/// exploitable, s'il y en a un.
fn last_failure_message(model: &Model) -> Option<String> {
    match model.run.last_failure.as_ref()? {
        RunFailure::Error { target, error } => Some(format!(
            "Échec de l'exécution de {} : {error}",
            target.display()
        )),
        RunFailure::Cancelled { target } => {
            Some(format!("Exécution de {} annulée", target.display()))
        }
    }
}

/// Ligne de saisie de filtre, tant qu'elle est ouverte : remplace la
/// barre d'état, à la place des rappels de touches habituels.
fn filter_input_line(model: &Model) -> Option<String> {
    let filter = model.filter.as_ref()?;
    filter.editing.then(|| format!("|{}", filter.draft))
}

/// Ligne de saisie de recherche, tant qu'elle est ouverte : remplace la
/// barre d'état, à la place des rappels de touches habituels.
fn search_input_line(model: &Model) -> Option<String> {
    let search = model.search.as_ref()?;
    search.editing.then(|| format!("/{}", search.draft))
}

fn status_message_text(message: &StatusMessage) -> String {
    match message {
        StatusMessage::Copied => "Copié dans le presse-papiers".to_owned(),
        StatusMessage::ClipboardError(reason) => format!("Échec de la copie : {reason}"),
        StatusMessage::NoMatch => "Aucune correspondance".to_owned(),
    }
}

fn status_line(model: &Model) -> String {
    if let Some(line) = filter_input_line(model) {
        return line;
    }
    if let Some(line) = search_input_line(model) {
        return line;
    }
    if let Some(message) = active_run_message(model) {
        return message;
    }
    if let Some(message) = last_failure_message(model) {
        return message;
    }
    if let Some(message) = &model.last_status {
        return status_message_text(message);
    }
    match (&model.collection, model.focus) {
        (CollectionState::Loaded(_), Focus::Tree) => {
            "↑↓ naviguer  → déplier  ← replier  r lancer  / chercher  Tab détail  q quitter"
                .to_owned()
        }
        (CollectionState::Loaded(_), Focus::Detail) => {
            "↑↓ défiler  Début/Fin  / chercher  n/N suivant  v sélection  y copier  Échap arbre  q quitter"
                .to_owned()
        }
        _ => "q quitter".to_owned(),
    }
}

fn panel(title: &'static str, focused: bool) -> Block<'static> {
    let style = if focused {
        Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD)
    } else {
        Style::new()
    };
    Block::bordered().title(title).border_style(style)
}

fn render_tree(model: &Model, frame: &mut Frame, area: Rect) {
    let block = panel(" Collection ", model.focus == Focus::Tree);
    let state = &model.tree;
    if state.rows.is_empty() {
        frame.render_widget(Paragraph::new("collection vide").block(block), area);
        return;
    }
    let height = usize::from(inner(area).height);
    let end = (state.offset + height).min(state.rows.len());
    let start = state.offset.min(end);
    let items: Vec<ListItem> = state.rows[start..end]
        .iter()
        .filter_map(|row| {
            let node = model.node_at(&row.address)?;
            Some(ListItem::new(tree::row_line(
                node,
                row.depth,
                model.is_expanded(node),
                run_status(model, node),
            )))
        })
        .collect();
    let selected = state
        .selected
        .checked_sub(start)
        .filter(|index| *index < end - start);
    let mut list_state = ListState::default().with_selected(selected);
    frame.render_stateful_widget(
        List::new(items)
            .block(block)
            .highlight_style(Style::new().add_modifier(Modifier::REVERSED)),
        area,
        &mut list_state,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::message::Message;
    use crate::app::test_support::{fixture, loaded_model, render, select};
    use crate::app::update::update;
    use crate::collection::{Collection, LoadError};

    #[test]
    fn initial_tree_lists_first_level_in_order_with_error_marks() {
        let model = loaded_model((100, 30));
        let lines = render(&model, 100, 30);
        assert!(
            lines[0].contains("bruno-tui · parser-cases"),
            "{}",
            lines[0]
        );
        // Colonnes de l'arbre : 35 % de 100, bordures exclues.
        let tree: Vec<String> = lines[2..13]
            .iter()
            .map(|l| {
                l.chars()
                    .skip(1)
                    .take(33)
                    .collect::<String>()
                    .trim_end()
                    .to_owned()
            })
            .collect();
        assert_eq!(
            tree,
            [
                "▸ Groupe",
                "GET    ping",
                "POST   post-json",
                "GET    scripted",
                "▸ badmeta ✗",
                "✗ broken.bru",
                "▸ misc",
                "GET    multiline",
                "✗ no-method.bru",
                "GET    no-seq",
                "GET    unknown-block",
            ]
        );
        assert!(lines[13].starts_with('│') && lines[13].chars().nth(1) == Some(' '));
        assert!(lines[29].contains("q quitter"), "{}", lines[29]);
    }

    #[test]
    fn loading_and_failed_states() {
        let mut model = Model::new("/somewhere/else".into(), (100, 30));
        let lines = render(&model, 100, 30);
        assert!(
            lines[0].contains("Chargement de /somewhere/else"),
            "{}",
            lines[0]
        );

        update(
            &mut model,
            Message::CollectionLoaded(Err(LoadError::NotACollection {
                path: "/somewhere/else".into(),
            })),
        );
        let screen = render(&model, 100, 30).join("\n");
        assert!(
            screen.contains("Impossible de charger la collection"),
            "{screen}"
        );
        assert!(screen.contains("/somewhere/else"), "{screen}");
        assert!(screen.contains("aucun bruno.json"), "{screen}");
    }

    #[test]
    fn empty_collection() {
        let mut model = Model::new(fixture(), (100, 30));
        update(
            &mut model,
            Message::CollectionLoaded(Ok(Collection {
                root: "/vide".into(),
                name: "vide".into(),
                settings: None,
                tree: Vec::new(),
                environments: Vec::new(),
            })),
        );
        let screen = render(&model, 100, 30).join("\n");
        assert!(screen.contains("collection vide"), "{screen}");
    }

    #[test]
    fn too_small_and_absurd_sizes() {
        let model = loaded_model((30, 8));
        let screen = render(&model, 30, 8).join("\n");
        assert!(screen.contains("trop petit"), "{screen}");
        assert!(!screen.contains("Groupe"), "{screen}");

        let model = loaded_model((1, 1));
        render(&model, 1, 1);
    }

    #[test]
    fn layout_respects_minimums() {
        assert_eq!(layout_for((39, 30)), None);
        assert_eq!(layout_for((100, 9)), None);
        let areas = layout_for((40, 10)).expect("taille minimale");
        assert_eq!(areas.tree.width, MIN_TREE_WIDTH);
        assert_eq!(areas.tree.width + areas.detail.width, 40);
        assert_eq!(areas.tree.height, 8);
    }

    #[test]
    fn status_line_shows_the_active_target_and_cancel_hint() {
        let mut model = loaded_model((100, 30));
        model.run.active = Some(crate::app::model::ActiveRun {
            id: crate::runner::RunId(1),
            target: "simple-get.bru".into(),
            recursive: false,
            handle: None,
        });
        let line = status_line(&model);
        assert!(line.contains("simple-get.bru"), "{line}");
        assert!(line.contains("Ctrl+X"), "{line}");
    }

    #[test]
    fn status_line_shows_the_last_failure_message() {
        let mut model = loaded_model((100, 30));
        model.run.last_failure = Some(RunFailure::Error {
            target: "simple-get.bru".into(),
            error: crate::runner::RunError::BruNotFound,
        });
        let line = status_line(&model);
        assert!(line.contains("simple-get.bru"), "{line}");
        assert!(line.contains("introuvable"), "{line}");
    }

    #[test]
    fn status_line_shows_the_usual_hint_otherwise() {
        let model = loaded_model((100, 30));
        assert!(model.run.active.is_none());
        assert!(model.run.last_failure.is_none());
        let line = status_line(&model);
        assert!(line.contains("q quitter"), "{line}");
        assert!(line.contains("r lancer"), "{line}");
    }

    #[test]
    fn search_input_line_replaces_the_status_bar_while_editing() {
        let mut model = loaded_model((100, 30));
        update(&mut model, Message::StartSearch);
        update(&mut model, Message::SearchInput('p'));
        update(&mut model, Message::SearchInput('o'));
        let line = status_line(&model);
        assert_eq!(line, "/po");

        update(&mut model, Message::ConfirmSearch);
        let line = status_line(&model);
        assert!(!line.starts_with('/'), "{line}");
    }

    #[test]
    fn filter_input_line_shows_draft_in_status_bar_before_confirmation() {
        use crate::app::test_support::runner_probe_model;

        let mut model = runner_probe_model();
        select(&mut model, "json.bru");

        // Avant ouverture : status_line ordinaire
        let line_before = status_line(&model);
        assert!(!line_before.starts_with('|'), "{line_before}");

        // Ouvre le filtre et tape `.a`
        update(&mut model, Message::OpenFilter);
        update(&mut model, Message::FilterInput('.'));
        update(&mut model, Message::FilterInput('a'));
        assert!(model.filter.as_ref().unwrap().editing);

        // La barre d'état logique affiche le draft préfixé par |
        let line = status_line(&model);
        assert_eq!(line, "|.a");

        // Rendu à l'écran via TestBackend
        let screen = render(&model, 100, 30);
        let status_row = &screen[29]; // ligne 29 = barre d'état sur hauteur 30
        assert!(
            status_row.contains("|.a"),
            "la barre d'état à l'écran doit contenir |.a mais vaut :\n{status_row}"
        );

        // Après validation : le texte tapé disparaît de la barre d'état
        update(&mut model, Message::ConfirmFilter);
        assert!(!model.filter.as_ref().unwrap().editing);
        let line_after = status_line(&model);
        assert!(!line_after.starts_with('|'), "{line_after}");
        let screen_after = render(&model, 100, 30);
        assert!(!screen_after[29].contains("|.a"), "{}", screen_after[29]);
    }

    #[test]
    fn last_status_is_shown_when_no_run_is_active() {
        let mut model = loaded_model((100, 30));
        model.last_status = Some(crate::app::model::StatusMessage::Copied);
        let line = status_line(&model);
        assert!(line.contains("Copié"), "{line}");
    }

    #[test]
    fn selection_and_match_are_highlighted_distinctly_on_screen() {
        let mut model = loaded_model((100, 12));
        select(&mut model, "scripted.bru");
        update(&mut model, Message::NextFocus);
        update(&mut model, Message::StartSearch);
        for c in "res.body.ok".chars() {
            update(&mut model, Message::SearchInput(c));
        }
        update(&mut model, Message::ConfirmSearch);

        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(100, 12)).expect("terminal");
        terminal.draw(|frame| view(&model, frame)).expect("rendu");
        let buffer = terminal.backend().buffer();
        let (line, range) = model.detail_match.clone().expect("correspondance");
        let detail_area = layout_for((100, 12)).expect("taille suffisante").detail;
        let inner_area = inner(detail_area);
        let row = inner_area.y + (line - model.detail_scroll);
        let col = inner_area.x + range.start as u16;
        let highlighted = buffer[(col, row)].bg;
        assert_ne!(
            highlighted,
            ratatui::style::Color::Reset,
            "la correspondance doit avoir un fond distinct"
        );

        // Sélection visuelle : fond distinct de la surbrillance de motif.
        update(&mut model, Message::ToggleVisual);
        update(&mut model, Message::Down);
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(100, 12)).expect("terminal");
        terminal.draw(|frame| view(&model, frame)).expect("rendu");
        let buffer = terminal.backend().buffer();
        let selection_row = inner_area.y;
        let selection_bg = buffer[(inner_area.x, selection_row)].bg;
        assert_ne!(
            selection_bg,
            ratatui::style::Color::Reset,
            "sélection visible"
        );
    }
}
