//! Transitions du modèle.
//!
//! `update` est la seule fonction qui modifie le modèle. Elle ne fait
//! aucune I/O : le chargement et le rendu sont pilotés par la boucle.

use ratatui::widgets::{Paragraph, Wrap};

use super::message::Message;
use super::model::{CollectionState, Exit, Focus, Model, visible_rows};
use super::view::{detail::detail_text, inner, layout_for};
use crate::collection::TreeNode;

/// Applique un message au modèle.
pub fn update(model: &mut Model, message: Message) {
    match message {
        Message::Quit | Message::ForceQuit => model.exit = Some(Exit::Normal),
        Message::TerminalClosed(error) => model.exit = Some(Exit::TerminalError(error)),
        Message::Resize { width, height } => {
            model.size = (width, height);
            scroll_tree_into_view(model);
            model.detail_scroll = model.detail_scroll.min(detail_max_scroll(model));
        }
        Message::CollectionLoaded(result) => {
            model.collection = match result {
                Ok(collection) => CollectionState::Loaded(collection),
                Err(error) => CollectionState::Failed(error),
            };
            model.tree.expanded.clear();
            refresh_rows(model);
            model.tree.selected = 0;
            model.tree.offset = 0;
            model.detail_scroll = 0;
        }
        Message::NextFocus => {
            model.focus = match model.focus {
                Focus::Tree => Focus::Detail,
                Focus::Detail => Focus::Tree,
            };
        }
        Message::FocusTree => model.focus = Focus::Tree,
        navigation => match model.focus {
            Focus::Tree => navigate_tree(model, navigation),
            Focus::Detail => scroll_detail(model, navigation),
        },
    }
}

/// Recalcule les lignes visibles de l'arbre.
pub(crate) fn refresh_rows(model: &mut Model) {
    let rows = match &model.collection {
        CollectionState::Loaded(collection) => visible_rows(&collection.tree, &model.tree.expanded),
        _ => Vec::new(),
    };
    model.tree.rows = rows;
    let last = model.tree.rows.len().saturating_sub(1);
    model.tree.selected = model.tree.selected.min(last);
}

fn navigate_tree(model: &mut Model, message: Message) {
    let len = model.tree.rows.len();
    if len == 0 {
        return;
    }
    let before = model.tree.selected;
    match message {
        Message::Up => model.tree.selected = before.saturating_sub(1),
        Message::Down => model.tree.selected = (before + 1).min(len - 1),
        Message::Home => model.tree.selected = 0,
        Message::End => model.tree.selected = len - 1,
        Message::Right => expand_or_enter(model),
        Message::Left => collapse_or_parent(model),
        _ => {}
    }
    if model.tree.selected != before {
        model.detail_scroll = 0;
    }
    scroll_tree_into_view(model);
}

/// Dossier sélectionné : (chemin, déplié, vide).
fn selected_folder(model: &Model) -> Option<(std::path::PathBuf, bool, bool)> {
    match model.selected_node()? {
        TreeNode::Folder(folder) => Some((
            folder.path.clone(),
            model.tree.expanded.contains(&folder.path),
            folder.children.is_empty(),
        )),
        _ => None,
    }
}

fn expand_or_enter(model: &mut Model) {
    match selected_folder(model) {
        Some((path, false, _)) => {
            model.tree.expanded.insert(path);
            refresh_rows(model);
        }
        Some((_, true, false)) => model.tree.selected += 1,
        _ => {}
    }
}

fn collapse_or_parent(model: &mut Model) {
    if let Some((path, true, _)) = selected_folder(model) {
        // Les lignes précédant le dossier ne changent pas : son indice reste
        // celui de la sélection.
        model.tree.expanded.remove(&path);
        refresh_rows(model);
        return;
    }
    let selected = model.tree.selected;
    let Some(row) = model.tree.rows.get(selected) else {
        return;
    };
    let Some((_, parent)) = row.address.split_last() else {
        return;
    };
    if parent.is_empty() {
        return;
    }
    if let Some(index) = model.tree.rows[..selected]
        .iter()
        .rposition(|candidate| candidate.address == parent)
    {
        model.tree.selected = index;
    }
}

/// Hauteur utile du panneau de l'arbre.
fn tree_height(model: &Model) -> usize {
    layout_for(model.size).map_or(1, |areas| usize::from(inner(areas.tree).height).max(1))
}

/// Ajuste `offset` pour que la sélection reste visible sans laisser de
/// vide en bas de panneau.
pub(crate) fn scroll_tree_into_view(model: &mut Model) {
    let height = tree_height(model);
    let tree = &mut model.tree;
    if tree.selected < tree.offset {
        tree.offset = tree.selected;
    } else if tree.selected >= tree.offset + height {
        tree.offset = tree.selected + 1 - height;
    }
    tree.offset = tree.offset.min(tree.rows.len().saturating_sub(height));
}

/// Défilement maximal du détail : lignes après retour à la ligne moins la
/// hauteur du panneau.
fn detail_max_scroll(model: &Model) -> u16 {
    let Some(areas) = layout_for(model.size) else {
        return 0;
    };
    let area = inner(areas.detail);
    let lines = Paragraph::new(detail_text(model))
        .wrap(Wrap { trim: false })
        .line_count(area.width);
    let max = lines.saturating_sub(usize::from(area.height));
    u16::try_from(max).unwrap_or(u16::MAX)
}

fn scroll_detail(model: &mut Model, message: Message) {
    let page = layout_for(model.size).map_or(1, |areas| inner(areas.detail).height.max(1));
    let max = detail_max_scroll(model);
    let scroll = model.detail_scroll;
    model.detail_scroll = match message {
        Message::Up => scroll.saturating_sub(1),
        Message::Down => scroll.saturating_add(1).min(max),
        Message::PageUp => scroll.saturating_sub(page),
        Message::PageDown => scroll.saturating_add(page).min(max),
        Message::Home => 0,
        Message::End => max,
        _ => scroll,
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::test_support::{loaded_model, select, selected_name};
    use crate::collection::LoadError;

    #[test]
    fn right_twice_enters_the_folder() {
        let mut model = loaded_model((100, 30));
        assert_eq!(selected_name(&model), "Groupe");
        update(&mut model, Message::Right);
        assert_eq!(selected_name(&model), "Groupe");
        assert!(model.tree.expanded.contains(std::path::Path::new("grp")));
        update(&mut model, Message::Right);
        assert_eq!(selected_name(&model), "x");
    }

    #[test]
    fn left_goes_to_parent_then_collapses() {
        let mut model = loaded_model((100, 30));
        update(&mut model, Message::Right);
        update(&mut model, Message::Right);
        update(&mut model, Message::Left);
        assert_eq!(selected_name(&model), "Groupe");
        assert!(model.tree.expanded.contains(std::path::Path::new("grp")));
        update(&mut model, Message::Left);
        assert_eq!(selected_name(&model), "Groupe");
        assert!(model.tree.expanded.is_empty());
        assert_eq!(model.tree.rows.len(), 11);
        // Au premier niveau, `Left` sur une requête est sans effet.
        update(&mut model, Message::Down);
        update(&mut model, Message::Left);
        assert_eq!(selected_name(&model), "ping");
    }

    #[test]
    fn collapsing_from_deep_keeps_selection_on_folder() {
        let mut model = loaded_model((100, 30));
        select(&mut model, "grp/sub/deep.bru");
        update(&mut model, Message::Left);
        assert_eq!(selected_name(&model), "Sous-groupe");
        update(&mut model, Message::Left);
        assert_eq!(selected_name(&model), "Sous-groupe");
        assert_eq!(model.tree.rows[model.tree.selected].address, [0, 1]);
    }

    #[test]
    fn extremities_and_leaves() {
        let mut model = loaded_model((100, 30));
        update(&mut model, Message::Up);
        assert_eq!(model.tree.selected, 0);
        update(&mut model, Message::End);
        assert_eq!(selected_name(&model), "unknown-block");
        update(&mut model, Message::Down);
        assert_eq!(selected_name(&model), "unknown-block");
        update(&mut model, Message::Right);
        assert_eq!(selected_name(&model), "unknown-block");
        assert_eq!(model.tree.rows.len(), 11);
        update(&mut model, Message::Home);
        assert_eq!(selected_name(&model), "Groupe");
    }

    #[test]
    fn selection_stays_visible_on_a_short_terminal() {
        // 12 lignes : titre, barre d'état et bordures laissent 8 lignes.
        let mut model = loaded_model((100, 12));
        update(&mut model, Message::End);
        assert_eq!(model.tree.selected, 10);
        assert_eq!(model.tree.offset, 3);
        update(&mut model, Message::Up);
        assert_eq!(model.tree.offset, 3);
        update(&mut model, Message::Home);
        assert_eq!(model.tree.offset, 0);

        update(&mut model, Message::End);
        update(
            &mut model,
            Message::Resize {
                width: 100,
                height: 10,
            },
        );
        let height = 6;
        assert!(model.tree.selected >= model.tree.offset);
        assert!(model.tree.selected < model.tree.offset + height);
        update(
            &mut model,
            Message::Resize {
                width: 100,
                height: 40,
            },
        );
        assert_eq!(model.tree.offset, 0);
    }

    #[test]
    fn detail_scroll_is_bounded_and_reset() {
        let mut model = loaded_model((100, 12));
        select(&mut model, "scripted.bru");
        update(&mut model, Message::NextFocus);
        assert_eq!(model.focus, Focus::Detail);
        update(&mut model, Message::End);
        let max = model.detail_scroll;
        assert!(max > 0, "le détail de scripted doit dépasser 8 lignes");
        assert_eq!(max, detail_max_scroll(&model));
        update(&mut model, Message::Down);
        assert_eq!(model.detail_scroll, max);
        update(&mut model, Message::PageUp);
        assert_eq!(model.detail_scroll, max.saturating_sub(8));
        update(&mut model, Message::Home);
        assert_eq!(model.detail_scroll, 0);
        update(&mut model, Message::PageDown);
        assert_eq!(model.detail_scroll, 8.min(max));

        let before = model.tree.selected;
        update(&mut model, Message::FocusTree);
        update(&mut model, Message::Down);
        assert_eq!(model.tree.selected, before + 1);
        assert_eq!(model.detail_scroll, 0);
    }

    #[test]
    fn last_detail_line_is_reachable() {
        let mut model = loaded_model((100, 12));
        select(&mut model, "scripted.bru");
        update(&mut model, Message::NextFocus);
        update(&mut model, Message::End);
        let lines = crate::app::test_support::render(&model, 100, 12);
        let screen = lines.join("\n");
        assert!(screen.contains("res.body.ok: isTrue"), "{screen}");
    }

    #[test]
    fn loading_states_and_quit() {
        let mut model = Model::new("/x".into(), (100, 30));
        update(&mut model, Message::Down);
        update(&mut model, Message::Right);
        assert!(model.exit.is_none());
        update(&mut model, Message::Quit);
        assert!(matches!(model.exit, Some(Exit::Normal)));

        let mut model = Model::new("/x".into(), (100, 30));
        update(
            &mut model,
            Message::CollectionLoaded(Err(LoadError::NotACollection { path: "/x".into() })),
        );
        assert!(matches!(model.collection, CollectionState::Failed(_)));
        assert!(model.tree.rows.is_empty());
        update(&mut model, Message::End);
        update(&mut model, Message::ForceQuit);
        assert!(matches!(model.exit, Some(Exit::Normal)));

        let mut model = Model::new("/x".into(), (100, 30));
        update(
            &mut model,
            Message::TerminalClosed(std::io::Error::other("tty perdu")),
        );
        assert!(matches!(model.exit, Some(Exit::TerminalError(_))));
    }
}
