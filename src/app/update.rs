//! Transitions du modèle.
//!
//! `update` est la seule fonction qui modifie le modèle. Elle ne fait
//! aucune I/O : le chargement et le rendu sont pilotés par la boucle.

use std::ops::RangeInclusive;

use super::message::Message;
use super::model::{
    CollectionState, DetailSelection, Exit, Focus, Model, StatusMessage, tree_node_at, visible_rows,
};
use super::search::{SearchScope, SearchState, find_detail_match, find_tree_match};
use super::view::detail::plain_lines;
use super::view::{detail::detail_text, inner, layout_for};
use crate::collection::TreeNode;
use crate::runner::RunRequest;

/// Effet demandé par `update`, à exécuter par la boucle `run`, qui seule
/// détient le `BruRunner` et tourne dans un contexte tokio.
#[derive(Debug)]
pub enum Command {
    None,
    /// Démarrer cette exécution ; la boucle appelle `BruRunner::start` puis
    /// renvoie le résultat à `update` via `Message::RunStarted`.
    StartRun {
        request: RunRequest,
        target: std::path::PathBuf,
    },
    /// Copier ce texte dans le presse-papiers du système ; la boucle
    /// l'exécute dans `spawn_blocking` et renvoie le résultat à `update`
    /// via `Message::ClipboardResult`.
    CopyToClipboard {
        token: u64,
        text: String,
    },
}

/// Applique un message au modèle.
pub fn update(model: &mut Model, message: Message) -> Command {
    match message {
        Message::Quit | Message::ForceQuit => {
            model.exit = Some(Exit::Normal);
            Command::None
        }
        Message::TerminalClosed(error) => {
            model.exit = Some(Exit::TerminalError(error));
            Command::None
        }
        Message::Resize { width, height } => {
            model.size = (width, height);
            scroll_tree_into_view(model);
            model.detail_scroll = model.detail_scroll.min(detail_max_scroll(model));
            Command::None
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
            Command::None
        }
        Message::NextFocus => {
            model.focus = match model.focus {
                Focus::Tree => Focus::Detail,
                Focus::Detail => Focus::Tree,
            };
            Command::None
        }
        Message::FocusTree => {
            // Une sélection visuelle active absorbe le premier `Échap` (ne
            // rien copier, ne pas déplacer le défilement) ; le focus ne
            // rejoint l'arbre qu'à un `Échap` suivant, sans sélection.
            if model.detail_selection.take().is_none() {
                model.focus = Focus::Tree;
            }
            Command::None
        }
        Message::RunSelected => run_selected(model),
        Message::CancelRun => {
            cancel_run(model);
            Command::None
        }
        Message::RunStarted {
            id,
            target,
            recursive,
            handle,
        } => {
            model.run.active = Some(super::model::ActiveRun {
                id,
                target,
                recursive,
                handle: Some(handle),
            });
            Command::None
        }
        Message::RunFinished(event) => {
            run_finished(model, event);
            Command::None
        }
        Message::StartSearch => {
            start_search(model);
            Command::None
        }
        Message::SearchInput(c) => {
            if let Some(search) = &mut model.search {
                search.draft.push(c);
            }
            Command::None
        }
        Message::SearchBackspace => {
            if let Some(search) = &mut model.search {
                search.draft.pop();
            }
            Command::None
        }
        Message::ConfirmSearch => {
            confirm_search(model);
            Command::None
        }
        Message::CancelSearch => {
            if let Some(search) = &mut model.search {
                search.editing = false;
            }
            Command::None
        }
        Message::NextMatch => {
            repeat_search(model, true);
            Command::None
        }
        Message::PreviousMatch => {
            repeat_search(model, false);
            Command::None
        }
        Message::ToggleVisual => {
            toggle_visual(model);
            Command::None
        }
        Message::Yank => yank(model),
        Message::ClipboardResult { token, result } => {
            apply_clipboard_result(model, token, result);
            Command::None
        }
        navigation => {
            match model.focus {
                Focus::Tree => navigate_tree(model, navigation),
                Focus::Detail => scroll_detail(model, navigation),
            }
            Command::None
        }
    }
}

/// `r` : lance la requête ou le dossier sélectionné, sauf exécution déjà en
/// cours, nœud en erreur, ou absence de sélection.
fn run_selected(model: &mut Model) -> Command {
    if model.run.active.is_some() {
        return Command::None;
    }
    let target = match model.selected_node() {
        Some(TreeNode::Request(request)) => Some((request.path.clone(), false)),
        Some(TreeNode::Folder(folder)) => Some((folder.path.clone(), true)),
        Some(TreeNode::Error(_)) | None => None,
    };
    let Some((target, recursive)) = target else {
        return Command::None;
    };
    let Some(root) = model.loaded().map(|collection| collection.root.clone()) else {
        return Command::None;
    };
    model.run.last_failure = None;
    Command::StartRun {
        request: RunRequest {
            collection_root: root,
            targets: vec![target.clone()],
            recursive,
            env: None,
            env_vars: Vec::new(),
        },
        target,
    }
}

/// `Ctrl+X` : annule l'exécution en cours, sans effet si aucune ou si déjà
/// demandée.
fn cancel_run(model: &mut Model) {
    if let Some(active) = &mut model.run.active
        && let Some(handle) = active.handle.take()
    {
        handle.cancel();
    }
}

/// Applique l'issue d'une exécution, en ignorant un identifiant obsolète.
fn run_finished(model: &mut Model, event: crate::runner::RunEvent) {
    let matches_active = model
        .run
        .active
        .as_ref()
        .is_some_and(|active| active.id == event.id);
    if !matches_active {
        return;
    }
    let Some(active) = model.run.active.take() else {
        return;
    };
    match event.outcome {
        crate::runner::RunOutcome::Completed { report, exit_code } => {
            for result in report.0.into_iter().flat_map(|iteration| iteration.results) {
                let key = std::path::PathBuf::from(&result.test.filename);
                model
                    .run
                    .outcomes
                    .insert(key, super::model::RequestOutcome { result, exit_code });
            }
            model.run.last_failure = None;
        }
        crate::runner::RunOutcome::Failed(error) => {
            model.run.last_failure = Some(super::model::RunFailure::Error {
                target: active.target,
                error,
            });
        }
        crate::runner::RunOutcome::Cancelled => {
            model.run.last_failure = Some(super::model::RunFailure::Cancelled {
                target: active.target,
            });
        }
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
        clear_detail_view_state(model);
    }
    scroll_tree_into_view(model);
}

/// Efface l'état propre à l'affichage du détail d'un nœud précis : appelé à
/// chaque changement de sélection dans l'arbre, qu'il vienne de la
/// navigation normale ou d'une recherche.
fn clear_detail_view_state(model: &mut Model) {
    model.detail_scroll = 0;
    model.detail_selection = None;
    model.detail_match = None;
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

/// Nombre de lignes logiques du détail (`Text.lines`, une entrée par
/// `Line`). Unité commune au défilement, à la recherche et à la sélection
/// visuelle : `ratatui::Paragraph` ne permet pas de récupérer le texte
/// réellement retourné à la ligne à une largeur donnée (seul un compte est
/// public, `Paragraph::line_count`), donc le défilement avance d'une ligne
/// logique à la fois plutôt que d'une ligne visuelle après retour à la
/// ligne. Le rendu continue d'envelopper visuellement les lignes trop
/// longues (`Wrap`) ; seule l'unité de défilement change.
fn detail_line_count(model: &Model) -> usize {
    detail_text(model).lines.len()
}

/// Défilement maximal du détail : lignes logiques moins la hauteur du
/// panneau.
fn detail_max_scroll(model: &Model) -> u16 {
    let Some(areas) = layout_for(model.size) else {
        return 0;
    };
    let height = inner(areas.detail).height;
    let max = detail_line_count(model).saturating_sub(usize::from(height));
    u16::try_from(max).unwrap_or(u16::MAX)
}

/// Dernière ligne visible du panneau de détail, à la position de
/// défilement courante. Toujours la vraie dernière ligne du contenu quand
/// `detail_scroll` est à son maximum (voir le calcul dans `design.md`,
/// D5) : c'est ce qui permet à une sélection visuelle d'atteindre des
/// lignes qu'aucun sommet de panneau ne peut jamais atteindre seul.
pub(crate) fn bottom_of_viewport(model: &Model) -> u16 {
    let Some(areas) = layout_for(model.size) else {
        return model.detail_scroll;
    };
    let height = inner(areas.detail).height;
    let lines = detail_line_count(model);
    if lines == 0 {
        return 0;
    }
    let bottom = model.detail_scroll.saturating_add(height.saturating_sub(1));
    bottom.min(u16::try_from(lines - 1).unwrap_or(u16::MAX))
}

/// Plage de lignes couvertes par la sélection visuelle active, s'il y en a
/// une. Jamais stockée à part : dérivée de l'ancre et du défilement
/// courant à chaque appel.
pub(crate) fn selection_range(model: &Model) -> Option<RangeInclusive<u16>> {
    let anchor = model.detail_selection?.anchor;
    let bottom = bottom_of_viewport(model);
    Some(anchor.min(bottom)..=anchor.max(bottom))
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

// --- Recherche -----------------------------------------------------------

/// `/` : ouvre la saisie sur le motif déjà validé, ou vide s'il n'y en a
/// pas encore eu. `scope` n'est fixé qu'à la validation (`confirm_search`),
/// pas ici : rouvrir sans valider ne doit pas changer sur quel panneau `n`
/// rejoue une recherche précédente.
fn start_search(model: &mut Model) {
    let search = model.search.get_or_insert_with(SearchState::default);
    search.draft = search.pattern.clone();
    search.editing = true;
}

/// `Entrée` en saisie : motif vide = annulation (comme `Échap`), sinon
/// valide le motif, fixe le panneau associé au focus courant, et lance la
/// recherche vers l'avant.
fn confirm_search(model: &mut Model) {
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
fn repeat_search(model: &mut Model, same_direction: bool) {
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
    let all = super::model::all_rows(&collection.tree);
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
    if let Some(index) = model
        .tree
        .rows
        .iter()
        .position(|row| row.address == address)
    {
        model.tree.selected = index;
    }
    clear_detail_view_state(model);
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

// --- Sélection visuelle et copie -----------------------------------------

/// `v`, hors saisie, en focus Détail seulement (produit uniquement dans ce
/// cas par `message.rs`, mais revérifié ici par défense).
fn toggle_visual(model: &mut Model) {
    if model.focus != Focus::Detail {
        return;
    }
    if model.detail_selection.take().is_some() {
        return;
    }
    model.detail_selection = Some(DetailSelection {
        anchor: model.detail_scroll,
    });
}

/// `y` : copie la sélection visuelle si elle est active, sinon la seule
/// ligne au sommet du panneau. Sans effet hors focus Détail (l'arbre et la
/// saisie de recherche n'ont pas de notion de ligne à copier).
fn yank(model: &mut Model) -> Command {
    if model.focus != Focus::Detail {
        return Command::None;
    }
    let lines = plain_lines(model);
    let text = match selection_range(model) {
        Some(range) => {
            let selected: Vec<&str> = lines
                .iter()
                .enumerate()
                .filter(|(index, _)| range.contains(&(*index as u16)))
                .map(|(_, line)| line.as_str())
                .collect();
            model.detail_selection = None;
            selected.join("\n")
        }
        None => lines
            .get(usize::from(model.detail_scroll))
            .cloned()
            .unwrap_or_default(),
    };
    let token = model.next_clipboard_token;
    model.next_clipboard_token += 1;
    model.pending_clipboard_token = Some(token);
    Command::CopyToClipboard { token, text }
}

/// Résultat d'une copie précédemment lancée. Un jeton qui ne correspond
/// plus au dernier émis est ignoré (résultat en retard sur une copie plus
/// récente, cf. `Model::pending_clipboard_token`).
fn apply_clipboard_result(
    model: &mut Model,
    token: u64,
    result: Result<(), super::clipboard::ClipboardError>,
) {
    if model.pending_clipboard_token != Some(token) {
        return;
    }
    model.pending_clipboard_token = None;
    model.last_status = Some(match result {
        Ok(()) => StatusMessage::Copied,
        Err(error) => StatusMessage::ClipboardError(error.to_string()),
    });
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::app::model::{ActiveRun, RunFailure};
    use crate::app::test_support::{loaded_model, select, selected_name};
    use crate::collection::LoadError;
    use crate::runner;

    fn sample_request_result(filename: &str) -> runner::report::RequestResult {
        use runner::report::{
            RequestFile, RequestInfo, ResponseInfo, ResponseStatus, ResultStatus,
        };
        runner::report::RequestResult {
            name: filename.trim_end_matches(".bru").to_owned(),
            path: filename.trim_end_matches(".bru").to_owned(),
            test: RequestFile {
                filename: filename.to_owned(),
            },
            request: RequestInfo {
                method: "GET".into(),
                url: "http://x".into(),
                headers: Default::default(),
            },
            response: ResponseInfo {
                status: ResponseStatus::Http(200),
                status_text: Some("OK".into()),
                headers: Some(Default::default()),
                data: serde_json::Value::Null,
                url: Some("http://x".into()),
                response_time: 1,
            },
            error: None,
            status: ResultStatus::Pass,
            skipped: false,
            assertion_results: Vec::new(),
            test_results: Vec::new(),
            pre_request_test_results: Vec::new(),
            post_response_test_results: Vec::new(),
            should_stop_runner_execution: false,
            run_duration: 0.0,
            iteration_index: 0,
        }
    }

    fn sample_report(filenames: &[&str]) -> runner::Report {
        let results = filenames.iter().map(|f| sample_request_result(f)).collect();
        runner::Report(vec![runner::report::Iteration {
            iteration_index: 0,
            results,
            summary: runner::report::Summary {
                total_requests: 0,
                passed_requests: 0,
                failed_requests: 0,
                error_requests: 0,
                skipped_requests: 0,
                total_assertions: 0,
                passed_assertions: 0,
                failed_assertions: 0,
                total_tests: 0,
                passed_tests: 0,
                failed_tests: 0,
                total_pre_request_tests: 0,
                passed_pre_request_tests: 0,
                failed_pre_request_tests: 0,
                total_post_response_tests: 0,
                passed_post_response_tests: 0,
                failed_post_response_tests: 0,
            },
        }])
    }

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

    #[test]
    fn run_selected_on_a_request_starts_a_non_recursive_run() {
        let mut model = loaded_model((100, 30));
        select(&mut model, "simple-get.bru");
        match update(&mut model, Message::RunSelected) {
            Command::StartRun { request, target } => {
                assert_eq!(target, Path::new("simple-get.bru"));
                assert_eq!(
                    request.targets,
                    vec![std::path::PathBuf::from("simple-get.bru")]
                );
                assert!(!request.recursive);
            }
            other => panic!("StartRun attendu, obtenu {other:?}"),
        }
        assert!(model.run.active.is_none());
        assert!(model.run.last_failure.is_none());
    }

    #[test]
    fn run_selected_on_a_folder_is_recursive() {
        let mut model = loaded_model((100, 30));
        select(&mut model, "grp");
        match update(&mut model, Message::RunSelected) {
            Command::StartRun { request, target } => {
                assert_eq!(target, Path::new("grp"));
                assert_eq!(request.targets, vec![std::path::PathBuf::from("grp")]);
                assert!(request.recursive);
            }
            other => panic!("StartRun attendu, obtenu {other:?}"),
        }
    }

    #[test]
    fn run_selected_on_an_error_node_does_nothing() {
        let mut model = loaded_model((100, 30));
        select(&mut model, "broken.bru");
        assert!(matches!(
            update(&mut model, Message::RunSelected),
            Command::None
        ));
        assert!(model.run.active.is_none());
        assert!(model.run.outcomes.is_empty());
    }

    #[test]
    fn run_selected_does_nothing_while_a_run_is_active() {
        let mut model = loaded_model((100, 30));
        select(&mut model, "simple-get.bru");
        model.run.active = Some(ActiveRun {
            id: runner::RunId(1),
            target: "post-json.bru".into(),
            recursive: false,
            handle: None,
        });
        assert!(matches!(
            update(&mut model, Message::RunSelected),
            Command::None
        ));
        assert!(model.run.active.is_some());
    }

    #[test]
    fn cancel_run_without_an_active_run_does_nothing() {
        let mut model = loaded_model((100, 30));
        assert!(matches!(
            update(&mut model, Message::CancelRun),
            Command::None
        ));
        assert!(model.run.active.is_none());
    }

    #[test]
    fn run_finished_completed_distributes_results_by_filename() {
        let mut model = loaded_model((100, 30));
        model.run.active = Some(ActiveRun {
            id: runner::RunId(7),
            target: "grp".into(),
            recursive: true,
            handle: None,
        });
        let report = sample_report(&["grp/x.bru", "grp/y.bru"]);
        let command = update(
            &mut model,
            Message::RunFinished(runner::RunEvent {
                id: runner::RunId(7),
                outcome: runner::RunOutcome::Completed {
                    report,
                    exit_code: Some(0),
                },
            }),
        );
        assert!(matches!(command, Command::None));
        assert!(model.run.active.is_none());
        assert!(model.run.last_failure.is_none());
        assert_eq!(model.run.outcomes.len(), 2);
        assert!(model.run.outcomes.contains_key(Path::new("grp/x.bru")));
        assert!(model.run.outcomes.contains_key(Path::new("grp/y.bru")));
    }

    #[test]
    fn run_finished_failed_sets_last_failure_and_clears_active() {
        let mut model = loaded_model((100, 30));
        model.run.active = Some(ActiveRun {
            id: runner::RunId(3),
            target: "simple-get.bru".into(),
            recursive: false,
            handle: None,
        });
        let command = update(
            &mut model,
            Message::RunFinished(runner::RunEvent {
                id: runner::RunId(3),
                outcome: runner::RunOutcome::Failed(runner::RunError::BruNotFound),
            }),
        );
        assert!(matches!(command, Command::None));
        assert!(model.run.active.is_none());
        match model.run.last_failure {
            Some(RunFailure::Error {
                ref target,
                error: runner::RunError::BruNotFound,
            }) => {
                assert_eq!(target, Path::new("simple-get.bru"));
            }
            other => panic!("échec attendu : {other:?}"),
        }
    }

    #[test]
    fn run_finished_with_mismatched_id_is_ignored() {
        let mut model = loaded_model((100, 30));
        model.run.active = Some(ActiveRun {
            id: runner::RunId(1),
            target: "simple-get.bru".into(),
            recursive: false,
            handle: None,
        });
        let command = update(
            &mut model,
            Message::RunFinished(runner::RunEvent {
                id: runner::RunId(2),
                outcome: runner::RunOutcome::Cancelled,
            }),
        );
        assert!(matches!(command, Command::None));
        let active = model
            .run
            .active
            .as_ref()
            .expect("exécution toujours active");
        assert_eq!(active.id, runner::RunId(1));
    }

    // --- Recherche ---------------------------------------------------

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

    fn plain_line_index_containing(model: &Model, needle: &str) -> u16 {
        crate::app::view::detail::plain_lines(model)
            .iter()
            .position(|line| line.contains(needle))
            .expect("ligne attendue") as u16
    }

    #[test]
    fn detail_search_finds_a_match_beyond_the_top_of_the_panel() {
        // Panneau court : le détail de `scripted` dépasse ses 8 lignes
        // utiles (déjà vérifié par `detail_scroll_is_bounded_and_reset`),
        // ce qui force un vrai défilement pour rendre la ligne visible.
        let mut model = loaded_model((100, 12));
        select(&mut model, "scripted.bru");
        update(&mut model, Message::NextFocus);
        assert_eq!(model.focus, Focus::Detail);
        let expected_line = plain_line_index_containing(&model, "res.body.ok: isTrue");

        update(&mut model, Message::StartSearch);
        for c in "res.body.ok".chars() {
            update(&mut model, Message::SearchInput(c));
        }
        update(&mut model, Message::ConfirmSearch);

        let areas = crate::app::view::layout_for(model.size).expect("taille suffisante");
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

    // --- Sélection visuelle -------------------------------------------

    #[test]
    fn toggle_visual_then_escape_does_not_change_scroll_or_copy() {
        let mut model = loaded_model((100, 30));
        select(&mut model, "scripted.bru");
        update(&mut model, Message::NextFocus);
        let scroll_before = model.detail_scroll;
        update(&mut model, Message::ToggleVisual);
        assert!(model.detail_selection.is_some());
        update(&mut model, Message::FocusTree);
        assert!(model.detail_selection.is_none());
        assert_eq!(model.detail_scroll, scroll_before);
        // Le premier Échap annule seulement la sélection : le focus reste
        // sur le détail.
        assert_eq!(model.focus, Focus::Detail);
    }

    #[test]
    fn a_second_toggle_visual_cancels_the_selection_without_copying() {
        let mut model = loaded_model((100, 30));
        select(&mut model, "scripted.bru");
        update(&mut model, Message::NextFocus);
        let scroll_before = model.detail_scroll;
        update(&mut model, Message::ToggleVisual);
        assert!(model.detail_selection.is_some());
        update(&mut model, Message::ToggleVisual);
        assert!(model.detail_selection.is_none());
        assert_eq!(model.detail_scroll, scroll_before);
    }

    #[test]
    fn toggle_visual_extends_to_the_true_last_line_on_end() {
        let mut model = loaded_model((100, 12));
        select(&mut model, "scripted.bru");
        update(&mut model, Message::NextFocus);
        update(&mut model, Message::ToggleVisual);
        update(&mut model, Message::End);
        let range = selection_range(&model).expect("sélection active");
        let last_line = (plain_lines(&model).len() - 1) as u16;
        assert_eq!(*range.end(), last_line);
        assert_eq!(*range.start(), 0);
    }

    #[test]
    fn selecting_another_node_clears_the_visual_selection() {
        let mut model = loaded_model((100, 30));
        select(&mut model, "scripted.bru");
        update(&mut model, Message::NextFocus); // Tab : focus Détail
        update(&mut model, Message::ToggleVisual);
        assert!(model.detail_selection.is_some());
        update(&mut model, Message::NextFocus); // Tab : focus Arbre, sélection intacte
        assert!(model.detail_selection.is_some());
        update(&mut model, Message::Down); // change le nœud sélectionné
        assert!(model.detail_selection.is_none());
    }

    // --- Copie -----------------------------------------------------------

    #[test]
    fn yank_without_selection_copies_the_top_line() {
        let mut model = loaded_model((100, 30));
        select(&mut model, "simple-get.bru");
        update(&mut model, Message::NextFocus);
        let expected = plain_lines(&model)[0].clone();
        match update(&mut model, Message::Yank) {
            Command::CopyToClipboard { token, text } => {
                assert_eq!(text, expected);
                assert_eq!(model.pending_clipboard_token, Some(token));
            }
            other => panic!("CopyToClipboard attendu, obtenu {other:?}"),
        }
    }

    #[test]
    fn yank_with_selection_joins_lines_and_clears_selection() {
        let mut model = loaded_model((100, 12));
        select(&mut model, "scripted.bru");
        update(&mut model, Message::NextFocus);
        update(&mut model, Message::ToggleVisual);
        update(&mut model, Message::End);
        let expected = plain_lines(&model).join("\n");
        match update(&mut model, Message::Yank) {
            Command::CopyToClipboard { text, .. } => assert_eq!(text, expected),
            other => panic!("CopyToClipboard attendu, obtenu {other:?}"),
        }
        assert!(model.detail_selection.is_none());
    }

    #[test]
    fn yank_outside_detail_focus_does_nothing() {
        let mut model = loaded_model((100, 30));
        assert!(matches!(update(&mut model, Message::Yank), Command::None));
    }

    #[test]
    fn clipboard_success_and_failure_update_the_status() {
        let mut model = loaded_model((100, 30));
        select(&mut model, "simple-get.bru");
        update(&mut model, Message::NextFocus);
        let Command::CopyToClipboard { token, .. } = update(&mut model, Message::Yank) else {
            panic!("CopyToClipboard attendu");
        };
        update(
            &mut model,
            Message::ClipboardResult {
                token,
                result: Ok(()),
            },
        );
        assert!(matches!(model.last_status, Some(StatusMessage::Copied)));

        let Command::CopyToClipboard { token, .. } = update(&mut model, Message::Yank) else {
            panic!("CopyToClipboard attendu");
        };
        update(
            &mut model,
            Message::ClipboardResult {
                token,
                result: Err(crate::app::clipboard::test_support::error_for_test()),
            },
        );
        assert!(matches!(
            model.last_status,
            Some(StatusMessage::ClipboardError(_))
        ));
    }

    #[test]
    fn a_stale_clipboard_result_is_ignored() {
        let mut model = loaded_model((100, 30));
        select(&mut model, "simple-get.bru");
        update(&mut model, Message::NextFocus);
        let Command::CopyToClipboard { token: first, .. } = update(&mut model, Message::Yank)
        else {
            panic!("CopyToClipboard attendu");
        };
        let Command::CopyToClipboard { .. } = update(&mut model, Message::Yank) else {
            panic!("CopyToClipboard attendu");
        };
        // Le résultat de la première copie arrive après la seconde : il
        // doit être ignoré, `pending_clipboard_token` pointe déjà sur la
        // seconde.
        update(
            &mut model,
            Message::ClipboardResult {
                token: first,
                result: Ok(()),
            },
        );
        assert!(model.last_status.is_none());
        assert!(model.pending_clipboard_token.is_some());
    }
}
