use std::path::Path;

use super::scroll::{body_height, reset_response_tab_view_state};
use super::{
    clear_detail_view_state, clear_tree_filter, current_tree_filter_pattern, refresh_rows,
    reset_filter_if_selection_changed, scroll_tree_into_view,
};
use crate::app::message::Message;
use crate::app::model::{
    CampaignComparison, CampaignView, Focus, Model, StatusMessage, all_rows, tree_node_at,
};
use crate::app::tree_filter::{find_node_by_path, matches_tree_filter};
use crate::collection::TreeNode;

pub(crate) fn navigate_diagnostics(model: &mut Model, message: Message) {
    let Some(collection) = model.loaded() else {
        return;
    };
    let entries = crate::app::diagnostics::diagnostics(collection);
    let count = entries.len();
    if count == 0 {
        model.diagnostics_selected = 0;
        return;
    }
    let page = body_height(model);
    let before = model.diagnostics_selected.min(count - 1);
    match message {
        Message::Up => model.diagnostics_selected = before.saturating_sub(1),
        Message::Down => model.diagnostics_selected = (before + 1).min(count - 1),
        Message::Home => model.diagnostics_selected = 0,
        Message::End => model.diagnostics_selected = count - 1,
        Message::PageUp => model.diagnostics_selected = before.saturating_sub(page),
        Message::PageDown => model.diagnostics_selected = (before + page).min(count - 1),
        Message::Right => {
            if let Some(entry) = entries.get(before) {
                let address = entry.address.clone();
                cross_navigate_to_tree(model, &address);
            }
        }
        _ => {}
    }
}

pub(crate) fn navigate_history(model: &mut Model, message: Message) {
    let count = model.history.len();
    if count == 0 {
        model.history_selected = 0;
        return;
    }
    let page = body_height(model);
    let before = model.history_selected.min(count - 1);
    match message {
        Message::Up => model.history_selected = before.saturating_sub(1),
        Message::Down => model.history_selected = (before + 1).min(count - 1),
        Message::Home => model.history_selected = 0,
        Message::End => model.history_selected = count - 1,
        Message::PageUp => model.history_selected = before.saturating_sub(page),
        Message::PageDown => model.history_selected = (before + page).min(count - 1),
        _ => {}
    }
}

/// Navigation croisée depuis le panneau de diagnostics vers l'arbre :
/// déplie les dossiers ancêtres (et le nœud lui-même si dossier), sélectionne
/// le nœud et repasse le focus à l'arbre.
pub(crate) fn cross_navigate_to_tree(model: &mut Model, address: &[usize]) {
    for prefix_len in 1..=address.len() {
        let prefix = &address[..prefix_len];
        if let Some(TreeNode::Folder(folder)) = model.node_at(prefix) {
            model.tree.expanded.insert(folder.path.clone());
        }
    }
    crate::app::workspace::open_active(model);
    refresh_rows(model);
    if let Some(index) = model.active_row_index(address) {
        model.tree.selected = index;
        model.selected_collection = false;
    }
    clear_detail_view_state(model);
    scroll_tree_into_view(model);
    model.focus = Focus::Tree;
}

/// Tableau comparatif, s'il existe et que la vue Comparaison est active.
pub(crate) fn comparison_shown_mut(model: &mut Model) -> Option<&mut CampaignComparison> {
    if model.campaign_view == CampaignView::Comparison {
        model.comparison.as_mut()
    } else {
        None
    }
}

/// `t` en focus Campagne : bascule entre la liste et le tableau comparatif,
/// seulement si la dernière campagne porte sur plusieurs environnements.
pub(crate) fn toggle_campaign_view(model: &mut Model) {
    if model.focus != Focus::Campaign || model.comparison.is_none() {
        return;
    }
    model.campaign_view = match model.campaign_view {
        CampaignView::List => CampaignView::Comparison,
        CampaignView::Comparison => CampaignView::List,
    };
    model.campaign_selected = 0;
}

/// Navigation dans le tableau comparatif : `↑`/`↓` changent de requête,
/// `←`/`→` d'environnement, `Entrée` affiche le résultat de la cellule.
pub(crate) fn navigate_comparison(model: &mut Model, message: Message) {
    let Some(comparison) = &model.comparison else {
        return;
    };
    let count = comparison.filtered_rows().len();
    let columns = comparison.environments.len();
    let page = body_height(model);
    let row = model.campaign_selected.min(count.saturating_sub(1));
    let column = model.comparison_column.min(columns.saturating_sub(1));
    match message {
        Message::Up => model.campaign_selected = row.saturating_sub(1),
        Message::Down => model.campaign_selected = (row + 1).min(count.saturating_sub(1)),
        Message::Home => model.campaign_selected = 0,
        Message::End => model.campaign_selected = count.saturating_sub(1),
        Message::PageUp => model.campaign_selected = row.saturating_sub(page),
        Message::PageDown => {
            model.campaign_selected = (row + page).min(count.saturating_sub(1));
        }
        Message::Left => model.comparison_column = column.saturating_sub(1),
        Message::Right => model.comparison_column = (column + 1).min(columns.saturating_sub(1)),
        Message::Enter => show_comparison_cell(model, row, column),
        _ => {}
    }
}

/// Sélectionne la requête de la ligne dans l'arbre et affiche, dans le
/// panneau Réponse, son résultat pour l'environnement de la colonne.
pub(crate) fn show_comparison_cell(model: &mut Model, row: usize, column: usize) {
    let Some(comparison) = &model.comparison else {
        return;
    };
    let Some(path) = comparison.filtered_rows().get(row).map(|r| r.path.clone()) else {
        return;
    };
    let Some(environment) = comparison.environments.get(column).cloned() else {
        return;
    };
    let Some(outcome) = comparison
        .outcomes
        .get(column)
        .and_then(|outcomes| outcomes.get(&path))
        .cloned()
    else {
        model.last_status = Some(StatusMessage::NoEnvironmentResult(environment));
        return;
    };
    select_request_in_tree(model, &path);
    model.run.outcomes.insert(path, outcome);
    reset_response_tab_view_state(model);
    model.focus = Focus::Response;
}

pub(crate) fn navigate_campaign(model: &mut Model, message: Message) {
    if model.campaign_view == CampaignView::Comparison && model.comparison.is_some() {
        navigate_comparison(model, message);
        return;
    }
    let Some(campaign) = &model.campaign else {
        model.campaign_selected = 0;
        return;
    };
    let filtered = campaign.filtered_items();
    let count = filtered.len();
    if count == 0 {
        model.campaign_selected = 0;
        return;
    }
    let page = body_height(model);
    let before = model.campaign_selected.min(count - 1);
    match message {
        Message::Up => model.campaign_selected = before.saturating_sub(1),
        Message::Down => model.campaign_selected = (before + 1).min(count - 1),
        Message::Home => model.campaign_selected = 0,
        Message::End => model.campaign_selected = count - 1,
        Message::PageUp => model.campaign_selected = before.saturating_sub(page),
        Message::PageDown => model.campaign_selected = (before + page).min(count - 1),
        Message::Right | Message::Enter => {
            if let Some(item) = filtered.get(before) {
                let target_path = item.path.clone();
                select_request_and_focus_detail(model, &target_path);
            }
        }
        _ => {}
    }
}

/// Sélectionne une requête dans l'arbre en dépliant ses dossiers parents
/// et donne le focus au panneau Détail.
pub(crate) fn select_request_and_focus_detail(model: &mut Model, target_path: &Path) {
    select_request_in_tree(model, target_path);
    model.focus = Focus::Detail;
}

/// Sélectionne une requête dans l'arbre en dépliant ses dossiers parents.
pub(crate) fn select_request_in_tree(model: &mut Model, target_path: &Path) {
    if let Some(pattern) = current_tree_filter_pattern(model) {
        let is_target_visible = model.loaded().is_some_and(|collection| {
            find_node_by_path(&collection.tree, target_path)
                .is_some_and(|node| matches_tree_filter(node, pattern))
        });
        if !is_target_visible {
            clear_tree_filter(model);
        }
    }
    for ancestor in target_path.ancestors().skip(1) {
        if !ancestor.as_os_str().is_empty() {
            model.tree.expanded.insert(ancestor.to_path_buf());
        }
    }
    crate::app::workspace::open_active(model);
    refresh_rows(model);
    let previous_path = model.selected_node().map(|node| node.path().to_path_buf());
    if let Some(index) = model.active_row_position(|node| node.path() == target_path) {
        model.tree.selected = index;
        if model.workspace.is_some() {
            model.selected_collection = false;
        }
    }
    clear_detail_view_state(model);
    reset_filter_if_selection_changed(model, previous_path.as_deref());
    scroll_tree_into_view(model);
}

/// Navigue vers la requête en échec suivante (`forward = true`) ou précédente
/// (`forward = false`) dans l'ordre de l'arbre.
pub(crate) fn navigate_failed_request(model: &mut Model, forward: bool) {
    if !matches!(model.focus, Focus::Tree | Focus::Detail | Focus::Response) {
        return;
    }
    let Some(collection) = model.loaded() else {
        return;
    };
    let all = all_rows(&collection.tree);
    let mut failed_requests = Vec::new();
    for (all_idx, row) in all.iter().enumerate() {
        if let Some(TreeNode::Request(req)) = tree_node_at(&collection.tree, &row.address)
            && model
                .run
                .outcomes
                .get(&req.path)
                .is_some_and(|o| o.result.is_failure())
        {
            failed_requests.push((all_idx, req.path.clone()));
        }
    }

    if failed_requests.is_empty() {
        model.last_status = Some(StatusMessage::NoFailedRequests);
        return;
    }

    let current_addr = model
        .tree
        .rows
        .get(model.tree.selected)
        .filter(|r| model.is_active_row(r))
        .map(|r| &r.address);
    let current_all_idx = current_addr
        .and_then(|addr| all.iter().position(|r| &r.address == addr))
        .unwrap_or(0);

    let target_path = if forward {
        failed_requests
            .iter()
            .find(|(all_idx, _)| *all_idx > current_all_idx)
            .map(|(_, path)| path)
            .unwrap_or(&failed_requests[0].1)
    } else {
        failed_requests
            .iter()
            .rfind(|(all_idx, _)| *all_idx < current_all_idx)
            .map(|(_, path)| path)
            .unwrap_or(&failed_requests[failed_requests.len() - 1].1)
    };

    let target_path = target_path.clone();
    select_request_in_tree(model, &target_path);
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::super::test_support::*;
    use super::super::*;
    use crate::app::model::{
        CampaignFailure, CampaignFilter, CampaignItem, CampaignItemStatus, CampaignSummary,
        HistoryEntry, HistoryOutcome, StatusMessage,
    };
    use crate::app::test_support::{loaded_model, selected_name};
    use crate::collection::LoadError;
    use crate::runner;

    #[test]
    fn toggle_diagnostics_and_history() {
        let mut model = loaded_model((100, 30));
        assert_eq!(model.focus, Focus::Tree);

        // Ouverture puis fermeture diagnostics
        update(&mut model, Message::ToggleDiagnostics);
        assert_eq!(model.focus, Focus::Diagnostics);
        update(&mut model, Message::ToggleDiagnostics);
        assert_eq!(model.focus, Focus::Tree);

        // Ouverture puis fermeture history
        update(&mut model, Message::ToggleHistory);
        assert_eq!(model.focus, Focus::History);
        update(&mut model, Message::ToggleHistory);
        assert_eq!(model.focus, Focus::Tree);

        // Bascule directe entre les deux
        update(&mut model, Message::ToggleDiagnostics);
        assert_eq!(model.focus, Focus::Diagnostics);
        update(&mut model, Message::ToggleHistory);
        assert_eq!(model.focus, Focus::History);

        // Échap referme le panneau vers l'arbre
        update(&mut model, Message::FocusTree);
        assert_eq!(model.focus, Focus::Tree);

        // Sans collection chargée (Loading ou Failed) : aucun effet
        let mut loading_model = Model::new("/x".into(), (100, 30));
        update(&mut loading_model, Message::ToggleDiagnostics);
        assert_eq!(loading_model.focus, Focus::Tree);
        update(&mut loading_model, Message::ToggleHistory);
        assert_eq!(loading_model.focus, Focus::Tree);

        let mut failed_model = Model::new("/x".into(), (100, 30));
        update(
            &mut failed_model,
            Message::CollectionLoaded(Err(LoadError::NotACollection { path: "/x".into() })),
        );
        update(&mut failed_model, Message::ToggleDiagnostics);
        assert_eq!(failed_model.focus, Focus::Tree);
        update(&mut failed_model, Message::ToggleHistory);
        assert_eq!(failed_model.focus, Focus::Tree);
    }

    #[test]
    fn diagnostics_and_history_navigation() {
        // parser-cases a 3 erreurs : badmeta, broken.bru, no-method.bru
        let mut model = loaded_model((100, 12));
        model.focus = Focus::Diagnostics;
        assert_eq!(model.diagnostics_selected, 0);

        // Up à 0 reste à 0
        update(&mut model, Message::Up);
        assert_eq!(model.diagnostics_selected, 0);

        // Down incrémente
        update(&mut model, Message::Down);
        assert_eq!(model.diagnostics_selected, 1);
        update(&mut model, Message::Down);
        assert_eq!(model.diagnostics_selected, 2);

        // Down au-delà de la fin reste borné
        update(&mut model, Message::Down);
        assert_eq!(model.diagnostics_selected, 2);

        // Home retourne à 0
        update(&mut model, Message::Home);
        assert_eq!(model.diagnostics_selected, 0);

        // End va au dernier
        update(&mut model, Message::End);
        assert_eq!(model.diagnostics_selected, 2);

        // PageUp et PageDown
        update(&mut model, Message::PageUp);
        assert_eq!(model.diagnostics_selected, 0);
        update(&mut model, Message::PageDown);
        assert_eq!(model.diagnostics_selected, 2);

        // De même pour History
        model.focus = Focus::History;
        for i in 0..3 {
            model.history.push_back(HistoryEntry {
                started_at: std::time::SystemTime::now(),
                target: std::path::PathBuf::from(format!("req{i}.bru")),
                recursive: false,
                outcome: HistoryOutcome::Completed {
                    total: 1,
                    failed: 0,
                    duration_secs: 0.1,
                },
            });
        }
        assert_eq!(model.history_selected, 0);
        update(&mut model, Message::Up);
        assert_eq!(model.history_selected, 0);
        update(&mut model, Message::Down);
        assert_eq!(model.history_selected, 1);
        update(&mut model, Message::End);
        assert_eq!(model.history_selected, 2);
        update(&mut model, Message::Down);
        assert_eq!(model.history_selected, 2);
        update(&mut model, Message::Home);
        assert_eq!(model.history_selected, 0);
        update(&mut model, Message::PageDown);
        assert_eq!(model.history_selected, 2);
        update(&mut model, Message::PageUp);
        assert_eq!(model.history_selected, 0);
    }

    #[test]
    fn cross_navigation_from_diagnostics_to_tree() {
        let mut model = loaded_model((100, 30));
        model.focus = Focus::Diagnostics;

        // Entrée 0 : badmeta (dossier)
        model.diagnostics_selected = 0;
        assert!(!model.tree.expanded.contains(Path::new("badmeta")));
        update(&mut model, Message::Right);
        assert_eq!(model.focus, Focus::Tree);
        assert!(model.tree.expanded.contains(Path::new("badmeta")));
        assert_eq!(selected_name(&model), "badmeta");
        assert_eq!(model.selected_node().unwrap().path(), Path::new("badmeta"));

        // Entrée 1 : broken.bru (fichier à la racine)
        model.focus = Focus::Diagnostics;
        model.diagnostics_selected = 1;
        update(&mut model, Message::Right);
        assert_eq!(model.focus, Focus::Tree);
        assert_eq!(selected_name(&model), "broken.bru");
        assert_eq!(
            model.selected_node().unwrap().path(),
            Path::new("broken.bru")
        );
    }

    #[test]
    fn campaign_navigation_and_cross_selection() {
        let mut model = loaded_model((100, 30));
        model.campaign = Some(CampaignSummary {
            target: PathBuf::from("grp"),
            total: 2,
            passed: 0,
            failed: 2,
            skipped: 0,
            duration_secs: 0.3,
            items: vec![
                CampaignItem {
                    path: PathBuf::from("grp/x.bru"),
                    name: "x".into(),
                    status: CampaignItemStatus::Failed,
                    http_code: Some(500),
                    duration_secs: 0.1,
                    reason: Some("server error".into()),
                    iteration_index: 0,
                },
                CampaignItem {
                    path: PathBuf::from("grp/sub/deep.bru"),
                    name: "deep".into(),
                    status: CampaignItemStatus::Failed,
                    http_code: Some(400),
                    duration_secs: 0.2,
                    reason: Some("bad request".into()),
                    iteration_index: 0,
                },
            ],
            filter: CampaignFilter::default(),
            failures: vec![
                CampaignFailure {
                    path: PathBuf::from("grp/x.bru"),
                    name: "x".into(),
                    http_code: Some(500),
                    reason: "server error".into(),
                },
                CampaignFailure {
                    path: PathBuf::from("grp/sub/deep.bru"),
                    name: "deep".into(),
                    http_code: Some(400),
                    reason: "bad request".into(),
                },
            ],
            total_iterations: 1,
        });
        model.focus = Focus::Campaign;
        assert_eq!(model.campaign_selected, 0);

        // Navigation
        update(&mut model, Message::Down);
        assert_eq!(model.campaign_selected, 1);
        update(&mut model, Message::Down);
        assert_eq!(model.campaign_selected, 1);
        update(&mut model, Message::Up);
        assert_eq!(model.campaign_selected, 0);
        update(&mut model, Message::End);
        assert_eq!(model.campaign_selected, 1);
        update(&mut model, Message::Home);
        assert_eq!(model.campaign_selected, 0);

        // Sélection croisée vers Détail via Right
        assert!(!model.tree.expanded.contains(Path::new("grp")));
        update(&mut model, Message::Right);
        assert_eq!(model.focus, Focus::Detail);
        assert!(model.tree.expanded.contains(Path::new("grp")));
        assert_eq!(
            model.selected_node().unwrap().path(),
            Path::new("grp/x.bru")
        );

        // Sélection croisée vers Détail via Enter
        model.focus = Focus::Campaign;
        model.campaign_selected = 1;
        assert!(!model.tree.expanded.contains(Path::new("grp/sub")));
        update(&mut model, Message::Enter);
        assert_eq!(model.focus, Focus::Detail);
        assert!(model.tree.expanded.contains(Path::new("grp/sub")));
        assert_eq!(
            model.selected_node().unwrap().path(),
            Path::new("grp/sub/deep.bru")
        );
    }

    #[test]
    fn next_and_previous_failed_request_keys() {
        let mut model = loaded_model((100, 30));

        // 1. Sans aucune requête en échec : statut "Aucune requête en échec"
        update(&mut model, Message::NextFailedRequest);
        assert!(matches!(
            model.last_status,
            Some(StatusMessage::NoFailedRequests)
        ));

        model.last_status = None;
        update(&mut model, Message::PreviousFailedRequest);
        assert!(matches!(
            model.last_status,
            Some(StatusMessage::NoFailedRequests)
        ));

        // 2. Avec deux requêtes en échec dans l'arbre : grp/x.bru et grp/sub/deep.bru
        let mut res_fail1 = sample_request_result("grp/x.bru");
        res_fail1.error = Some("fail 1".into());
        res_fail1.status = runner::report::ResultStatus::Error;
        model.run.outcomes.insert(
            PathBuf::from("grp/x.bru"),
            crate::app::model::RequestOutcome::new(res_fail1, Some(1)),
        );

        let mut res_fail2 = sample_request_result("grp/sub/deep.bru");
        res_fail2.error = Some("fail 2".into());
        res_fail2.status = runner::report::ResultStatus::Error;
        model.run.outcomes.insert(
            PathBuf::from("grp/sub/deep.bru"),
            crate::app::model::RequestOutcome::new(res_fail2, Some(1)),
        );

        // Positionné au début de l'arbre (dossier grp)
        model.focus = Focus::Tree;
        model.tree.selected = 0;

        // ']' sélectionne le premier échec (grp/x.bru) et déplie grp
        update(&mut model, Message::NextFailedRequest);
        assert_eq!(
            model.selected_node().unwrap().path(),
            Path::new("grp/x.bru")
        );
        assert!(model.tree.expanded.contains(Path::new("grp")));
        assert_eq!(model.focus, Focus::Tree);

        // ']' suivant sélectionne le deuxième échec (grp/sub/deep.bru) et déplie sub
        update(&mut model, Message::NextFailedRequest);
        assert_eq!(
            model.selected_node().unwrap().path(),
            Path::new("grp/sub/deep.bru")
        );
        assert!(model.tree.expanded.contains(Path::new("grp/sub")));

        // ']' suivant boucle au premier échec
        update(&mut model, Message::NextFailedRequest);
        assert_eq!(
            model.selected_node().unwrap().path(),
            Path::new("grp/x.bru")
        );

        // '[' recule et boucle sur le dernier échec
        update(&mut model, Message::PreviousFailedRequest);
        assert_eq!(
            model.selected_node().unwrap().path(),
            Path::new("grp/sub/deep.bru")
        );

        // '[' précédent revient à grp/x.bru
        update(&mut model, Message::PreviousFailedRequest);
        assert_eq!(
            model.selected_node().unwrap().path(),
            Path::new("grp/x.bru")
        );

        // Fonctionne aussi depuis Focus::Detail et Focus::Response
        model.focus = Focus::Detail;
        update(&mut model, Message::NextFailedRequest);
        assert_eq!(
            model.selected_node().unwrap().path(),
            Path::new("grp/sub/deep.bru")
        );
        assert_eq!(model.focus, Focus::Detail);

        model.focus = Focus::Response;
        update(&mut model, Message::NextFailedRequest);
        assert_eq!(
            model.selected_node().unwrap().path(),
            Path::new("grp/x.bru")
        );
        assert_eq!(model.focus, Focus::Response);

        // Ignoré depuis un autre panneau (ex: Diagnostics)
        model.focus = Focus::Diagnostics;
        update(&mut model, Message::NextFailedRequest);
        assert_eq!(
            model.selected_node().unwrap().path(),
            Path::new("grp/x.bru")
        );
        assert_eq!(model.focus, Focus::Diagnostics);
    }
}
