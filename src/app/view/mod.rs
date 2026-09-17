//! Rendu de l'interface.
//!
//! `view` est pur : il lit le modèle et dessine, sans I/O ni modification
//! d'état. `layout` est partagé avec `update`, qui en déduit la hauteur des
//! panneaux pour garder la sélection visible et borner le défilement.

pub mod detail;
pub mod panels;
pub mod theme;
pub mod tree;

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, List, ListItem, ListState, Paragraph, Wrap};

use super::model::{
    CollectionState, EditMode, Focus, Model, PendingConfirm, RunFailure, StatusMessage,
};
use crate::collection::TreeNode;

/// Largeur minimale du terminal : exactement la somme des planchers de
/// l'arbre, du détail et de la réponse (`MIN_TREE_WIDTH +
/// MIN_DETAIL_WIDTH + MIN_RESPONSE_WIDTH`), pour qu'à cette taille chaque
/// panneau soit exactement à son plancher, sans marge (`design.md`, D4).
pub const MIN_WIDTH: u16 = 60;
/// Hauteur minimale du terminal.
pub const MIN_HEIGHT: u16 = 10;
/// Largeur minimale du panneau de l'arbre.
const MIN_TREE_WIDTH: u16 = 24;
/// Largeur minimale du panneau de détail.
const MIN_DETAIL_WIDTH: u16 = 18;
/// Largeur minimale du panneau de réponse.
const MIN_RESPONSE_WIDTH: u16 = 18;

const TOO_SMALL: &str = "Terminal trop petit : agrandir à 60×10 au moins.";

/// Zones de l'écran.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Areas {
    pub title: Rect,
    /// Arbre, détail et réponse réunis.
    pub body: Rect,
    pub tree: Rect,
    pub detail: Rect,
    pub response: Rect,
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
    let remaining = area.width - tree_width;
    let detail_width = (remaining * 50 / 100).max(MIN_DETAIL_WIDTH);
    let response_width = (remaining - detail_width).max(MIN_RESPONSE_WIDTH);
    Some(Areas {
        title: Rect::new(area.x, area.y, area.width, 1),
        body,
        tree: Rect::new(body.x, body.y, tree_width, body_height),
        detail: Rect::new(body.x + tree_width, body.y, detail_width, body_height),
        response: Rect::new(
            body.x + tree_width + detail_width,
            body.y,
            response_width,
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
    frame.render_widget(Block::default().style(theme::BACKGROUND), frame.area());

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
            frame.render_widget(panel(" Réponse ", false), areas.response);
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
        CollectionState::Loaded(_) => match model.focus {
            Focus::Diagnostics => {
                panels::render_diagnostics(model, frame, areas.body);
            }
            Focus::History => {
                panels::render_history(model, frame, areas.body);
            }
            Focus::EnvironmentPicker => {
                panels::render_environment_picker(model, frame, areas.body);
            }
            Focus::Secrets => {
                panels::render_secrets(model, frame, areas.body);
            }
            Focus::Tree | Focus::Detail | Focus::Response => {
                render_tree(model, frame, areas.tree);
                frame.render_widget(
                    Paragraph::new(detail::render_text(model))
                        .wrap(Wrap { trim: false })
                        .scroll((model.detail_scroll, 0))
                        .block(panel(" Détail ", model.focus == Focus::Detail)),
                    areas.detail,
                );
                render_insert_cursor(model, frame, areas.detail);
                render_response(model, frame, areas.response);
            }
        },
    }

    frame.render_widget(
        Paragraph::new(status_line(model)).style(Style::new().add_modifier(Modifier::DIM)),
        areas.status,
    );
}

fn render_insert_cursor(model: &Model, frame: &mut Frame, detail_area: Rect) {
    let Some(session) = &model.editing else {
        return;
    };
    if !matches!(session.mode, EditMode::Insert { .. }) {
        return;
    }
    let Some(TreeNode::Request(req)) = model.selected_node() else {
        return;
    };
    if req.path != session.path {
        return;
    }
    let Some((line_index, col_index)) = detail::cursor_position_in_detail(req, session) else {
        return;
    };
    let inner_area = inner(detail_area);
    if inner_area.width == 0 || inner_area.height == 0 {
        return;
    }
    let scroll = model.detail_scroll as usize;
    if line_index < scroll {
        return;
    }
    let rel_y = line_index - scroll;
    if rel_y >= inner_area.height as usize {
        return;
    }
    let cursor_y = inner_area.y + rel_y as u16;
    let cursor_x = inner_area.x + (col_index as u16).min(inner_area.width.saturating_sub(1));
    frame.set_cursor_position((cursor_x, cursor_y));
}

fn title_line(model: &Model) -> Line<'static> {
    match &model.collection {
        CollectionState::Loading => Line::from(vec![
            Span::styled("bruno-tui", Style::new().add_modifier(Modifier::BOLD)),
            Span::raw(" · "),
            Span::raw(format!("Chargement de {}…", model.source.display())),
        ]),
        CollectionState::Failed(_) => Line::from(vec![
            Span::styled("bruno-tui", Style::new().add_modifier(Modifier::BOLD)),
            Span::raw(" · "),
            Span::raw("Erreur de chargement".to_owned()),
        ]),
        CollectionState::Loaded(collection) => {
            let env_label = model
                .current_environment
                .as_deref()
                .unwrap_or("Aucun environnement");
            let mut spans = vec![
                Span::styled("bruno-tui", Style::new().add_modifier(Modifier::BOLD)),
                Span::raw(" · "),
                Span::raw(collection.name.clone()),
                Span::raw(" · "),
                Span::styled(
                    env_label.to_owned(),
                    Style::new().add_modifier(Modifier::DIM),
                ),
            ];
            let error_count = crate::app::diagnostics::diagnostics(collection).len();
            if error_count > 0 {
                spans.push(Span::raw("  "));
                spans.push(Span::styled(
                    format!("⚠ {error_count}"),
                    Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD),
                ));
            }
            Line::from(spans)
        }
    }
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
    let target = active.target.display();
    if active.recursive {
        Some(format!("Exécution récursive de {target}… (Ctrl+X annuler)"))
    } else {
        Some(format!("Exécution de {target}… (Ctrl+X annuler)"))
    }
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
        StatusMessage::SaveError(reason) => format!("Échec de sauvegarde : {reason}"),
    }
}

fn status_line(model: &Model) -> String {
    if let Some(confirm) = model.confirm {
        return match confirm {
            PendingConfirm::DiscardEdit => "Abandonner les modifications ? (y/n)".to_owned(),
            PendingConfirm::QuitWithUnsavedEdit => {
                "Quitter sans sauvegarder les modifications ? (y/n)".to_owned()
            }
        };
    }
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
    if let Some(session) = &model.editing {
        let mode_str = match &session.mode {
            EditMode::Normal => "-- NORMAL --",
            EditMode::Insert { .. } => "-- INSERT --",
        };
        let field_name = match (session.fields.get(session.cursor), model.selected_node()) {
            (Some(field), Some(TreeNode::Request(req))) => field.display_name(&req.view),
            _ => String::new(),
        };
        return if field_name.is_empty() {
            mode_str.to_owned()
        } else {
            format!("{mode_str}  {field_name}")
        };
    }
    match (&model.collection, model.focus) {
        (CollectionState::Loaded(_), Focus::Tree) => {
            "↑↓ naviguer  → déplier  ← replier  r lancer  / chercher  Tab détail  S secrets  q quitter"
                .to_owned()
        }
        (CollectionState::Loaded(_), Focus::Detail) => {
            "↑↓ défiler  Début/Fin  / chercher  n/N suivant  v sélection  y copier  Échap arbre  q quitter"
                .to_owned()
        }
        (CollectionState::Loaded(_), Focus::Diagnostics) => {
            "↑↓ naviguer  → aller au nœud  Échap arbre".to_owned()
        }
        (CollectionState::Loaded(_), Focus::History) => {
            "↑↓ naviguer  r rejouer  Échap arbre".to_owned()
        }
        (CollectionState::Loaded(_), Focus::EnvironmentPicker) => {
            "↑↓ naviguer  Entrée choisir  Échap annuler".to_owned()
        }
        (CollectionState::Loaded(_), Focus::Secrets) => secrets_hint(model).to_owned(),
        _ => "q quitter".to_owned(),
    }
}

/// Rappel de touches du panneau des variables secrètes.
fn secrets_hint(model: &Model) -> &'static str {
    let secrets = &model.secrets;
    if secrets.input.is_some() {
        "Entrée valider  Échap annuler"
    } else if secrets.pending_run.is_some() {
        "↑↓ naviguer  Entrée saisir  a ajouter  d oublier  r lancer  Échap abandonner"
    } else {
        "↑↓ naviguer  Entrée saisir  a ajouter  d oublier  Échap fermer"
    }
}

pub(crate) fn panel(title: &'static str, focused: bool) -> Block<'static> {
    let style = if focused { theme::FOCUS } else { theme::BORDER };
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

/// Dessine le panneau Réponse : le résultat de la dernière exécution de la
/// requête sélectionnée, ou un message unique délibérément centré quand
/// il n'y en a aucun (`visual-theme`, `split-request-response-panels`).
fn render_response(model: &Model, frame: &mut Frame, area: Rect) {
    let block = panel(" Réponse ", model.focus == Focus::Response);
    let text = detail::render_response_text(model);
    if text.lines.is_empty() {
        frame.render_widget(
            panels::empty_state_message(area, "aucun résultat").block(block),
            area,
        );
        return;
    }
    frame.render_widget(
        Paragraph::new(text)
            .wrap(Wrap { trim: false })
            .scroll((model.response_scroll, 0))
            .block(block),
        area,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::message::Message;
    use crate::app::model::{HistoryEntry, HistoryOutcome};
    use crate::app::test_support::{fixture, loaded_model, render, select};
    use crate::app::update::update;
    use crate::collection::{Collection, LoadError};
    use std::path::PathBuf;

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
        assert_eq!(layout_for((59, 30)), None);
        assert_eq!(layout_for((100, 9)), None);
        let areas = layout_for((60, 10)).expect("taille minimale");
        assert_eq!(areas.tree.width, MIN_TREE_WIDTH);
        assert_eq!(areas.detail.width, MIN_DETAIL_WIDTH);
        assert_eq!(areas.response.width, MIN_RESPONSE_WIDTH);
        assert_eq!(
            areas.tree.width + areas.detail.width + areas.response.width,
            60
        );
        assert_eq!(areas.tree.height, 8);
    }

    /// À la taille minimale, les trois panneaux sont affichés ; juste
    /// en dessous, le message « trop petit » remplace tout le reste
    /// (`tui-shell`, exigence « Disposition et barre d'état »).
    #[test]
    fn too_small_just_below_the_new_minimum_width() {
        let model = loaded_model((59, 30));
        let screen = crate::app::test_support::render(&model, 59, 30).join("\n");
        assert!(screen.contains("trop petit"), "{screen}");

        let model = loaded_model((60, 30));
        let screen = crate::app::test_support::render(&model, 60, 30).join("\n");
        assert!(!screen.contains("trop petit"), "{screen}");
        assert!(screen.contains("Réponse"), "{screen}");
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

    #[test]
    fn field_under_cursor_is_distinct_and_shows_pending_edit() {
        let mut model = loaded_model((100, 30));
        select(&mut model, "simple-get.bru");
        update(&mut model, Message::NextFocus);
        update(&mut model, Message::StartEdit);

        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(100, 30)).expect("terminal");
        terminal.draw(|frame| view(&model, frame)).expect("rendu");

        let detail_area = layout_for((100, 30)).expect("layout").detail;
        let inner_area = inner(detail_area);
        let buffer = terminal.backend().buffer();

        // Ligne 3 du détail = ligne URL (inner_area.y + 3)
        let url_row = inner_area.y + 3;
        let url_cell = &buffer[(inner_area.x, url_row)];
        assert!(
            url_cell.modifier.contains(Modifier::REVERSED),
            "le champ sous le curseur doit être visuellement distinct (REVERSED)"
        );

        // Édition de l'URL
        update(&mut model, Message::EnterInsert);
        update(&mut model, Message::InsertChar('!'));
        update(&mut model, Message::LeaveInsert);

        terminal.draw(|frame| view(&model, frame)).expect("rendu");
        let buffer = terminal.backend().buffer();
        let row_text: String = (inner_area.x..inner_area.x + inner_area.width)
            .map(|x| buffer[(x, url_row)].symbol())
            .collect();
        assert!(
            row_text.contains("https://{{host}}/ping!"),
            "la valeur affichée reflète l'édition en attente : {row_text}"
        );
    }

    #[test]
    fn status_line_and_confirm_states() {
        let mut model = loaded_model((100, 30));

        // 1. Aucune session : contenu habituel
        let line = status_line(&model);
        assert!(line.contains("q quitter"));
        assert!(!line.contains("-- NORMAL --"));
        assert!(!line.contains("-- INSERT --"));

        // 2. Session ouverte en mode Normal
        select(&mut model, "simple-get.bru");
        update(&mut model, Message::NextFocus);
        update(&mut model, Message::StartEdit);
        let line = status_line(&model);
        assert!(line.contains("-- NORMAL --"));
        assert!(line.contains("Url"));

        // 3. Session en mode Insert
        update(&mut model, Message::EnterInsert);
        let line = status_line(&model);
        assert!(line.contains("-- INSERT --"));
        assert!(line.contains("Url"));

        // 4. Confirmation active (prioritaire)
        update(&mut model, Message::LeaveInsert);
        model.confirm = Some(PendingConfirm::DiscardEdit);
        let line = status_line(&model);
        assert!(line.contains("Abandonner les modifications ?"), "{line}");
        assert!(!line.contains("-- NORMAL --"));

        model.confirm = Some(PendingConfirm::QuitWithUnsavedEdit);
        let line = status_line(&model);
        assert!(line.contains("Quitter sans sauvegarder"), "{line}");
    }

    #[test]
    fn insert_cursor_positioning_and_bounds() {
        let mut model = loaded_model((100, 30));
        select(&mut model, "simple-get.bru");
        update(&mut model, Message::NextFocus);
        update(&mut model, Message::StartEdit);

        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(100, 30)).expect("terminal");

        // En mode Normal : pas de curseur positionné
        terminal.draw(|frame| view(&model, frame)).expect("rendu");

        // En mode Insert : curseur positionné
        update(&mut model, Message::EnterInsert);
        terminal.draw(|frame| view(&model, frame)).expect("rendu");
        let cursor = terminal.get_cursor_position().expect("cursor position");

        let detail_area = layout_for((100, 30)).expect("layout").detail;
        let inner_area = inner(detail_area);
        assert_eq!(cursor.y, inner_area.y + 3);
        assert!(cursor.x >= inner_area.x);
        assert!(cursor.x < inner_area.x + inner_area.width);

        // Défilement lointain : curseur hors vue, ne panique pas
        model.detail_scroll = 500;
        terminal
            .draw(|frame| view(&model, frame))
            .expect("rendu sans panique");
    }

    #[test]
    fn title_line_badge_rendered_on_errors_and_hidden_otherwise() {
        // parser-cases a 3 erreurs
        let model = loaded_model((100, 30));
        let lines = render(&model, 100, 30);
        assert!(
            lines[0].contains("⚠ 3"),
            "Titre attendu avec ⚠ 3: {}",
            lines[0]
        );

        // Collection sans erreur
        let mut clean_model = Model::new(fixture(), (100, 30));
        update(
            &mut clean_model,
            Message::CollectionLoaded(Ok(Collection {
                root: "/clean".into(),
                name: "clean".into(),
                settings: None,
                tree: Vec::new(),
                environments: Vec::new(),
            })),
        );
        let clean_lines = render(&clean_model, 100, 30);
        assert!(
            !clean_lines[0].contains('⚠'),
            "Titre ne doit pas contenir ⚠: {}",
            clean_lines[0]
        );
    }

    #[test]
    fn response_panel_shows_result_separately_from_detail() {
        use crate::app::test_support::runner_probe_model;

        // 1. Requête exécutée : le résultat est dans la réponse, pas
        // dans le détail.
        let mut model = runner_probe_model();
        select(&mut model, "green.bru");
        let lines = render(&model, 100, 30);
        let screen = lines.join("\n");
        assert!(screen.contains("Réponse"), "{screen}");
        assert!(screen.contains("Résultat"), "{screen}");
        assert!(screen.contains("Verdict"), "{screen}");
        let detail_col_end = layout_for((100, 30)).expect("layout").detail.right();
        let detail_only: String = lines
            .iter()
            .map(|line| {
                line.chars()
                    .take(detail_col_end as usize)
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            !detail_only.contains("Résultat"),
            "le détail ne doit plus afficher le résultat :\n{detail_only}"
        );

        // 2. Requête sans exécution : message unique dans la réponse.
        select(&mut model, "skip.bru");
        model.run.outcomes.remove(std::path::Path::new("skip.bru"));
        let screen = render(&model, 100, 30).join("\n");
        assert!(screen.contains("aucun résultat"), "{screen}");

        // 3. Dossier sélectionné : même message.
        let mut folder_model = loaded_model((100, 30));
        select(&mut folder_model, "grp");
        let screen = render(&folder_model, 100, 30).join("\n");
        assert!(screen.contains("aucun résultat"), "{screen}");

        // 4. Nœud en erreur sélectionné : même message.
        let mut error_model = loaded_model((100, 30));
        select(&mut error_model, "broken.bru");
        let screen = render(&error_model, 100, 30).join("\n");
        assert!(screen.contains("aucun résultat"), "{screen}");
    }

    #[test]
    fn response_status_band_stays_visible_across_tabs() {
        use crate::app::test_support::runner_probe_model;

        let mut model = runner_probe_model();
        select(&mut model, "green.bru");
        update(&mut model, Message::NextFocus); // Détail
        update(&mut model, Message::NextFocus); // Réponse

        for tab in [
            crate::app::model::ResponseTab::Body,
            crate::app::model::ResponseTab::Headers,
            crate::app::model::ResponseTab::Tests,
        ] {
            model.response_tab = tab;
            let screen = render(&model, 100, 30).join("\n");
            assert!(screen.contains("Résultat"), "{tab:?} :\n{screen}");
            assert!(screen.contains("Verdict"), "{tab:?} :\n{screen}");
            assert!(screen.contains("Statut : 200"), "{tab:?} :\n{screen}");
        }
    }

    #[test]
    fn response_tab_content_is_shown_only_when_active() {
        use crate::app::test_support::runner_probe_model;

        let mut model = runner_probe_model();
        select(&mut model, "green.bru");
        update(&mut model, Message::NextFocus); // Détail
        update(&mut model, Message::NextFocus); // Réponse

        model.response_tab = crate::app::model::ResponseTab::Body;
        let body_screen = render(&model, 100, 30).join("\n");
        assert!(body_screen.contains("\"a\": ["), "{body_screen}");
        assert!(!body_screen.contains("content-type"), "{body_screen}");
        assert!(!body_screen.contains("body a"), "{body_screen}");

        model.response_tab = crate::app::model::ResponseTab::Headers;
        let headers_screen = render(&model, 100, 30).join("\n");
        assert!(headers_screen.contains("content-type"), "{headers_screen}");
        assert!(!headers_screen.contains("\"a\": ["), "{headers_screen}");
        assert!(!headers_screen.contains("body a"), "{headers_screen}");

        model.response_tab = crate::app::model::ResponseTab::Tests;
        let tests_screen = render(&model, 100, 30).join("\n");
        assert!(tests_screen.contains("body a"), "{tests_screen}");
        assert!(tests_screen.contains("res.status"), "{tests_screen}");
        assert!(!tests_screen.contains("\"a\": ["), "{tests_screen}");
        assert!(!tests_screen.contains("content-type"), "{tests_screen}");
    }

    #[test]
    fn render_diagnostics_and_history_panels_with_and_without_entries() {
        // 1. Diagnostics avec entrées (sur parser-cases)
        let mut model = loaded_model((100, 30));
        model.focus = Focus::Diagnostics;
        let screen = render(&model, 100, 30).join("\n");
        assert!(screen.contains("Diagnostics"), "{screen}");
        assert!(screen.contains("badmeta"), "{screen}");
        assert!(screen.contains("broken.bru"), "{screen}");
        assert!(screen.contains("no-method.bru"), "{screen}");

        // 2. Diagnostics sans erreur
        let mut clean_model = Model::new(fixture(), (100, 30));
        update(
            &mut clean_model,
            Message::CollectionLoaded(Ok(Collection {
                root: "/clean".into(),
                name: "clean".into(),
                settings: None,
                tree: Vec::new(),
                environments: Vec::new(),
            })),
        );
        clean_model.focus = Focus::Diagnostics;
        let screen = render(&clean_model, 100, 30).join("\n");
        assert!(screen.contains("aucune erreur"), "{screen}");

        // 3. Historique sans entrée
        let mut hist_model = loaded_model((100, 30));
        hist_model.focus = Focus::History;
        let screen = render(&hist_model, 100, 30).join("\n");
        assert!(screen.contains("Historique"), "{screen}");
        assert!(screen.contains("aucune exécution"), "{screen}");

        // 4. Historique avec entrée
        hist_model.history.push_back(HistoryEntry {
            started_at: std::time::SystemTime::now(),
            target: PathBuf::from("ping.bru"),
            recursive: false,
            outcome: HistoryOutcome::Completed {
                total: 1,
                failed: 0,
                duration_secs: 0.25,
            },
        });
        let screen = render(&hist_model, 100, 30).join("\n");
        assert!(screen.contains("ping.bru"), "{screen}");
        assert!(screen.contains("succès"), "{screen}");
        assert!(screen.contains("0.25s"), "{screen}");
    }

    /// Le message unique d'un panneau vide n'est plus collé à la première
    /// ligne intérieure : il est présenté de façon délibérée
    /// (`visual-theme`), sans changer son texte.
    #[test]
    fn empty_diagnostics_and_history_messages_are_not_on_the_first_inner_row() {
        let mut clean_model = Model::new(fixture(), (100, 30));
        update(
            &mut clean_model,
            Message::CollectionLoaded(Ok(Collection {
                root: "/clean".into(),
                name: "clean".into(),
                settings: None,
                tree: Vec::new(),
                environments: Vec::new(),
            })),
        );
        clean_model.focus = Focus::Diagnostics;
        let lines = render(&clean_model, 100, 30);
        let row = lines
            .iter()
            .position(|l| l.contains("aucune erreur"))
            .expect("message « aucune erreur » présent");
        assert_eq!(row, 14, "message pas centré :\n{}", lines.join("\n"));

        let mut hist_model = loaded_model((100, 30));
        hist_model.focus = Focus::History;
        let lines = render(&hist_model, 100, 30);
        let row = lines
            .iter()
            .position(|l| l.contains("aucune exécution"))
            .expect("message « aucune exécution » présent");
        assert_eq!(row, 14, "message pas centré :\n{}", lines.join("\n"));
    }

    #[test]
    fn status_line_shows_expected_keys_for_diagnostics_and_history() {
        let mut model = loaded_model((100, 30));

        model.focus = Focus::Diagnostics;
        let lines = render(&model, 100, 30);
        let status = &lines[29];
        assert!(
            status.contains("↑↓ naviguer  → aller au nœud  Échap arbre"),
            "{status}"
        );

        model.focus = Focus::History;
        let lines = render(&model, 100, 30);
        let status = &lines[29];
        assert!(
            status.contains("↑↓ naviguer  r rejouer  Échap arbre"),
            "{status}"
        );

        model.focus = Focus::EnvironmentPicker;
        let lines = render(&model, 100, 30);
        let status = &lines[29];
        assert!(
            status.contains("↑↓ naviguer  Entrée choisir  Échap annuler"),
            "{status}"
        );
    }

    /// La fixture `parser-cases/environments/` porte, dans l'ordre
    /// alphabétique : `local` (valide), `malformed` (en erreur),
    /// `staging` (valide).
    #[test]
    fn environment_picker_panel_and_permanent_indicator() {
        let mut model = loaded_model((100, 30));

        // Indicateur permanent : « Aucun » par défaut, panneau fermé
        let screen = render(&model, 100, 30).join("\n");
        assert!(screen.contains("Aucun environnement"), "{screen}");
        assert!(!screen.contains("Environnement"), "{screen}");

        // Panneau ouvert : « Aucun » + les 3 entrées, celle en erreur marquée
        model.focus = Focus::EnvironmentPicker;
        let screen = render(&model, 100, 30).join("\n");
        assert!(screen.contains("Environnement"), "{screen}");
        assert!(screen.contains("Aucun"), "{screen}");
        assert!(screen.contains("local"), "{screen}");
        assert!(screen.contains("staging"), "{screen}");
        assert!(
            screen.contains("malformed.bru (invalide)"),
            "entrée en erreur non marquée : {screen}"
        );

        // Indicateur mis à jour après sélection
        model.current_environment = Some("staging".into());
        model.focus = Focus::Tree;
        let screen = render(&model, 100, 30).join("\n");
        assert!(screen.contains("staging"), "{screen}");
        assert!(!screen.contains("Aucun environnement"), "{screen}");
    }

    /// La bordure d'un panneau porte `theme::BORDER` sans focus,
    /// `theme::FOCUS` avec focus ; le titre du panneau, posé sur la même
    /// ligne de bordure, doit porter la même couleur (`visual-theme`).
    #[test]
    fn panel_border_and_title_use_border_or_focus_color() {
        let model = loaded_model((100, 30)); // focus = Tree
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(100, 30)).expect("terminal");
        terminal.draw(|frame| view(&model, frame)).expect("rendu");
        let buffer = terminal.backend().buffer();
        let areas = layout_for((100, 30)).expect("layout");
        let focus_fg = theme::FOCUS.fg.expect("fg défini");
        let border_fg = theme::BORDER.fg.expect("fg défini");

        // Coin de bordure : arbre focalisé en FOCUS, détail non focalisé
        // en BORDER.
        assert_eq!(
            buffer[(areas.tree.x, areas.tree.y)].fg,
            focus_fg,
            "bordure du panneau focalisé"
        );
        assert_eq!(
            buffer[(areas.detail.x, areas.detail.y)].fg,
            border_fg,
            "bordure du panneau non focalisé"
        );

        // Titre : porte la même couleur que la bordure du même panneau.
        assert_eq!(
            buffer[(areas.tree.x + 1, areas.tree.y)].fg,
            focus_fg,
            "titre du panneau focalisé"
        );
        assert_eq!(
            buffer[(areas.detail.x + 1, areas.detail.y)].fg,
            border_fg,
            "titre du panneau non focalisé"
        );
    }

    /// Le fond d'application (`visual-theme`) est posé une seule fois au
    /// début de `view()`, avant toute autre chose : il doit donc être
    /// visible dans tous les états, pas seulement `Loaded`.
    #[test]
    fn background_is_applied_in_every_state() {
        let bg = theme::BACKGROUND.bg.expect("fond défini");

        // État chargé : titre, panneau, barre d'état.
        let model = loaded_model((100, 30));
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(100, 30)).expect("terminal");
        terminal.draw(|frame| view(&model, frame)).expect("rendu");
        let buffer = terminal.backend().buffer();
        assert_eq!(buffer[(0, 0)].bg, bg, "ligne de titre");
        assert_eq!(buffer[(2, 5)].bg, bg, "panneau de l'arbre");
        assert_eq!(buffer[(0, 29)].bg, bg, "barre d'état");

        // État Loading.
        let loading = Model::new("/somewhere/else".into(), (100, 30));
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(100, 30)).expect("terminal");
        terminal.draw(|frame| view(&loading, frame)).expect("rendu");
        assert_eq!(terminal.backend().buffer()[(0, 0)].bg, bg, "état Loading");

        // État Failed.
        let mut failed = Model::new("/somewhere/else".into(), (100, 30));
        update(
            &mut failed,
            Message::CollectionLoaded(Err(LoadError::NotACollection {
                path: "/somewhere/else".into(),
            })),
        );
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(100, 30)).expect("terminal");
        terminal.draw(|frame| view(&failed, frame)).expect("rendu");
        assert_eq!(terminal.backend().buffer()[(0, 0)].bg, bg, "état Failed");

        // Terminal trop petit.
        let small = loaded_model((30, 8));
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(30, 8)).expect("terminal");
        terminal.draw(|frame| view(&small, frame)).expect("rendu");
        assert_eq!(
            terminal.backend().buffer()[(0, 0)].bg,
            bg,
            "terminal trop petit"
        );
    }

    #[test]
    fn secrets_panel_shows_sources_never_values() {
        use crate::app::message::MaskedChar;
        use crate::runner::SecretString;
        use crate::secrets::{Resolved, SecretMapping, SecretSource};

        let mut model = loaded_model((120, 30));
        model.current_environment = Some("local".into());
        model.secrets.mappings = vec![SecretMapping {
            name: "oktaClientSecret".into(),
            key: None,
        }];
        model.secrets.resolved = vec![Resolved {
            name: "oktaClientSecret".into(),
            source: SecretSource::DotEnv {
                key: "OKTA_CLIENT_SECRET".into(),
            },
            value: Some(SecretString::new("dotenv-s3cr3t")),
        }];
        update(&mut model, Message::ToggleSecrets);
        let screen = render(&model, 120, 30).join("\n");
        assert!(screen.contains("Variables secrètes"), "{screen}");
        assert!(
            screen.contains("oktaClientSecret  .env (OKTA_CLIENT_SECRET)"),
            "{screen}"
        );
        assert!(
            screen.contains("token  non fournie (cherchée : token, TOKEN)"),
            "{screen}"
        );
        assert!(!screen.contains("dotenv-s3cr3t"), "{screen}");
        assert!(!screen.contains('•'), "longueur révélée : {screen}");
        assert!(
            screen.contains("↑↓ naviguer  Entrée saisir  a ajouter  d oublier  Échap fermer"),
            "{screen}"
        );

        // Saisie : un `•` par caractère, jamais la valeur.
        update(&mut model, Message::Down);
        update(&mut model, Message::Right);
        for c in "typed-s3cr3t".chars() {
            update(&mut model, Message::SecretInput(MaskedChar(c)));
        }
        let screen = render(&model, 120, 30).join("\n");
        assert!(
            screen.contains("Valeur de token : ••••••••••••"),
            "{screen}"
        );
        assert!(!screen.contains("typed-s3cr3t"), "{screen}");
        assert!(screen.contains("Entrée valider  Échap annuler"), "{screen}");

        update(&mut model, Message::ConfirmSecretInput);
        let screen = render(&model, 120, 30).join("\n");
        assert!(screen.contains("token  saisie"), "{screen}");
        assert!(!screen.contains("typed-s3cr3t"), "{screen}");
        assert!(!screen.contains('•'), "longueur révélée : {screen}");
    }

    #[test]
    fn secrets_panel_empty_pending_and_errors() {
        use crate::secrets::{InvalidReason, Resolved, SecretSource};

        let mut model = loaded_model((120, 30));
        update(&mut model, Message::ToggleSecrets);
        let screen = render(&model, 120, 30).join("\n");
        assert!(screen.contains("aucune variable secrète"), "{screen}");

        // Proposition au lancement : exécution en attente signalée.
        let mut model = loaded_model((120, 30));
        model.current_environment = Some("local".into());
        select(&mut model, "simple-get.bru");
        update(&mut model, Message::RunSelected);
        let screen = render(&model, 120, 30).join("\n");
        assert!(
            screen.contains("Exécution de simple-get.bru en attente"),
            "{screen}"
        );
        assert!(screen.contains("r lancer  Échap abandonner"), "{screen}");

        model.secrets.resolved = vec![Resolved {
            name: "token".into(),
            source: SecretSource::Invalid {
                key: "TOKEN".into(),
                reason: InvalidReason::Multiline,
            },
            value: None,
        }];
        let screen = render(&model, 120, 30).join("\n");
        assert!(
            screen.contains("token  erreur (TOKEN) : valeur multiligne"),
            "{screen}"
        );

        update(&mut model, Message::AddSecret);
        update(
            &mut model,
            Message::SecretInput(crate::app::message::MaskedChar('=')),
        );
        update(&mut model, Message::ConfirmSecretInput);
        let screen = render(&model, 120, 30).join("\n");
        assert!(screen.contains("Nom : ="), "{screen}");
        assert!(screen.contains("nom invalide"), "{screen}");
    }

    #[test]
    fn tree_status_line_mentions_secrets_panel() {
        let model = loaded_model((100, 30));
        let status = &render(&model, 100, 30)[29];
        assert!(status.contains("S secrets"), "{status}");
    }
}
