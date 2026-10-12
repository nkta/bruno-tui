//! Recherche `/` : saisie, exécution et répétition dans l'arbre, le détail ou la réponse.

use crate::app::model::{Focus, Model, StatusMessage, all_rows, tree_node_at};
use crate::app::search::{SearchScope, SearchState, find_detail_match, find_tree_match};
use crate::app::tree_filter::matches_tree_filter;
use crate::app::update::{
    clear_detail_view_state, clear_tree_filter, current_tree_filter_pattern, detail_max_scroll,
    refresh_rows, reset_filter_if_selection_changed, response_max_scroll, scroll_tree_into_view,
    selection_locked,
};
use crate::app::view::detail::{plain_lines, response_plain_lines};
use crate::collection::TreeNode;

/// `/` : ouvre la saisie sur le motif déjà validé, ou vide s'il n'y en a
/// pas encore eu. `scope` n'est fixé qu'à la validation (`confirm_search`),
/// pas ici : rouvrir sans valider ne doit pas changer sur quel panneau `n`
/// rejoue une recherche précédente.
pub(super) fn start_search(model: &mut Model) {
    let search = model.search.get_or_insert_with(SearchState::default);
    search.draft = search.pattern.clone();
    search.editing = true;
}

/// `Entrée` en saisie : motif vide = annulation (comme `Échap`), sinon
/// valide le motif, fixe le panneau associé au focus courant, et lance la
/// recherche vers l'avant.
pub(super) fn confirm_search(model: &mut Model) {
    let Some(search) = &mut model.search else {
        return;
    };
    if !search.editing {
        return;
    }
    if search.draft.is_empty() {
        search.editing = false;
        return;
    }
    let scope = match model.focus {
        Focus::Tree => SearchScope::Tree,
        Focus::Detail => SearchScope::Detail,
        Focus::Response => SearchScope::Response,
        Focus::Diagnostics
        | Focus::History
        | Focus::EnvironmentPicker
        | Focus::Secrets
        | Focus::Campaign => SearchScope::Tree,
    };
    search.pattern = search.draft.clone();
    search.editing = false;
    search.scope = Some(scope);
    search.direction_forward = true;
    run_search(model, true);
}

/// `n` (`same_direction: true`) ou `N` (`false`), sans effet tant qu'aucun
/// motif n'a été validé. Utilise le panneau associé au motif, pas le focus
/// courant.
pub(super) fn repeat_search(model: &mut Model, same_direction: bool) {
    let Some(search) = &model.search else {
        return;
    };
    if search.pattern.is_empty() || search.scope.is_none() {
        return;
    }
    let forward = if same_direction {
        search.direction_forward
    } else {
        !search.direction_forward
    };
    run_search(model, forward);
}

/// Exécute la recherche dans le panneau associé au motif validé, met à
/// jour `last_result`, et signale l'absence de correspondance dans la
/// barre d'état.
fn run_search(model: &mut Model, forward: bool) {
    let Some(scope) = model.search.as_ref().and_then(|s| s.scope) else {
        return;
    };
    let pattern = model
        .search
        .as_ref()
        .map(|s| s.pattern.clone())
        .unwrap_or_default();
    if pattern.is_empty() {
        return;
    }
    let found = match scope {
        SearchScope::Tree => search_tree(model, &pattern, forward),
        SearchScope::Detail => search_detail(model, &pattern, forward),
        SearchScope::Response => search_response(model, &pattern, forward),
    };
    if let Some(search) = &mut model.search {
        search.last_result = Some(found);
    }
    if !found {
        model.last_status = Some(StatusMessage::NoMatch);
    }
}

/// Recherche dans l'arbre entier (dossiers repliés compris), déplie les
/// ancêtres du nœud trouvé et le sélectionne.
fn search_tree(model: &mut Model, pattern: &str, forward: bool) -> bool {
    let Some(collection) = model.loaded() else {
        return false;
    };
    let all = all_rows(&collection.tree);
    let current = model
        .tree
        .rows
        .get(model.tree.selected)
        .map(|row| row.address.clone());
    let from = current
        .as_ref()
        .and_then(|address| all.iter().position(|row| &row.address == address))
        .unwrap_or(0);
    let Some(found) = find_tree_match(&collection.tree, &all, from, pattern, forward) else {
        return false;
    };
    let address = all[found].address.clone();
    if selection_locked(model) && current.as_ref() != Some(&address) {
        model.last_status = Some(StatusMessage::EditLocked);
        return true;
    }
    let should_clear_filter = if let Some(p) = current_tree_filter_pattern(model) {
        model.loaded().is_some_and(|col| {
            !tree_node_at(&col.tree, &address).is_some_and(|node| matches_tree_filter(node, p))
        })
    } else {
        false
    };
    if should_clear_filter {
        clear_tree_filter(model);
    }
    let Some(collection) = model.loaded() else {
        return false;
    };
    let mut ancestors = Vec::new();
    for depth in 1..address.len() {
        if let Some(TreeNode::Folder(folder)) = tree_node_at(&collection.tree, &address[..depth]) {
            ancestors.push(folder.path.clone());
        }
    }
    for path in ancestors {
        model.tree.expanded.insert(path);
    }
    refresh_rows(model);
    let previous_path = model.selected_node().map(|node| node.path().to_path_buf());
    if let Some(index) = model
        .tree
        .rows
        .iter()
        .position(|row| row.address == address)
    {
        model.tree.selected = index;
    }
    clear_detail_view_state(model);
    reset_filter_if_selection_changed(model, previous_path.as_deref());
    scroll_tree_into_view(model);
    true
}

/// Recherche dans le texte affiché du détail, depuis le sommet du panneau
/// courant ; sur correspondance, fait défiler la ligne trouvée en haut
/// (ou à la position maximale si elle ne peut pas monter plus) et
/// enregistre la position pour la surbrillance.
fn search_detail(model: &mut Model, pattern: &str, forward: bool) -> bool {
    let lines = plain_lines(model);
    let Some((line, range)) = find_detail_match(&lines, model.detail_scroll, pattern, forward)
    else {
        return false;
    };
    model.detail_scroll = line.min(detail_max_scroll(model));
    model.detail_match = Some((line, range));
    true
}

/// Même principe que [`search_detail`], pour la réponse.
fn search_response(model: &mut Model, pattern: &str, forward: bool) -> bool {
    let lines = response_plain_lines(model);
    let Some((line, range)) = find_detail_match(&lines, model.response_scroll, pattern, forward)
    else {
        return false;
    };
    model.response_scroll = line.min(response_max_scroll(model));
    model.response_match = Some((line, range));
    true
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::app::message::Message;
    use crate::app::model::{Focus, RequestTab, StatusMessage};
    use crate::app::test_support::{loaded_model, select, selected_name};
    use crate::app::update::test_support::{
        plain_line_index_containing, response_line_index_containing,
    };
    use crate::app::update::update;

    #[test]
    fn start_search_from_tree_and_type_then_backspace() {
        let mut model = loaded_model((100, 30));
        update(&mut model, Message::StartSearch);
        assert!(model.search.as_ref().expect("état créé").editing);
        for c in ['p', 'o', 's', 't'] {
            update(&mut model, Message::SearchInput(c));
        }
        update(&mut model, Message::SearchBackspace);
        assert_eq!(model.search.as_ref().unwrap().draft, "pos");
    }

    #[test]
    fn cancel_search_keeps_selection_scroll_and_last_pattern() {
        let mut model = loaded_model((100, 30));
        update(&mut model, Message::StartSearch);
        for c in "no-".chars() {
            update(&mut model, Message::SearchInput(c));
        }
        update(&mut model, Message::ConfirmSearch);
        assert_eq!(selected_name(&model), "no-method.bru");
        let selected_before = model.tree.selected;
        let scroll_before = model.detail_scroll;

        update(&mut model, Message::StartSearch);
        for c in "griffe".chars() {
            update(&mut model, Message::SearchInput(c));
        }
        update(&mut model, Message::CancelSearch);
        assert!(!model.search.as_ref().unwrap().editing);
        assert_eq!(model.tree.selected, selected_before);
        assert_eq!(model.detail_scroll, scroll_before);
        // Le motif validé précédemment reste disponible pour n/N.
        assert_eq!(model.search.as_ref().unwrap().pattern, "no-");
    }

    #[test]
    fn confirm_empty_pattern_cancels_without_searching() {
        let mut model = loaded_model((100, 30));
        let before = model.tree.selected;
        update(&mut model, Message::StartSearch);
        update(&mut model, Message::ConfirmSearch);
        assert!(!model.search.as_ref().unwrap().editing);
        assert_eq!(model.tree.selected, before);
        assert!(model.search.as_ref().unwrap().pattern.is_empty());
    }

    #[test]
    fn tree_search_matches_inside_a_collapsed_folder() {
        let mut model = loaded_model((100, 30));
        assert!(model.tree.expanded.is_empty());
        update(&mut model, Message::StartSearch);
        for c in "inherit".chars() {
            update(&mut model, Message::SearchInput(c));
        }
        update(&mut model, Message::ConfirmSearch);
        assert_eq!(selected_name(&model), "inherit");
        assert!(model.tree.expanded.contains(Path::new("grp")));
    }

    #[test]
    fn tree_search_wraps_circularly() {
        let mut model = loaded_model((100, 30));
        update(&mut model, Message::End); // "unknown-block", dernier nœud
        assert_eq!(selected_name(&model), "unknown-block");
        update(&mut model, Message::StartSearch);
        for c in "ping".chars() {
            update(&mut model, Message::SearchInput(c));
        }
        update(&mut model, Message::ConfirmSearch);
        assert_eq!(selected_name(&model), "ping");
    }

    #[test]
    fn tree_search_without_match_leaves_selection_unchanged_and_sets_status() {
        let mut model = loaded_model((100, 30));
        let before = selected_name(&model);
        update(&mut model, Message::StartSearch);
        for c in "zzz-inexistant".chars() {
            update(&mut model, Message::SearchInput(c));
        }
        update(&mut model, Message::ConfirmSearch);
        assert_eq!(selected_name(&model), before);
        assert!(matches!(model.last_status, Some(StatusMessage::NoMatch)));
        assert_eq!(model.search.as_ref().unwrap().last_result, Some(false));
    }

    #[test]
    fn next_match_repeats_in_tree_from_detail_focus_and_previous_inverts() {
        let mut model = loaded_model((100, 30));
        select(&mut model, "simple-get.bru"); // "ping"
        update(&mut model, Message::StartSearch);
        for c in "no-".chars() {
            update(&mut model, Message::SearchInput(c));
        }
        update(&mut model, Message::ConfirmSearch);
        assert_eq!(selected_name(&model), "no-method.bru");

        update(&mut model, Message::NextFocus); // focus passe au détail
        assert_eq!(model.focus, Focus::Detail);
        update(&mut model, Message::NextMatch); // 'n', rejoue dans l'arbre
        assert_eq!(model.focus, Focus::Detail, "le focus ne change pas");
        assert_eq!(selected_name(&model), "no-seq");

        update(&mut model, Message::PreviousMatch); // 'N', sens inverse
        assert_eq!(selected_name(&model), "no-method.bru");
    }

    #[test]
    fn next_match_without_any_confirmed_search_does_nothing() {
        let mut model = loaded_model((100, 30));
        let before = selected_name(&model);
        update(&mut model, Message::NextMatch);
        assert_eq!(selected_name(&model), before);
    }

    #[test]
    fn detail_search_finds_a_match_beyond_the_top_of_the_panel() {
        // Panneau court : le détail de `scripted` dépasse ses 8 lignes
        // utiles (déjà vérifié par `detail_scroll_is_bounded_and_reset`),
        // ce qui force un vrai défilement pour rendre la ligne visible.
        let mut model = loaded_model((100, 12));
        select(&mut model, "scripted.bru");
        // L'assertion cherchée est dans l'onglet Assert.
        model.request_tab = RequestTab::Assert;
        update(&mut model, Message::NextFocus);
        assert_eq!(model.focus, Focus::Detail);
        let expected_line = plain_line_index_containing(&model, "res.body.ok: isTrue");

        update(&mut model, Message::StartSearch);
        for c in "res.body.ok".chars() {
            update(&mut model, Message::SearchInput(c));
        }
        update(&mut model, Message::ConfirmSearch);

        let areas = crate::app::view::layout_for(model.size, model.zoomed_panel())
            .expect("taille suffisante");
        let height = crate::app::view::inner(areas.detail).height;
        assert!(
            model.detail_scroll <= expected_line && expected_line < model.detail_scroll + height,
            "ligne {expected_line} non visible à scroll={}, hauteur={height}",
            model.detail_scroll
        );
        let (line, range) = model.detail_match.clone().expect("correspondance");
        assert_eq!(line, expected_line);
        let lines = crate::app::view::detail::plain_lines(&model);
        assert_eq!(&lines[line as usize][range], "res.body.ok");
    }

    #[test]
    fn response_search_finds_a_match_without_moving_the_detail_scroll() {
        use crate::app::test_support::runner_probe_model;

        // Panneau court : le résultat de `green.bru` dépasse ses lignes
        // utiles, ce qui force un vrai défilement pour rendre la ligne
        // visible.
        let mut model = runner_probe_model();
        select(&mut model, "green.bru");
        update(&mut model, Message::NextFocus); // Détail
        update(&mut model, Message::NextFocus); // Réponse
        assert_eq!(model.focus, Focus::Response);
        let detail_scroll_before = model.detail_scroll;
        let expected_line = response_line_index_containing(&model, "\"a\": [");

        update(&mut model, Message::StartSearch);
        for c in "\"a\": [".chars() {
            update(&mut model, Message::SearchInput(c));
        }
        update(&mut model, Message::ConfirmSearch);

        let areas = crate::app::view::layout_for(model.size, model.zoomed_panel())
            .expect("taille suffisante");
        let height = crate::app::view::inner(areas.response).height;
        assert!(
            model.response_scroll <= expected_line
                && expected_line < model.response_scroll + height,
            "ligne {expected_line} non visible à scroll={}, hauteur={height}",
            model.response_scroll
        );
        let (line, range) = model.response_match.clone().expect("correspondance");
        assert_eq!(line, expected_line);
        let lines = response_plain_lines(&model);
        assert_eq!(&lines[line as usize][range], "\"a\": [");

        // Le défilement et la mise en évidence du détail restent
        // inchangés par une recherche dans la réponse.
        assert_eq!(model.detail_scroll, detail_scroll_before);
        assert!(model.detail_match.is_none());
    }

    #[test]
    fn detail_search_without_match_leaves_scroll_unchanged() {
        let mut model = loaded_model((100, 30));
        select(&mut model, "post-json.bru");
        update(&mut model, Message::NextFocus);
        let before = model.detail_scroll;
        update(&mut model, Message::StartSearch);
        for c in "ne-figure-nulle-part".chars() {
            update(&mut model, Message::SearchInput(c));
        }
        update(&mut model, Message::ConfirmSearch);
        assert_eq!(model.detail_scroll, before);
        assert!(matches!(model.last_status, Some(StatusMessage::NoMatch)));
    }
}
