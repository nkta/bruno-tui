//! Gestion de l'arbre de la collection : navigation, pliage/dépliage,
//! rechargement, surveillance disque et filtrage (arbre et réponse jq).

use crate::app::filter::{FilterState, evaluate};
use crate::app::message::Message;
use crate::app::model::{
    CollectionState, EditSession, EnvironmentEditSession, Model, ResponseTab, Row, StatusMessage,
    visible_rows,
};
use crate::app::tree_filter::{TreeFilterState, filtered_rows, matches_tree_filter};
use crate::app::update::Command;
use crate::app::view::{inner, layout_for};
use crate::app::watch::is_watched_path;
use crate::app::workspace::{
    activate, active_member, member_collection, member_expanded, run_in_progress,
};
use crate::collection::TreeNode;
use crate::runner::report::ResponseStatus;
use crate::writer::FileStamp;

/// Recalcule les lignes visibles de l'arbre.
pub fn refresh_rows(model: &mut Model) {
    if model.workspace.is_some() {
        refresh_workspace_rows(model);
        return;
    }
    let previous_path = model.selected_node().map(|node| node.path().to_path_buf());
    let pattern = current_tree_filter_pattern(model).map(str::to_owned);

    let rows = match &model.collection {
        CollectionState::Loaded(collection) => {
            if let Some(p) = &pattern {
                let mut expanded = std::collections::HashSet::new();
                let rows = filtered_rows(&collection.tree, p, &mut expanded);
                model.tree.expanded = expanded;
                rows
            } else {
                visible_rows(&collection.tree, &model.tree.expanded)
            }
        }
        _ => Vec::new(),
    };
    model.tree.rows = rows;

    if model.tree.rows.is_empty() {
        model.tree.selected = 0;
        model.tree.offset = 0;
        if previous_path.is_some() {
            clear_detail_view_state(model);
        }
        return;
    }

    if let Some(p) = &pattern {
        // Garde la sélection sur le même nœud si possible, sinon sur la première correspondance.
        let found_same = previous_path
            .as_ref()
            .and_then(|path| model.active_row_position(|n| n.path() == path));

        if let Some(pos) = found_same {
            model.tree.selected = pos;
        } else {
            let first_match = model.active_row_position(|n| matches_tree_filter(n, p));
            model.tree.selected = first_match.unwrap_or(0);
        }
    } else {
        let last = model.tree.rows.len().saturating_sub(1);
        model.tree.selected = model.tree.selected.min(last);
    }

    let new_path = model.selected_node().map(|n| n.path().to_path_buf());
    if new_path != previous_path {
        clear_detail_view_state(model);
    }
}

/// Clé d'une ligne indépendante de sa position : membre et chemin du nœud
/// (`None` pour un en-tête de collection).
pub(crate) type RowKey = (usize, Option<std::path::PathBuf>);

/// Clé de la ligne sélectionnée.
pub(crate) fn selected_key(model: &Model) -> Option<RowKey> {
    let row = model.selected_row()?;
    Some((
        row.member,
        model.row_node(row).map(|node| node.path().to_path_buf()),
    ))
}

/// Indice de la ligne de cette clé.
pub(crate) fn row_of_key(model: &Model, key: &RowKey) -> Option<usize> {
    model.tree.rows.iter().position(|row| {
        row.member == key.0
            && model.row_node(row).map(|node| node.path()) == key.1.as_deref()
            && (key.1.is_some() || row.is_header())
    })
}

/// Lignes du mode workspace : un en-tête par collection, suivi de ses
/// nœuds visibles si elle est dépliée. Le filtre de l'arbre ne porte que
/// sur la collection active, alors dépliée.
fn refresh_workspace_rows(model: &mut Model) {
    let previous = selected_key(model);
    let pattern = current_tree_filter_pattern(model).map(str::to_owned);
    let Some(state) = model.workspace.as_ref() else {
        return;
    };
    let active = state.active;
    let mut filtered_expanded = None;
    let mut rows = Vec::new();
    for member in 0..state.members.len() {
        rows.push(Row::header(member));
        let is_filtered = member == active && pattern.is_some();
        if !(state.open.get(member).copied().unwrap_or(false) || is_filtered) {
            continue;
        }
        let Some(CollectionState::Loaded(collection)) = member_collection(model, member) else {
            continue;
        };
        let member_rows = match (&pattern, is_filtered) {
            (Some(p), true) => {
                let mut expanded = std::collections::HashSet::new();
                let rows = filtered_rows(&collection.tree, p, &mut expanded);
                filtered_expanded = Some(expanded);
                rows
            }
            _ => match member_expanded(model, member) {
                Some(expanded) => visible_rows(&collection.tree, expanded),
                None => Vec::new(),
            },
        };
        rows.extend(member_rows.into_iter().map(|row| Row {
            depth: row.depth + 1,
            member,
            ..row
        }));
    }
    if let Some(expanded) = filtered_expanded {
        model.tree.expanded = expanded;
    }
    model.tree.rows = rows;

    let same = previous.as_ref().and_then(|key| row_of_key(model, key));
    let first_match = pattern.as_deref().and_then(|p| {
        model
            .active_row_position(|n| matches_tree_filter(n, p))
            .filter(|_| same.is_none_or(|index| model.tree.rows[index].is_header()))
    });
    model.tree.selected = match (first_match, same) {
        (Some(index), _) | (None, Some(index)) => index,
        (None, None) => model
            .tree
            .selected
            .min(model.tree.rows.len().saturating_sub(1)),
    };
    if selected_key(model) != previous {
        clear_detail_view_state(model);
    }
}

pub(crate) fn navigate_tree(model: &mut Model, message: Message) {
    let len = model.tree.rows.len();
    if len == 0 {
        return;
    }
    let previous_path = model.selected_node().map(|node| node.path().to_path_buf());
    let before = model.tree.selected;
    if matches!(
        message,
        Message::Up | Message::Down | Message::Home | Message::End
    ) {
        model.selected_collection = false;
    }
    match message {
        Message::Up => model.tree.selected = before.saturating_sub(1),
        Message::Down => model.tree.selected = (before + 1).min(len - 1),
        Message::Home => model.tree.selected = 0,
        Message::End => model.tree.selected = len - 1,
        Message::Right => expand_or_enter(model),
        Message::Left => collapse_or_parent(model),
        _ => {}
    }
    finish_tree_selection(model, before, previous_path);
}

/// Sélectionne la ligne `index` de l'arbre, avec les mêmes effets et
/// restrictions que la navigation au clavier (clic dans l'arbre,
/// `mouse-support`).
pub(crate) fn select_row(model: &mut Model, index: usize) {
    if index >= model.tree.rows.len() {
        return;
    }
    model.selected_collection = false;
    let previous_path = model.selected_node().map(|node| node.path().to_path_buf());
    let before = model.tree.selected;
    model.tree.selected = index;
    finish_tree_selection(model, before, previous_path);
}

/// Suite commune à tout déplacement de la sélection de l'arbre : refus si
/// la session porte des modifications non enregistrées et que le nœud
/// change, sinon remise à zéro de l'état d'affichage du nœud.
pub(crate) fn finish_tree_selection(
    model: &mut Model,
    before: usize,
    previous_path: Option<std::path::PathBuf>,
) {
    if model.tree.selected != before
        && let Some(row) = model.selected_row().cloned()
        && !model.is_active_row(&row)
    {
        change_member(model, before, row);
        return;
    }
    if model.workspace.is_some() && model.tree.selected != before {
        model.selected_collection = selected_header(model).is_some();
    } else if selected_header(model).is_some() {
        model.selected_collection = true;
    }
    if model.tree.selected != before {
        let changed_node =
            model.selected_node().map(|node| node.path().to_path_buf()) != previous_path;
        if changed_node && super::selection_locked(model) {
            model.tree.selected = before;
            model.last_status = Some(StatusMessage::EditLocked);
            scroll_tree_into_view(model);
            return;
        }
        clear_detail_view_state(model);
    }
    reset_filter_if_selection_changed(model, previous_path.as_deref());
    scroll_tree_into_view(model);
}

/// La sélection est passée sur une ligne d'une autre collection : refus
/// pendant une exécution ou une édition non enregistrée, sinon changement
/// de collection active.
fn change_member(model: &mut Model, before: usize, row: Row) {
    let refusal = if run_in_progress(model) {
        Some(StatusMessage::RunLocked)
    } else if member_switch_locked(model) {
        Some(StatusMessage::EditLocked)
    } else {
        None
    };
    if let Some(status) = refusal {
        model.tree.selected = before;
        if selected_header(model).is_some() {
            model.selected_collection = true;
        }
        model.last_status = Some(status);
        scroll_tree_into_view(model);
        return;
    }
    switch_member(model, row);
}

/// Vrai si une session d'édition (requête ou environnement) porte des
/// modifications non enregistrées : elle appartient à la collection
/// active, qui ne peut donc pas changer.
pub(crate) fn member_switch_locked(model: &Model) -> bool {
    super::selection_locked(model)
        || model
            .environment_editing
            .as_ref()
            .is_some_and(EnvironmentEditSession::has_unsaved)
}

/// Rend active la collection de `row` et y place la sélection. Une
/// session d'édition d'environnement sans modification est fermée : elle
/// décrit un fichier de la collection quittée.
pub(crate) fn switch_member(model: &mut Model, row: Row) {
    model.environment_editing = None;
    let key = (
        row.member,
        model.row_node(&row).map(|node| node.path().to_path_buf()),
    );
    clear_detail_view_state(model);
    model.filter = None;
    activate(model, row.member);
    refresh_rows(model);
    if let Some(index) = row_of_key(model, &key) {
        model.tree.selected = index;
    }
    model.selected_collection = row.is_header();
    scroll_tree_into_view(model);
}

/// Efface l'état propre à l'affichage du détail et de la réponse d'un
/// nœud précis : appelé à chaque changement de sélection dans l'arbre,
/// qu'il vienne de la navigation normale ou d'une recherche.
pub(crate) fn clear_detail_view_state(model: &mut Model) {
    model.detail_scroll = 0;
    model.detail_selection = None;
    model.detail_match = None;
    model.response_scroll = 0;
    model.response_selection = None;
    model.response_match = None;
    model.response_tab = ResponseTab::Body;
    model.editing = None;
}

/// Vrai si la collection chargée est dans un format que le writer ne sait
/// pas écrire (OpenCollection YAML) : toute édition est alors refusée.
pub(crate) fn read_only_collection(model: &Model) -> bool {
    model
        .loaded()
        .is_some_and(|collection| !collection.format.is_editable())
}

/// Traite une notification de modification de fichiers surveillés sur disque.
pub(crate) fn handle_watch_reload(model: &mut Model, paths: Vec<std::path::PathBuf>) -> Command {
    if !external_change(model, &paths) {
        return Command::None;
    }

    // Session d'édition ouverte avec des modifications non enregistrées :
    // NE PAS écraser. Afficher un avertissement dans le statut (« fichier modifié sur disque »),
    // recharger seulement après la fermeture de la session.
    let has_unsaved_session = model.editing.as_ref().is_some_and(EditSession::has_unsaved)
        || model
            .environment_editing
            .as_ref()
            .is_some_and(EnvironmentEditSession::has_unsaved);

    if has_unsaved_session {
        model.pending_reload = true;
        model.last_status = Some(StatusMessage::FileModifiedOnDisk);
        Command::None
    } else {
        model.editing = None;
        model.environment_editing = None;
        model.pending_reload = false;
        Command::ReloadCollection
    }
}

/// Vrai si un des chemins modifiés justifie un rechargement de la
/// collection active et ne vient pas d'une écriture du TUI lui-même.
pub(crate) fn external_change(model: &Model, paths: &[std::path::PathBuf]) -> bool {
    let relevant_paths: Vec<&std::path::PathBuf> =
        paths.iter().filter(|p| is_watched_path(p)).collect();
    if relevant_paths.is_empty() {
        return false;
    }

    // Vérifie si ces modifications proviennent de nos propres écritures du TUI (Ctrl+S).
    // Si tous les chemins modifiés correspondent à un fichier enregistré récemment par le TUI
    // dont le FileStamp sur disque correspond à notre empreinte, il ne s'agit pas d'une modification externe.
    let root = model
        .loaded()
        .map(|c| c.root.clone())
        .unwrap_or_else(|| std::path::PathBuf::from("."));

    // Chemins relatifs à la racine, comme les clés de `last_saved_stamps`.
    let all_self_saved = relevant_paths.iter().all(|path| {
        model
            .last_saved_stamps
            .get(*path)
            .is_some_and(|saved_stamp| {
                FileStamp::capture(&root.join(path)).is_ok_and(|current| current == *saved_stamp)
            })
    });
    !all_self_saved
}

/// Dossier sélectionné : (chemin, déplié, vide).
pub(crate) fn selected_folder(model: &Model) -> Option<(std::path::PathBuf, bool, bool)> {
    match model.selected_node()? {
        TreeNode::Folder(folder) => Some((
            folder.path.clone(),
            model.tree.expanded.contains(&folder.path),
            folder.children.is_empty(),
        )),
        _ => None,
    }
}

pub(crate) fn expand_or_enter(model: &mut Model) {
    if let Some(member) = selected_header(model) {
        if !member_open(model, member) {
            set_member_open(model, member, true);
            refresh_rows(model);
        } else if model
            .tree
            .rows
            .get(model.tree.selected + 1)
            .is_some_and(|row| row.member == member && !row.is_header())
        {
            model.tree.selected += 1;
        }
        return;
    }
    match selected_folder(model) {
        Some((path, false, _)) => {
            model.tree.expanded.insert(path);
            refresh_rows(model);
        }
        Some((_, true, false)) => model.tree.selected += 1,
        _ => {}
    }
}

pub(crate) fn collapse_or_parent(model: &mut Model) {
    if let Some(member) = selected_header(model) {
        if member_open(model, member) {
            if member_is_filtered(model, member) {
                clear_tree_filter(model);
            }
            // Les lignes précédant l'en-tête ne changent pas.
            set_member_open(model, member, false);
            refresh_rows(model);
        }
        return;
    }
    if current_tree_filter_pattern(model).is_none()
        && let Some((path, true, _)) = selected_folder(model)
    {
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
    let member = row.member;
    if let Some(index) = model.tree.rows[..selected]
        .iter()
        .rposition(|candidate| candidate.member == member && candidate.address == parent)
    {
        model.tree.selected = index;
    }
}

/// Membre dont l'en-tête est sélectionné.
pub(crate) fn selected_header(model: &Model) -> Option<usize> {
    model
        .selected_row()
        .filter(|row| row.is_header())
        .map(|row| row.member)
}

fn member_open(model: &Model, member: usize) -> bool {
    model
        .workspace
        .as_ref()
        .and_then(|state| state.open.get(member).copied())
        .unwrap_or(false)
}

/// Déplie ou replie l'en-tête d'une collection.
pub(crate) fn set_member_open(model: &mut Model, member: usize, open: bool) {
    if let Some(flag) = model
        .workspace
        .as_mut()
        .and_then(|state| state.open.get_mut(member))
    {
        *flag = open;
    }
}

fn member_is_filtered(model: &Model, member: usize) -> bool {
    active_member(model) == Some(member) && current_tree_filter_pattern(model).is_some()
}

/// Hauteur utile du panneau de l'arbre.
pub(crate) fn tree_height(model: &Model) -> usize {
    layout_for(model.size, model.zoomed_panel())
        .map_or(1, |areas| usize::from(inner(areas.tree).height).max(1))
}

/// Ajuste `offset` pour que la sélection reste visible sans laisser de
/// vide en bas de panneau.
pub fn scroll_tree_into_view(model: &mut Model) {
    let height = tree_height(model);
    let tree = &mut model.tree;
    if tree.selected < tree.offset {
        tree.offset = tree.selected;
    } else if tree.selected >= tree.offset + height {
        tree.offset = tree.selected + 1 - height;
    }
    tree.offset = tree.offset.min(tree.rows.len().saturating_sub(height));
}

// --- Filtre de réponse ---------------------------------------------------

/// `|` : ouvre la saisie du filtre jq si la sélection courante a produit
/// une réponse HTTP avec un corps non nul (D4).
pub(crate) fn open_filter(model: &mut Model) {
    let Some(TreeNode::Request(request)) = model.selected_node() else {
        return;
    };
    let Some(outcome) = model.run.outcomes.get(&request.path) else {
        return;
    };
    if !matches!(outcome.result.response.status, ResponseStatus::Http(_)) {
        return;
    }
    if outcome.result.response.data.is_null() {
        return;
    }
    let target = request.path.clone();
    if let Some(filter) = &mut model.filter
        && filter.target == target
    {
        filter.editing = true;
    } else {
        model.filter = Some(FilterState::new(target));
    }
    // Le résultat du filtre s'affiche dans l'onglet Corps (`response-tabs`).
    model.response_tab = ResponseTab::Body;
}

/// `Entrée` en saisie de filtre : un filtre vide équivaut à une annulation,
/// sinon évalue le filtre jq et stocke le résultat dans `model.filter.applied` (D6).
pub(crate) fn confirm_filter(model: &mut Model) {
    let Some(filter) = &mut model.filter else {
        return;
    };
    if !filter.editing {
        return;
    }
    if filter.draft.is_empty() {
        filter.editing = false;
        return;
    }
    let Some(outcome) = model.run.outcomes.get(&filter.target) else {
        filter.editing = false;
        return;
    };
    let result = evaluate(&filter.draft, &outcome.result.response.data);
    filter.applied = Some(result);
    filter.editing = false;
}

/// Réinitialise le filtre si le chemin du nœud sélectionné a changé (D5).
pub(crate) fn reset_filter_if_selection_changed(
    model: &mut Model,
    previous_path: Option<&std::path::Path>,
) {
    let current_path = model.selected_node().map(TreeNode::path);
    if previous_path != current_path {
        model.filter = None;
    }
}

// --- Filtre de l'arbre Collection -----------------------------------------

/// Motif actuellement actif pour filtrer l'arbre (en saisie non vide ou validé).
pub(crate) fn current_tree_filter_pattern(model: &Model) -> Option<&str> {
    let filter = model.tree_filter.as_ref()?;
    if filter.editing {
        if filter.draft.is_empty() {
            None
        } else {
            Some(&filter.draft)
        }
    } else {
        filter.pattern.as_deref().filter(|p| !p.is_empty())
    }
}

/// Vrai si un filtre d'arbre validé (hors saisie) est actuellement actif.
pub(crate) fn has_active_tree_filter(model: &Model) -> bool {
    model
        .tree_filter
        .as_ref()
        .is_some_and(|f| !f.editing && f.pattern.as_ref().is_some_and(|p| !p.is_empty()))
}

/// Vrai si l'arbre est actuellement sous l'effet d'un filtre (en saisie non vide ou validé).
pub fn is_tree_filtered(model: &Model) -> bool {
    current_tree_filter_pattern(model).is_some()
}

/// Motif validé rappelé dans le titre du panneau Collection.
pub fn active_tree_filter_pattern(model: &Model) -> Option<&str> {
    let filter = model.tree_filter.as_ref()?;
    if filter.editing {
        None
    } else {
        filter.pattern.as_deref().filter(|p| !p.is_empty())
    }
}

/// Efface le filtre d'arbre actif et restaure l'état de dépliage d'avant le filtre.
pub(crate) fn clear_tree_filter(model: &mut Model) {
    if let Some(filter) = model.tree_filter.take() {
        model.tree.expanded = filter.saved_expanded;
        refresh_rows(model);
    }
}

/// Ouvre la saisie du filtre d'arbre ('f').
pub(crate) fn open_tree_filter(model: &mut Model) {
    if let Some(filter) = &mut model.tree_filter {
        filter.reopen();
        model.tree.expanded = filter.saved_expanded.clone();
    } else {
        model.tree_filter = Some(TreeFilterState::new(model.tree.expanded.clone()));
    }
    refresh_rows(model);
    scroll_tree_into_view(model);
}

/// Valide la saisie du filtre d'arbre (Entrée).
pub(crate) fn confirm_tree_filter(model: &mut Model) {
    let Some(filter) = &mut model.tree_filter else {
        return;
    };
    if !filter.editing {
        return;
    }
    if filter.draft.is_empty() {
        // Valider un motif vide efface le filtre et restaure le dépliage initial
        let saved_expanded = filter.saved_expanded.clone();
        model.tree_filter = None;
        model.tree.expanded = saved_expanded;
        refresh_rows(model);
        scroll_tree_into_view(model);
    } else {
        filter.pattern = Some(filter.draft.clone());
        filter.editing = false;
        filter.previous_pattern = None;
        refresh_rows(model);
        scroll_tree_into_view(model);
    }
}

/// Annule la saisie du filtre d'arbre (Échap).
pub(crate) fn cancel_tree_filter(model: &mut Model) {
    let Some(filter) = &mut model.tree_filter else {
        return;
    };
    if !filter.editing {
        return;
    }
    if let Some(prev) = filter.previous_pattern.take() {
        // Restaure le filtre précédent
        filter.pattern = Some(prev);
        filter.editing = false;
        filter.draft = filter.pattern.clone().unwrap_or_default();
        refresh_rows(model);
    } else {
        // Aucun filtre précédent : annule et restaure l'état non filtré
        let saved_expanded = filter.saved_expanded.clone();
        model.tree_filter = None;
        model.tree.expanded = saved_expanded;
        refresh_rows(model);
    }
    scroll_tree_into_view(model);
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::super::test_support::*;
    use super::super::*;
    use crate::app::model::{Focus, HistoryEntry, StatusMessage};
    use crate::app::test_support::{loaded_model, runner_probe_model, select, selected_name};

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
    fn open_filter_availability_conditions() {
        let mut model = runner_probe_model();

        // 1. Aucune exécution (retire green.bru des outcomes pour simuler une requête non exécutée)
        model.run.outcomes.remove(Path::new("green.bru"));
        select(&mut model, "green.bru");
        assert!(!model.run.outcomes.contains_key(Path::new("green.bru")));
        update(&mut model, Message::OpenFilter);
        assert!(
            model.filter.is_none(),
            "requête sans exécution : filtre indisponible"
        );

        // Nœud non requête (dossier)
        select(&mut model, "folder");
        update(&mut model, Message::OpenFilter);
        assert!(model.filter.is_none(), "dossier : filtre indisponible");

        // 2. Erreur de connexion (folder/down.bru)
        select(&mut model, "folder/down.bru");
        update(&mut model, Message::OpenFilter);
        assert!(
            model.filter.is_none(),
            "erreur de connexion : filtre indisponible"
        );

        // 3. Requête ignorée (skip.bru)
        select(&mut model, "skip.bru");
        update(&mut model, Message::OpenFilter);
        assert!(
            model.filter.is_none(),
            "requête ignorée : filtre indisponible"
        );

        // 4. Contre-exemples disponibles (ok.bru et json.bru)
        select(&mut model, "ok.bru");
        update(&mut model, Message::OpenFilter);
        assert!(model.filter.is_some(), "ok.bru : filtre disponible");
        assert_eq!(model.filter.as_ref().unwrap().target, Path::new("ok.bru"));
        assert!(model.filter.as_ref().unwrap().editing);

        select(&mut model, "json.bru");
        update(&mut model, Message::OpenFilter);
        assert!(model.filter.is_some(), "json.bru : filtre disponible");
        assert_eq!(model.filter.as_ref().unwrap().target, Path::new("json.bru"));
        assert!(model.filter.as_ref().unwrap().editing);
    }

    #[test]
    fn filter_composition_and_correction_and_cancel() {
        let mut model = runner_probe_model();
        select(&mut model, "json.bru");
        update(&mut model, Message::OpenFilter);
        assert!(model.filter.is_some());

        // Tape `.a.b`
        for c in ".a.b".chars() {
            update(&mut model, Message::FilterInput(c));
        }
        assert_eq!(model.filter.as_ref().unwrap().draft, ".a.b");

        // Deux effacements (retire 'b' et '.') -> ".a"
        update(&mut model, Message::FilterBackspace);
        update(&mut model, Message::FilterBackspace);
        assert_eq!(model.filter.as_ref().unwrap().draft, ".a");

        // Tape 'x' -> ".ax"
        update(&mut model, Message::FilterInput('x'));
        assert_eq!(model.filter.as_ref().unwrap().draft, ".ax");

        // Correction pour atteindre ".a.x"
        update(&mut model, Message::FilterBackspace);
        update(&mut model, Message::FilterInput('.'));
        update(&mut model, Message::FilterInput('x'));
        assert_eq!(model.filter.as_ref().unwrap().draft, ".a.x");

        // Annulation : referme la saisie sans toucher à applied
        update(&mut model, Message::CancelFilter);
        assert!(!model.filter.as_ref().unwrap().editing);
        assert!(model.filter.as_ref().unwrap().applied.is_none());
        assert_eq!(model.filter.as_ref().unwrap().draft, ".a.x");
    }

    #[test]
    fn confirm_filter_scenarios() {
        use crate::app::filter::FilterResult;

        let mut model = runner_probe_model();
        select(&mut model, "json.bru");

        // Scénario : validation d'un filtre vide équivaut à annuler
        update(&mut model, Message::OpenFilter);
        update(&mut model, Message::ConfirmFilter);
        assert!(!model.filter.as_ref().unwrap().editing);
        assert!(model.filter.as_ref().unwrap().applied.is_none());

        // Scénario : filtre extrayant une valeur (`.a` sur `{"a": [1, 2], "b": null}`)
        update(&mut model, Message::OpenFilter);
        for c in ".a".chars() {
            update(&mut model, Message::FilterInput(c));
        }
        update(&mut model, Message::ConfirmFilter);
        assert!(!model.filter.as_ref().unwrap().editing);
        match &model.filter.as_ref().unwrap().applied {
            Some(FilterResult::Output(out)) => {
                assert_eq!(out.len(), 1);
                assert_eq!(out[0], "[\n  1,\n  2\n]");
            }
            other => panic!("attendu Output, obtenu {other:?}"),
        }

        // Scénario : filtre à plusieurs sorties (`.a[]`)
        update(&mut model, Message::OpenFilter);
        model.filter.as_mut().unwrap().draft.clear();
        for c in ".a[]".chars() {
            update(&mut model, Message::FilterInput(c));
        }
        update(&mut model, Message::ConfirmFilter);
        match &model.filter.as_ref().unwrap().applied {
            Some(FilterResult::Output(out)) => {
                assert_eq!(out.len(), 2);
                assert_eq!(out[0], "1");
                assert_eq!(out[1], "2");
            }
            other => panic!("attendu Output à 2 sorties, obtenu {other:?}"),
        }

        // Scénario : correction après erreur
        // 1. Filtre syntaxiquement invalide `.a.b |`
        update(&mut model, Message::OpenFilter);
        model.filter.as_mut().unwrap().draft.clear();
        for c in ".a.b |".chars() {
            update(&mut model, Message::FilterInput(c));
        }
        update(&mut model, Message::ConfirmFilter);
        assert!(matches!(
            model.filter.as_ref().unwrap().applied,
            Some(FilterResult::Error(_))
        ));

        // 2. Correction par filtre valide `.b`
        update(&mut model, Message::OpenFilter);
        model.filter.as_mut().unwrap().draft.clear();
        for c in ".b".chars() {
            update(&mut model, Message::FilterInput(c));
        }
        update(&mut model, Message::ConfirmFilter);
        match &model.filter.as_ref().unwrap().applied {
            Some(FilterResult::Output(out)) => {
                assert_eq!(out.len(), 1);
                assert_eq!(out[0], "null");
            }
            other => panic!("attendu Output après correction, obtenu {other:?}"),
        }
    }

    #[test]
    fn filter_cleared_on_selection_change_and_not_reapplied() {
        let mut model = runner_probe_model();
        select(&mut model, "json.bru");

        // Applique un filtre sur json.bru
        update(&mut model, Message::OpenFilter);
        for c in ".a".chars() {
            update(&mut model, Message::FilterInput(c));
        }
        update(&mut model, Message::ConfirmFilter);
        assert!(model.filter.is_some());

        // Navigation vers un autre nœud (Down)
        let initial_selected = model.tree.selected;
        update(&mut model, Message::Down);
        assert_ne!(model.tree.selected, initial_selected);
        assert!(
            model.filter.is_none(),
            "le filtre doit disparaître au changement de sélection"
        );

        // Revenir sur la requête précédemment filtrée (Up)
        update(&mut model, Message::Up);
        assert_eq!(model.tree.selected, initial_selected);
        assert!(
            model.filter.is_none(),
            "le filtre ne doit pas être réappliqué"
        );
    }

    #[test]
    fn tree_filter_open_type_and_backspace_filters_live() {
        let mut model = loaded_model((100, 30));
        let total_initial = model.tree.rows.len();
        assert!(total_initial > 5);

        // 'f' ouvre la saisie du filtre
        update(&mut model, Message::OpenTreeFilter);
        assert!(model.tree_filter.as_ref().expect("filtre présent").editing);
        assert_eq!(model.tree_filter.as_ref().unwrap().draft, "");

        // La saisie en direct filtre les requêtes
        for c in "ping".chars() {
            update(&mut model, Message::TreeFilterInput(c));
        }
        assert_eq!(model.tree_filter.as_ref().unwrap().draft, "ping");
        // "ping" correspond à simple-get.bru
        assert_eq!(model.tree.rows.len(), 1);
        assert_eq!(selected_name(&model), "ping");

        // Backspace modifie le motif et recalcule en direct
        update(&mut model, Message::TreeFilterBackspace);
        assert_eq!(model.tree_filter.as_ref().unwrap().draft, "pin");
        assert_eq!(model.tree.rows.len(), 1);
    }

    #[test]
    fn tree_filter_matches_by_name_or_path_case_and_accent_insensitive() {
        let mut model = loaded_model((100, 30));

        // Filtrage insensible à la casse par nom
        update(&mut model, Message::OpenTreeFilter);
        for c in "PING".chars() {
            update(&mut model, Message::TreeFilterInput(c));
        }
        assert_eq!(model.tree.rows.len(), 1);
        assert_eq!(selected_name(&model), "ping");

        // Filtrage par chemin : "grp/sub" ou "deep"
        update(&mut model, Message::CancelTreeFilter);
        update(&mut model, Message::OpenTreeFilter);
        for c in "deep".chars() {
            update(&mut model, Message::TreeFilterInput(c));
        }
        // "grp" et "grp/sub" sont des dossiers parents dépliés automatiquement ("Groupe", "Sous-groupe"), plus "deep"
        let names: Vec<String> = model
            .tree
            .rows
            .iter()
            .map(|r| {
                model
                    .node_at(&r.address)
                    .map(crate::app::view::tree::display_name)
                    .unwrap_or_default()
            })
            .collect();
        assert_eq!(names, vec!["Groupe", "Sous-groupe", "deep"]);
    }

    #[test]
    fn tree_filter_confirm_keeps_filter_active() {
        let mut model = loaded_model((100, 30));
        update(&mut model, Message::OpenTreeFilter);
        for c in "ping".chars() {
            update(&mut model, Message::TreeFilterInput(c));
        }
        update(&mut model, Message::ConfirmTreeFilter);

        // La saisie est terminée mais le filtre reste actif
        let filter = model.tree_filter.as_ref().expect("filtre actif");
        assert!(!filter.editing);
        assert_eq!(filter.pattern.as_deref(), Some("ping"));
        assert_eq!(model.tree.rows.len(), 1);
    }

    #[test]
    fn tree_filter_cancel_during_input_restores_previous_state() {
        let mut model = loaded_model((100, 30));
        let total_initial = model.tree.rows.len();

        // 1. Annulation d'une première saisie -> retour à l'état non filtré
        update(&mut model, Message::OpenTreeFilter);
        for c in "ping".chars() {
            update(&mut model, Message::TreeFilterInput(c));
        }
        assert_eq!(model.tree.rows.len(), 1);
        update(&mut model, Message::CancelTreeFilter);
        assert!(model.tree_filter.is_none());
        assert_eq!(model.tree.rows.len(), total_initial);

        // 2. Filtre actif ("ping"), nouvelle saisie de "inexistant", puis annulation
        update(&mut model, Message::OpenTreeFilter);
        for c in "ping".chars() {
            update(&mut model, Message::TreeFilterInput(c));
        }
        update(&mut model, Message::ConfirmTreeFilter);
        assert_eq!(model.tree.rows.len(), 1);

        update(&mut model, Message::OpenTreeFilter);
        for c in "inexistant".chars() {
            update(&mut model, Message::TreeFilterInput(c));
        }
        assert!(model.tree.rows.is_empty());
        update(&mut model, Message::CancelTreeFilter);

        // Restaure le filtre précédent ("ping")
        let filter = model
            .tree_filter
            .as_ref()
            .expect("filtre précédent restauré");
        assert!(!filter.editing);
        assert_eq!(filter.pattern.as_deref(), Some("ping"));
        assert_eq!(model.tree.rows.len(), 1);
    }

    #[test]
    fn tree_filter_cleared_by_empty_confirm_or_escape_without_input() {
        let mut model = loaded_model((100, 30));
        let total_initial = model.tree.rows.len();

        // Valider un motif vide efface le filtre actif
        update(&mut model, Message::OpenTreeFilter);
        for c in "ping".chars() {
            update(&mut model, Message::TreeFilterInput(c));
        }
        update(&mut model, Message::ConfirmTreeFilter);
        assert!(model.tree_filter.is_some());

        update(&mut model, Message::OpenTreeFilter);
        for _ in 0..4 {
            update(&mut model, Message::TreeFilterBackspace);
        }
        update(&mut model, Message::ConfirmTreeFilter);
        assert!(model.tree_filter.is_none());
        assert_eq!(model.tree.rows.len(), total_initial);

        // Échap en focus Collection sans saisie en cours efface le filtre actif
        update(&mut model, Message::OpenTreeFilter);
        for c in "ping".chars() {
            update(&mut model, Message::TreeFilterInput(c));
        }
        update(&mut model, Message::ConfirmTreeFilter);
        assert!(model.tree_filter.is_some());

        model.focus = Focus::Tree;
        update(&mut model, Message::FocusTree); // Échap en focus Tree
        assert!(model.tree_filter.is_none());
        assert_eq!(model.tree.rows.len(), total_initial);
    }

    #[test]
    fn tree_filter_selection_preserves_node_or_jumps_to_first_match() {
        let mut model = loaded_model((100, 30));
        select(&mut model, "post-json.bru");
        assert_eq!(selected_name(&model), "post-json");

        // Si le filtre inclut le nœud sélectionné ("post"), il reste sélectionné
        update(&mut model, Message::OpenTreeFilter);
        for c in "post".chars() {
            update(&mut model, Message::TreeFilterInput(c));
        }
        assert_eq!(selected_name(&model), "post-json");

        // Si le filtre ne correspond pas au nœud ("ping"), la sélection passe à la 1re correspondance
        update(&mut model, Message::CancelTreeFilter);
        select(&mut model, "post-json.bru");
        update(&mut model, Message::OpenTreeFilter);
        for c in "ping".chars() {
            update(&mut model, Message::TreeFilterInput(c));
        }
        assert_eq!(selected_name(&model), "ping");
    }

    #[test]
    fn tree_filter_no_match_empties_rows() {
        let mut model = loaded_model((100, 30));
        update(&mut model, Message::OpenTreeFilter);
        for c in "motif-totalement-introuvable".chars() {
            update(&mut model, Message::TreeFilterInput(c));
        }
        assert!(model.tree.rows.is_empty());
        assert_eq!(model.tree.selected, 0);
    }

    #[test]
    fn tree_filter_select_request_in_tree_clears_filter_if_target_is_hidden() {
        let mut model = loaded_model((100, 30));
        update(&mut model, Message::OpenTreeFilter);
        for c in "ping".chars() {
            update(&mut model, Message::TreeFilterInput(c));
        }
        update(&mut model, Message::ConfirmTreeFilter);
        assert!(model.tree_filter.is_some());

        // Sélection d'une requête masquée ("grp/sub/deep.bru") efface le filtre
        let target = Path::new("grp/sub/deep.bru");
        assert!(select_request_in_tree(&mut model, target));
        assert!(model.tree_filter.is_none());
        assert_eq!(selected_name(&model), "deep");

        // Sélection d'une requête visible conserve le filtre
        update(&mut model, Message::OpenTreeFilter);
        for c in "deep".chars() {
            update(&mut model, Message::TreeFilterInput(c));
        }
        update(&mut model, Message::ConfirmTreeFilter);
        assert!(model.tree_filter.is_some());

        assert!(select_request_in_tree(&mut model, target));
        assert!(model.tree_filter.is_some());
        assert_eq!(selected_name(&model), "deep");
    }

    #[test]
    fn tree_filter_restores_saved_expanded_state_on_exit() {
        let mut model = loaded_model((100, 30));
        // Initialement, "grp" n'est pas dans expanded
        let initial_expanded = model.tree.expanded.clone();
        assert!(!initial_expanded.contains(Path::new("grp")));

        // Le filtre déplie temporairement "grp" et "grp/sub"
        update(&mut model, Message::OpenTreeFilter);
        for c in "deep".chars() {
            update(&mut model, Message::TreeFilterInput(c));
        }
        update(&mut model, Message::ConfirmTreeFilter);
        assert_eq!(model.tree.rows.len(), 3);

        // Désactivation du filtre restaure l'état d'expansion d'origine
        update(&mut model, Message::FocusTree);
        assert!(model.tree_filter.is_none());
        assert_eq!(model.tree.expanded, initial_expanded);
    }

    #[test]
    fn reload_preserves_selection_by_path_expanded_folders_and_focus() {
        let mut model = loaded_model((100, 30));
        // Déplier grp et grp/sub, sélectionner grp/sub/deep.bru
        select(&mut model, "grp/sub/deep.bru");
        model.focus = Focus::Detail;
        let selected_path = model.selected_node().map(|n| n.path().to_path_buf());
        assert_eq!(selected_path, Some(PathBuf::from("grp/sub/deep.bru")));
        assert!(model.tree.expanded.contains(Path::new("grp")));
        assert!(model.tree.expanded.contains(Path::new("grp/sub")));

        // Rechargement
        use crate::collection::CollectionLoader;
        let reloaded = crate::collection::BruLoader.load(&model.source.clone());
        let cmd = update(&mut model, Message::CollectionLoaded(reloaded));
        assert!(matches!(
            cmd,
            Command::ResolveSecrets { .. } | Command::None
        ));

        // Sélection préservée par chemin
        let reloaded_selected = model.selected_node().map(|n| n.path().to_path_buf());
        assert_eq!(reloaded_selected, Some(PathBuf::from("grp/sub/deep.bru")));
        // Dossiers dépliés préservés
        assert!(model.tree.expanded.contains(Path::new("grp")));
        assert!(model.tree.expanded.contains(Path::new("grp/sub")));
        // Focus préservé
        assert_eq!(model.focus, Focus::Detail);
        // Message de statut indiquant le rechargement
        assert_eq!(model.last_status, Some(StatusMessage::CollectionReloaded));
    }

    #[test]
    fn reload_preserves_execution_history() {
        let mut model = loaded_model((100, 30));
        model.history.push_back(HistoryEntry {
            started_at: std::time::SystemTime::UNIX_EPOCH,
            target: PathBuf::from("simple-get.bru"),
            recursive: false,
            outcome: crate::app::model::HistoryOutcome::Cancelled,
        });

        use crate::collection::CollectionLoader;
        let reloaded = crate::collection::BruLoader.load(&model.source.clone());
        update(&mut model, Message::CollectionLoaded(reloaded));

        assert_eq!(model.history.len(), 1);
        assert_eq!(model.history[0].target, PathBuf::from("simple-get.bru"));
    }

    #[test]
    fn watch_reload_with_unsaved_edit_defers_reload_and_shows_warning() {
        let mut model = edit_session("simple-get.bru", (100, 30));
        // Marquer la session dirty
        model.editing.as_mut().unwrap().dirty = true;
        assert!(model.editing.as_ref().unwrap().has_unsaved());

        // Événement de modification sur disque
        let cmd = update(
            &mut model,
            Message::WatchReload(vec![PathBuf::from("simple-get.bru")]),
        );

        // La commande ne recharge pas immédiatement
        assert!(matches!(cmd, Command::None));
        // Statut d'avertissement affiché
        assert_eq!(model.last_status, Some(StatusMessage::FileModifiedOnDisk));
        // Rechargement en attente
        assert!(model.pending_reload);
        // L'édition n'est pas écrasée
        assert!(model.editing.is_some());
        assert!(model.editing.as_ref().unwrap().dirty);

        // Fermeture de la session d'édition (par exemple validation d'abandon)
        model.confirm = Some(crate::app::model::PendingConfirm::DiscardEdit);
        let close_cmd = update(&mut model, Message::ConfirmYes);

        // La fermeture déclenche le rechargement différé
        assert!(matches!(close_cmd, Command::ReloadCollection));
        assert!(!model.pending_reload);
        assert!(model.editing.is_none());
    }

    #[test]
    fn watch_reload_without_unsaved_edit_reloads_immediately() {
        let mut model = edit_session("simple-get.bru", (100, 30));
        // Session propre (aucune modification)
        assert!(!model.editing.as_ref().unwrap().has_unsaved());

        let cmd = update(
            &mut model,
            Message::WatchReload(vec![PathBuf::from("simple-get.bru")]),
        );

        // Recharge immédiatement et referme la session propre
        assert!(matches!(cmd, Command::ReloadCollection));
        assert!(model.editing.is_none());
        assert!(!model.pending_reload);
        assert_ne!(model.last_status, Some(StatusMessage::FileModifiedOnDisk));
    }

    #[test]
    fn watch_reload_ignored_for_non_watched_paths() {
        let mut model = loaded_model((100, 30));
        let cmd = update(
            &mut model,
            Message::WatchReload(vec![
                PathBuf::from("README.md"),
                PathBuf::from(".git/index"),
            ]),
        );

        assert!(matches!(cmd, Command::None));
        assert!(!model.pending_reload);
    }

    #[test]
    fn watch_reload_from_tui_save_causes_no_false_warning() {
        let mut model = edit_session("simple-get.bru", (100, 30));
        let session = model.editing.as_mut().unwrap();
        session.dirty = true;
        let stamp = session.stamp;

        // Simule que le TUI vient de sauvegarder ce fichier avec ce stamp
        model
            .last_saved_stamps
            .insert(PathBuf::from("simple-get.bru"), stamp);

        // L'événement du watcher arrive pour ce fichier
        // Le stamp sur disque de simple-get.bru correspond à celui de la fixture
        let full_path = model.loaded().unwrap().root.join("simple-get.bru");
        if let Ok(current_stamp) = crate::writer::FileStamp::capture(&full_path) {
            model
                .last_saved_stamps
                .insert(PathBuf::from("simple-get.bru"), current_stamp);
            let cmd = update(
                &mut model,
                Message::WatchReload(vec![PathBuf::from("simple-get.bru")]),
            );

            // Aucun avertissement, aucun rechargement inutile
            assert!(matches!(cmd, Command::None));
            assert_ne!(model.last_status, Some(StatusMessage::FileModifiedOnDisk));
            assert!(!model.pending_reload);
        }
    }
}
