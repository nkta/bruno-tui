//! Transitions du modèle.
//!
//! `update` est la seule fonction qui modifie le modèle. Elle ne fait
//! aucune I/O : le chargement et le rendu sont pilotés par la boucle.

use std::ops::RangeInclusive;

use super::filter::{FilterState, evaluate};
use super::message::Message;
use super::model::{
    CollectionState, DetailSelection, EditMode, EditSession, EditableField, Exit, Focus,
    HistoryEntry, HistoryOutcome, Model, ResponseTab, SecretError, SecretInput, StatusMessage,
    environment_name_at, field_enabled, field_value, secret_env_vars, secret_lookups, secret_rows,
    tree_node_at, visible_rows,
};
use super::search::{SearchScope, SearchState, find_detail_match, find_tree_match};
use super::view::detail::{plain_lines, response_plain_lines};
use super::view::{
    detail::{detail_text, response_text},
    inner, layout_for,
};
use crate::collection::TreeNode;
use crate::runner::report::ResponseStatus;
use crate::runner::{RunRequest, SecretString};
use crate::secrets::{SecretLookup, is_valid_name};
use crate::writer::{FieldEdit, FileStamp};

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
    /// Sauvegarder les modifications sur disque ; la boucle l'exécute
    /// dans `spawn_blocking` via `RequestWriter::write_request` et renvoie
    /// le résultat à `update` via `Message::EditSaved`.
    SaveEdit {
        path: std::path::PathBuf,
        ast: crate::collection::BruFile,
        stamp: crate::writer::FileStamp,
        edits: Vec<crate::writer::FieldEdit>,
    },
    /// Résoudre les variables secrètes depuis `<root>/.env` et le shell ;
    /// la boucle l'exécute dans `spawn_blocking` et renvoie le résultat à
    /// `update` via `Message::SecretsResolved`.
    ResolveSecrets {
        root: std::path::PathBuf,
        lookups: Vec<SecretLookup>,
    },
}

/// Applique un message au modèle.
pub fn update(model: &mut Model, message: Message) -> Command {
    if let Some(confirm) = model.confirm {
        return match message {
            Message::ConfirmYes | Message::Yank | Message::Right => {
                model.confirm = None;
                match confirm {
                    super::model::PendingConfirm::DiscardEdit => {
                        model.editing = None;
                    }
                    super::model::PendingConfirm::QuitWithUnsavedEdit => {
                        model.exit = Some(Exit::Normal);
                    }
                }
                Command::None
            }
            Message::ConfirmNo | Message::NextMatch | Message::FocusTree => {
                model.confirm = None;
                Command::None
            }
            Message::ForceQuit => {
                model.confirm = None;
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
            _ => Command::None,
        };
    }

    match message {
        Message::Quit | Message::ForceQuit => {
            if model.editing.as_ref().is_some_and(|s| s.dirty) {
                model.confirm = Some(super::model::PendingConfirm::QuitWithUnsavedEdit);
            } else {
                model.exit = Some(Exit::Normal);
            }
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
            let command = match &result {
                Ok(collection) => {
                    let lookups = secret_lookups(&model.secrets.mappings, collection);
                    if lookups.is_empty() {
                        Command::None
                    } else {
                        Command::ResolveSecrets {
                            root: collection.root.clone(),
                            lookups,
                        }
                    }
                }
                Err(_) => Command::None,
            };
            model.collection = match result {
                Ok(collection) => CollectionState::Loaded(collection),
                Err(error) => CollectionState::Failed(error),
            };
            reset_secrets_for_collection(model);
            model.tree.expanded.clear();
            refresh_rows(model);
            model.tree.selected = 0;
            model.tree.offset = 0;
            model.detail_scroll = 0;
            model.current_environment = None;
            if matches!(model.focus, Focus::EnvironmentPicker | Focus::Secrets) {
                model.focus = Focus::Tree;
            }
            command
        }
        Message::SecretsResolved { root, resolved } => {
            // Résultat d'une collection qui n'est plus chargée : ignoré.
            if model.loaded().is_some_and(|c| c.root == root) {
                model.secrets.resolved = resolved;
            }
            Command::None
        }
        Message::ToggleSecrets => {
            match model.focus {
                Focus::Secrets => close_secrets(model),
                _ => {
                    if model.loaded().is_some() {
                        model.secrets.pending_run = None;
                        model.secrets.selected = 0;
                        model.secrets.error = None;
                        model.focus = Focus::Secrets;
                    }
                }
            }
            Command::None
        }
        Message::AddSecret => {
            if model.focus == Focus::Secrets {
                model.secrets.error = None;
                model.secrets.input = Some(SecretInput::Name(String::new()));
            }
            Command::None
        }
        Message::ForgetSecret => {
            if model.focus == Focus::Secrets {
                forget_secret(model);
            }
            Command::None
        }
        Message::SecretInput(c) => {
            match &mut model.secrets.input {
                Some(SecretInput::Name(name)) => name.push(c.0),
                Some(SecretInput::Value { buffer, .. }) => buffer.push(c.0),
                None => {}
            }
            Command::None
        }
        Message::SecretBackspace => {
            match &mut model.secrets.input {
                Some(SecretInput::Name(name)) => {
                    name.pop();
                }
                Some(SecretInput::Value { buffer, .. }) => buffer.pop(),
                None => {}
            }
            Command::None
        }
        Message::ConfirmSecretInput => {
            confirm_secret_input(model);
            Command::None
        }
        Message::CancelSecretInput => {
            model.secrets.input = None;
            Command::None
        }
        Message::NextFocus => {
            model.focus = match model.focus {
                Focus::Tree => Focus::Detail,
                Focus::Detail => Focus::Response,
                _ => Focus::Tree,
            };
            Command::None
        }
        Message::FocusTree if model.focus == Focus::Secrets => {
            close_secrets(model);
            Command::None
        }
        Message::FocusTree => {
            if let Some(session) = &model.editing {
                if session.dirty {
                    model.confirm = Some(super::model::PendingConfirm::DiscardEdit);
                } else {
                    model.editing = None;
                }
            } else {
                // `Échap` annule d'abord la sélection visuelle du panneau
                // focalisé, s'il y en a une, avant de rendre le focus à
                // l'arbre (`search-and-yank`, généralisé au détail et à
                // la réponse par `split-request-response-panels`).
                let cancelled_selection = match model.focus {
                    Focus::Detail => model.detail_selection.take().is_some(),
                    Focus::Response => model.response_selection.take().is_some(),
                    _ => false,
                };
                if !cancelled_selection {
                    model.focus = Focus::Tree;
                }
            }
            Command::None
        }
        Message::ToggleDiagnostics => {
            if model.loaded().is_some() {
                model.focus = match model.focus {
                    Focus::Diagnostics => Focus::Tree,
                    _ => Focus::Diagnostics,
                };
            }
            Command::None
        }
        Message::ToggleHistory => {
            if model.loaded().is_some() {
                model.focus = match model.focus {
                    Focus::History => Focus::Tree,
                    _ => Focus::History,
                };
            }
            Command::None
        }
        Message::ToggleEnvironmentPicker => {
            match model.focus {
                Focus::EnvironmentPicker => model.focus = Focus::Tree,
                _ => {
                    if let Some(collection) = model.loaded() {
                        let index =
                            environment_index_for(collection, model.current_environment.as_deref());
                        model.environment_selected = index;
                        model.focus = Focus::EnvironmentPicker;
                    }
                }
            }
            Command::None
        }
        Message::RunSelected if model.focus == Focus::Secrets => launch_pending_run(model),
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
        Message::OpenFilter => {
            open_filter(model);
            Command::None
        }
        Message::FilterInput(c) => {
            if let Some(filter) = &mut model.filter
                && filter.editing
            {
                filter.draft.push(c);
            }
            Command::None
        }
        Message::FilterBackspace => {
            if let Some(filter) = &mut model.filter
                && filter.editing
            {
                filter.draft.pop();
            }
            Command::None
        }
        Message::ConfirmFilter => {
            confirm_filter(model);
            Command::None
        }
        Message::CancelFilter => {
            if let Some(filter) = &mut model.filter {
                filter.editing = false;
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
        Message::StartEdit => {
            start_edit(model);
            Command::None
        }
        Message::MoveFieldCursor(delta) => {
            move_field_cursor(model, delta);
            Command::None
        }
        Message::ToggleField => {
            toggle_field(model);
            Command::None
        }
        Message::EnterInsert => {
            enter_insert(model);
            Command::None
        }
        Message::LeaveInsert => {
            leave_insert(model);
            Command::None
        }
        Message::InsertEnter => {
            insert_enter(model);
            Command::None
        }
        Message::InsertChar(c) => {
            insert_char(model, c);
            Command::None
        }
        Message::InsertBackspace => {
            insert_backspace(model);
            Command::None
        }
        Message::InsertCursorLeft => {
            insert_cursor_left(model);
            Command::None
        }
        Message::InsertCursorRight => {
            insert_cursor_right(model);
            Command::None
        }
        Message::SaveEdit => save_edit(model),
        Message::EditSaved { path, result } => {
            edit_saved(model, path, result);
            Command::None
        }
        navigation => {
            match model.focus {
                Focus::Tree => navigate_tree(model, navigation),
                Focus::Detail => {
                    if model
                        .editing
                        .as_ref()
                        .is_some_and(|s| matches!(s.mode, EditMode::Normal))
                    {
                        match navigation {
                            Message::Up => move_field_cursor(model, -1),
                            Message::Down => move_field_cursor(model, 1),
                            other => scroll_detail(model, other),
                        }
                    } else {
                        scroll_detail(model, navigation);
                    }
                }
                Focus::Response => match navigation {
                    Message::Left => previous_response_tab(model),
                    Message::Right => next_response_tab(model),
                    other => scroll_response(model, other),
                },
                Focus::Diagnostics => navigate_diagnostics(model, navigation),
                Focus::History => navigate_history(model, navigation),
                Focus::EnvironmentPicker => navigate_environment_picker(model, navigation),
                Focus::Secrets => navigate_secrets(model, navigation),
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
    let target = match model.focus {
        Focus::History => model
            .history
            .get(model.history_selected)
            .map(|entry| (entry.target.clone(), entry.recursive)),
        Focus::Tree | Focus::Detail | Focus::Response => match model.selected_node() {
            Some(TreeNode::Request(request)) => Some((request.path.clone(), false)),
            Some(TreeNode::Folder(folder)) => Some((folder.path.clone(), true)),
            Some(TreeNode::Error(_)) | None => None,
        },
        Focus::Diagnostics | Focus::EnvironmentPicker | Focus::Secrets => None,
    };
    let Some((target, recursive)) = target else {
        return Command::None;
    };
    if model.loaded().is_none() {
        return Command::None;
    }
    if should_propose_secrets(model) {
        let missing = first_missing_declared_secret(model);
        model.secrets.pending_run = Some((target, recursive));
        model.secrets.selected = missing;
        model.secrets.input = None;
        model.secrets.error = None;
        model.focus = Focus::Secrets;
        return Command::None;
    }
    start_run(model, target, recursive)
}

/// Construit l'exécution sur la cible, avec l'environnement et les
/// variables secrètes en vigueur à cet instant.
fn start_run(model: &mut Model, target: std::path::PathBuf, recursive: bool) -> Command {
    let Some(root) = model.loaded().map(|collection| collection.root.clone()) else {
        return Command::None;
    };
    model.run.last_failure = None;
    Command::StartRun {
        request: RunRequest {
            collection_root: root,
            targets: vec![target.clone()],
            recursive,
            env: model.current_environment.clone(),
            env_vars: secret_env_vars(model),
        },
        target,
    }
}

/// Vrai si l'environnement courant, non encore acquitté, déclare un
/// `vars:secret` resté sans valeur.
fn should_propose_secrets(model: &Model) -> bool {
    let Some(env) = &model.current_environment else {
        return false;
    };
    !model.secrets.acknowledged.contains(env)
        && secret_rows(model)
            .iter()
            .any(|row| row.declared && row.value.is_none())
}

/// Ligne de la première variable déclarée non fournie (0 à défaut).
fn first_missing_declared_secret(model: &Model) -> usize {
    secret_rows(model)
        .iter()
        .position(|row| row.declared && row.value.is_none())
        .unwrap_or(0)
}

/// `r` dans le panneau des variables secrètes : lance l'exécution en
/// attente avec les valeurs alors résolues, et acquitte l'environnement.
fn launch_pending_run(model: &mut Model) -> Command {
    if model.run.active.is_some() {
        return Command::None;
    }
    let Some((target, recursive)) = model.secrets.pending_run.take() else {
        return Command::None;
    };
    acknowledge_environment(model);
    model.secrets.input = None;
    model.focus = Focus::Tree;
    start_run(model, target, recursive)
}

fn acknowledge_environment(model: &mut Model) {
    if let Some(env) = &model.current_environment {
        model.secrets.acknowledged.insert(env.clone());
    }
}

/// `Échap` ou `S` : ferme le panneau ; une exécution en attente est
/// abandonnée et l'environnement acquitté.
fn close_secrets(model: &mut Model) {
    if model.secrets.pending_run.take().is_some() {
        acknowledge_environment(model);
    }
    model.secrets.input = None;
    model.secrets.error = None;
    model.focus = Focus::Tree;
}

/// Un nouveau chargement de collection : les résolutions, propositions et
/// acquittements ne valent que pour la collection précédente. Les valeurs
/// saisies restent en mémoire pour la session.
fn reset_secrets_for_collection(model: &mut Model) {
    let secrets = &mut model.secrets;
    secrets.resolved.clear();
    secrets.pending_run = None;
    secrets.acknowledged.clear();
    secrets.input = None;
    secrets.error = None;
    secrets.selected = 0;
}

/// `↑`/`↓`/`Début`/`Fin`/`Entrée` dans le panneau des variables secrètes.
fn navigate_secrets(model: &mut Model, message: Message) {
    let count = secret_rows(model).len();
    if count == 0 {
        model.secrets.selected = 0;
        return;
    }
    let before = model.secrets.selected.min(count - 1);
    match message {
        Message::Up => model.secrets.selected = before.saturating_sub(1),
        Message::Down => model.secrets.selected = (before + 1).min(count - 1),
        Message::Home => model.secrets.selected = 0,
        Message::End => model.secrets.selected = count - 1,
        Message::Right => {
            if let Some(row) = secret_rows(model).into_iter().nth(before) {
                model.secrets.error = None;
                model.secrets.input = Some(SecretInput::Value {
                    name: row.name,
                    buffer: SecretString::default(),
                    adding: false,
                });
            }
        }
        _ => {}
    }
}

/// `Entrée` pendant une saisie : un nom valide enchaîne sur sa valeur ;
/// une valeur non vide est retenue pour la session, vide l'oublie.
fn confirm_secret_input(model: &mut Model) {
    let Some(input) = model.secrets.input.take() else {
        return;
    };
    match input {
        SecretInput::Name(name) => {
            if !is_valid_name(&name) {
                model.secrets.error = Some(SecretError::InvalidName);
                model.secrets.input = Some(SecretInput::Name(name));
            } else if secret_rows(model).iter().any(|row| row.name == name) {
                model.secrets.error = Some(SecretError::DuplicateName);
                model.secrets.input = Some(SecretInput::Name(name));
            } else {
                model.secrets.error = None;
                model.secrets.input = Some(SecretInput::Value {
                    name,
                    buffer: SecretString::default(),
                    adding: true,
                });
            }
        }
        SecretInput::Value {
            name,
            buffer,
            adding,
        } => {
            let secrets = &mut model.secrets;
            secrets.typed.retain(|(typed, _)| *typed != name);
            if buffer.is_empty() {
                return;
            }
            secrets.typed.push((name.clone(), buffer));
            if adding {
                secrets.added.push(name.clone());
            }
            if let Some(index) = secret_rows(model).iter().position(|row| row.name == name) {
                model.secrets.selected = index;
            }
        }
    }
}

/// `d` : oublie la valeur saisie de la ligne sélectionnée ; un nom ajouté
/// depuis le panneau est aussi retiré.
fn forget_secret(model: &mut Model) {
    let rows = secret_rows(model);
    let Some(row) = rows.get(model.secrets.selected.min(rows.len().saturating_sub(1))) else {
        return;
    };
    let secrets = &mut model.secrets;
    secrets.error = None;
    secrets.typed.retain(|(name, _)| *name != row.name);
    secrets.added.retain(|name| *name != row.name);
    let count = secret_rows(model).len();
    model.secrets.selected = model.secrets.selected.min(count.saturating_sub(1));
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

    let started_at = std::time::SystemTime::now();
    let history_outcome = match &event.outcome {
        crate::runner::RunOutcome::Completed { report, .. } => {
            let results: Vec<_> = report
                .iterations()
                .iter()
                .flat_map(|it| &it.results)
                .collect();
            HistoryOutcome::Completed {
                total: results.len() as u64,
                failed: results.iter().filter(|r| r.is_failure()).count() as u64,
                duration_secs: results.iter().map(|r| r.run_duration).sum(),
            }
        }
        crate::runner::RunOutcome::Failed(error) => HistoryOutcome::Failed(error.to_string()),
        crate::runner::RunOutcome::Cancelled => HistoryOutcome::Cancelled,
    };
    let entry = HistoryEntry {
        started_at,
        target: active.target.clone(),
        recursive: active.recursive,
        outcome: history_outcome,
    };
    if model.history.len() >= super::model::HISTORY_LIMIT {
        model.history.pop_back();
    }
    model.history.push_front(entry);

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
    let previous_path = model.selected_node().map(|node| node.path().to_path_buf());
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
    reset_filter_if_selection_changed(model, previous_path.as_deref());
    scroll_tree_into_view(model);
}

/// Efface l'état propre à l'affichage du détail et de la réponse d'un
/// nœud précis : appelé à chaque changement de sélection dans l'arbre,
/// qu'il vienne de la navigation normale ou d'une recherche.
fn clear_detail_view_state(model: &mut Model) {
    model.detail_scroll = 0;
    model.detail_selection = None;
    model.detail_match = None;
    model.response_scroll = 0;
    model.response_selection = None;
    model.response_match = None;
    model.response_tab = ResponseTab::Body;
    model.editing = None;
}

/// Ouvre une session d'édition sur la requête sélectionnée (D1).
fn start_edit(model: &mut Model) {
    if model.focus != Focus::Detail || model.editing.is_some() {
        return;
    }
    let Some(TreeNode::Request(request)) = model.selected_node() else {
        return;
    };
    let Some(root) = model.loaded().map(|c| &c.root) else {
        return;
    };
    let full_path = root.join(&request.path);
    let Ok(stamp) = FileStamp::capture(&full_path) else {
        return;
    };
    let fields = EditableField::list_for(&request.view);
    model.editing = Some(EditSession {
        path: request.path.clone(),
        stamp,
        fields,
        cursor: 0,
        mode: EditMode::Normal,
        pending: Vec::new(),
        dirty: false,
    });
}

/// Déplace le curseur de champ en mode Normal de session d'édition (D5).
fn move_field_cursor(model: &mut Model, delta: i8) {
    let Some(session) = &mut model.editing else {
        return;
    };
    if !matches!(session.mode, EditMode::Normal) {
        return;
    }
    let count = session.fields.len();
    if count == 0 {
        return;
    }
    if delta < 0 {
        session.cursor = session.cursor.saturating_sub(delta.unsigned_abs() as usize);
    } else if delta > 0 {
        session.cursor = (session.cursor + delta as usize).min(count - 1);
    }
}

/// Bascule l'état activé/désactivé du champ d'en-tête ou paramètre courant (D5).
fn toggle_field(model: &mut Model) {
    let Some(session) = &model.editing else {
        return;
    };
    if !matches!(session.mode, EditMode::Normal) {
        return;
    }
    let Some(field) = session.fields.get(session.cursor).copied() else {
        return;
    };
    let Some(TreeNode::Request(request)) = model.selected_node() else {
        return;
    };
    let current = field_enabled(session, &request.view, &field);
    let edit = match field {
        EditableField::HeaderValue(index) => Some(FieldEdit::HeaderEnabled {
            index,
            enabled: !current,
        }),
        EditableField::QueryParamValue(index) => Some(FieldEdit::QueryParamEnabled {
            index,
            enabled: !current,
        }),
        EditableField::PathParamValue(index) => Some(FieldEdit::PathParamEnabled {
            index,
            enabled: !current,
        }),
        EditableField::Url | EditableField::BodyText => None,
    };
    if let Some(edit) = edit
        && let Some(session) = &mut model.editing
    {
        session.pending.push(edit);
        session.dirty = true;
    }
}

/// Entre en mode Insert sur le champ sous le curseur (D5).
fn enter_insert(model: &mut Model) {
    let Some(session) = &model.editing else {
        return;
    };
    if !matches!(session.mode, EditMode::Normal) {
        return;
    }
    let Some(field) = session.fields.get(session.cursor).copied() else {
        return;
    };
    let Some(TreeNode::Request(request)) = model.selected_node() else {
        return;
    };
    let current_val = field_value(session, &request.view, &field);
    let text_cursor = current_val.chars().count();
    let buffer = current_val.to_owned();
    if let Some(session) = &mut model.editing {
        session.mode = EditMode::Insert {
            text_cursor,
            buffer,
        };
    }
}

/// Quitte le mode Insert vers le mode Normal, conservant la valeur modifiée en mémoire (D5).
fn leave_insert(model: &mut Model) {
    let Some(session) = &model.editing else {
        return;
    };
    let EditMode::Insert { buffer, .. } = &session.mode else {
        return;
    };
    let Some(field) = session.fields.get(session.cursor).copied() else {
        if let Some(session) = &mut model.editing {
            session.mode = EditMode::Normal;
        }
        return;
    };
    let Some(TreeNode::Request(request)) = model.selected_node() else {
        return;
    };
    let committed = super::model::field_value_committed(session, &request.view, &field);
    let maybe_new_value = if buffer != committed {
        Some(buffer.clone())
    } else {
        None
    };
    if let Some(session) = &mut model.editing {
        if let Some(new_value) = maybe_new_value {
            update_or_push_pending(session, field, new_value);
            session.dirty = true;
        }
        session.mode = EditMode::Normal;
    }
}

fn update_or_push_pending(session: &mut EditSession, field: EditableField, value: String) {
    match field {
        EditableField::Url => {
            if let Some(existing) = session.pending.iter_mut().rev().find_map(|e| match e {
                FieldEdit::Url(v) => Some(v),
                _ => None,
            }) {
                *existing = value;
            } else {
                session.pending.push(FieldEdit::Url(value));
            }
        }
        EditableField::HeaderValue(index) => {
            if let Some(existing) = session.pending.iter_mut().rev().find_map(|e| match e {
                FieldEdit::HeaderValue { index: i, value: v } if *i == index => Some(v),
                _ => None,
            }) {
                *existing = value;
            } else {
                session
                    .pending
                    .push(FieldEdit::HeaderValue { index, value });
            }
        }
        EditableField::QueryParamValue(index) => {
            if let Some(existing) = session.pending.iter_mut().rev().find_map(|e| match e {
                FieldEdit::QueryParamValue { index: i, value: v } if *i == index => Some(v),
                _ => None,
            }) {
                *existing = value;
            } else {
                session
                    .pending
                    .push(FieldEdit::QueryParamValue { index, value });
            }
        }
        EditableField::PathParamValue(index) => {
            if let Some(existing) = session.pending.iter_mut().rev().find_map(|e| match e {
                FieldEdit::PathParamValue { index: i, value: v } if *i == index => Some(v),
                _ => None,
            }) {
                *existing = value;
            } else {
                session
                    .pending
                    .push(FieldEdit::PathParamValue { index, value });
            }
        }
        EditableField::BodyText => {
            if let Some(existing) = session.pending.iter_mut().rev().find_map(|e| match e {
                FieldEdit::BodyText(v) => Some(v),
                _ => None,
            }) {
                *existing = value;
            } else {
                session.pending.push(FieldEdit::BodyText(value));
            }
        }
    }
}

/// `Entrée` en mode Insert : saut de ligne sur le corps, sortie d'Insert sinon (D5).
fn insert_enter(model: &mut Model) {
    let Some(session) = &mut model.editing else {
        return;
    };
    let current_field = session.fields.get(session.cursor).copied();
    if current_field == Some(EditableField::BodyText) {
        if let EditMode::Insert {
            text_cursor,
            buffer,
        } = &mut session.mode
        {
            let mut chars: Vec<char> = buffer.chars().collect();
            let cursor = (*text_cursor).min(chars.len());
            chars.insert(cursor, '\n');
            *buffer = chars.into_iter().collect();
            *text_cursor = cursor + 1;
        }
    } else {
        leave_insert(model);
    }
}

/// Caractère saisi en mode Insert (D4, D5).
fn insert_char(model: &mut Model, c: char) {
    let Some(session) = &mut model.editing else {
        return;
    };
    if let EditMode::Insert {
        text_cursor,
        buffer,
    } = &mut session.mode
    {
        let mut chars: Vec<char> = buffer.chars().collect();
        let cursor = (*text_cursor).min(chars.len());
        chars.insert(cursor, c);
        *buffer = chars.into_iter().collect();
        *text_cursor = cursor + 1;
    }
}

/// `Retour arrière` en mode Insert (D4, D5).
fn insert_backspace(model: &mut Model) {
    let Some(session) = &mut model.editing else {
        return;
    };
    if let EditMode::Insert {
        text_cursor,
        buffer,
    } = &mut session.mode
        && *text_cursor > 0
    {
        let mut chars: Vec<char> = buffer.chars().collect();
        let cursor = *text_cursor;
        if cursor <= chars.len() {
            chars.remove(cursor - 1);
            *buffer = chars.into_iter().collect();
            *text_cursor = cursor - 1;
        }
    }
}

/// Déplacement du curseur texte vers la gauche (D4, D5).
fn insert_cursor_left(model: &mut Model) {
    let Some(session) = &mut model.editing else {
        return;
    };
    if let EditMode::Insert { text_cursor, .. } = &mut session.mode {
        *text_cursor = text_cursor.saturating_sub(1);
    }
}

/// Déplacement du curseur texte vers la droite (D4, D5).
fn insert_cursor_right(model: &mut Model) {
    let Some(session) = &mut model.editing else {
        return;
    };
    if let EditMode::Insert {
        text_cursor,
        buffer,
    } = &mut session.mode
    {
        let char_count = buffer.chars().count();
        *text_cursor = (*text_cursor + 1).min(char_count);
    }
}

/// Déclenche la sauvegarde des modifications si la session est modifiée (D7).
fn save_edit(model: &mut Model) -> Command {
    let Some(session) = &model.editing else {
        return Command::None;
    };
    if !session.dirty {
        return Command::None;
    }
    let Some(TreeNode::Request(req)) = model.selected_node() else {
        return Command::None;
    };
    if req.path != session.path {
        return Command::None;
    }
    let Some(ast) = &req.ast else {
        return Command::None;
    };
    Command::SaveEdit {
        path: session.path.clone(),
        ast: ast.clone(),
        stamp: session.stamp,
        edits: session.pending.clone(),
    }
}

/// Applique le résultat d'une sauvegarde (D7).
fn edit_saved(
    model: &mut Model,
    path: std::path::PathBuf,
    result: Result<super::model::SavedEdit, crate::writer::WriteError>,
) {
    let Some(session) = &mut model.editing else {
        return;
    };
    // Garde contre un événement obsolète (chemin ne correspondant plus à la session active)
    if session.path != path {
        return;
    }
    match result {
        Ok(saved) => {
            session.stamp = saved.stamp;
            session.pending.clear();
            session.dirty = false;
            session.fields = EditableField::list_for(&saved.view);
            if !session.fields.is_empty() {
                session.cursor = session.cursor.min(session.fields.len() - 1);
            }
            model.replace_request_node(&path, saved.ast, saved.view);
        }
        Err(error) => {
            model.last_status = Some(super::model::StatusMessage::SaveError(error.to_string()));
        }
    }
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

/// Même principe que [`detail_line_count`], pour la réponse
/// (`split-request-response-panels`).
fn response_line_count(model: &Model) -> usize {
    response_text(model).lines.len()
}

/// Défilement maximal étant donné un nombre de lignes logiques et la
/// hauteur intérieure du panneau. Pure : partagée par le détail et la
/// réponse, qui ne diffèrent que par la source du nombre de lignes et de
/// la hauteur (`design.md`, D3).
fn max_scroll(line_count: usize, height: u16) -> u16 {
    let max = line_count.saturating_sub(usize::from(height));
    u16::try_from(max).unwrap_or(u16::MAX)
}

/// Dernière ligne visible d'un panneau étant donné son défilement
/// courant, sa hauteur intérieure et son nombre de lignes logiques.
/// Toujours la vraie dernière ligne du contenu quand `scroll` est à son
/// maximum (voir le calcul dans `design.md`, D5 de
/// `improve-visual-design`) : c'est ce qui permet à une sélection
/// visuelle d'atteindre des lignes qu'aucun sommet de panneau ne peut
/// jamais atteindre seul.
fn viewport_bottom(scroll: u16, height: u16, line_count: usize) -> u16 {
    if line_count == 0 {
        return 0;
    }
    let bottom = scroll.saturating_add(height.saturating_sub(1));
    bottom.min(u16::try_from(line_count - 1).unwrap_or(u16::MAX))
}

/// Défilement maximal du détail : lignes logiques moins la hauteur du
/// panneau.
fn detail_max_scroll(model: &Model) -> u16 {
    let Some(areas) = layout_for(model.size) else {
        return 0;
    };
    max_scroll(detail_line_count(model), inner(areas.detail).height)
}

/// Même principe que [`detail_max_scroll`], pour la réponse.
fn response_max_scroll(model: &Model) -> u16 {
    let Some(areas) = layout_for(model.size) else {
        return 0;
    };
    max_scroll(response_line_count(model), inner(areas.response).height)
}

/// Dernière ligne visible du panneau de détail, à la position de
/// défilement courante.
pub(crate) fn bottom_of_viewport(model: &Model) -> u16 {
    let Some(areas) = layout_for(model.size) else {
        return model.detail_scroll;
    };
    viewport_bottom(
        model.detail_scroll,
        inner(areas.detail).height,
        detail_line_count(model),
    )
}

/// Même principe que [`bottom_of_viewport`], pour la réponse.
pub(crate) fn response_bottom_of_viewport(model: &Model) -> u16 {
    let Some(areas) = layout_for(model.size) else {
        return model.response_scroll;
    };
    viewport_bottom(
        model.response_scroll,
        inner(areas.response).height,
        response_line_count(model),
    )
}

/// Plage de lignes couvertes par la sélection visuelle active, s'il y en a
/// une. Jamais stockée à part : dérivée de l'ancre et du défilement
/// courant à chaque appel.
pub(crate) fn selection_range(model: &Model) -> Option<RangeInclusive<u16>> {
    let anchor = model.detail_selection?.anchor;
    let bottom = bottom_of_viewport(model);
    Some(anchor.min(bottom)..=anchor.max(bottom))
}

/// Même principe que [`selection_range`], pour la réponse.
pub(crate) fn response_selection_range(model: &Model) -> Option<RangeInclusive<u16>> {
    let anchor = model.response_selection?.anchor;
    let bottom = response_bottom_of_viewport(model);
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

/// Même principe que [`scroll_detail`], pour la réponse.
fn scroll_response(model: &mut Model, message: Message) {
    let page = layout_for(model.size).map_or(1, |areas| inner(areas.response).height.max(1));
    let max = response_max_scroll(model);
    let scroll = model.response_scroll;
    model.response_scroll = match message {
        Message::Up => scroll.saturating_sub(1),
        Message::Down => scroll.saturating_add(1).min(max),
        Message::PageUp => scroll.saturating_sub(page),
        Message::PageDown => scroll.saturating_add(page).min(max),
        Message::Home => 0,
        Message::End => max,
        _ => scroll,
    };
}

/// Réinitialise l'état d'affichage propre à un onglet de la réponse
/// (défilement, sélection, mise en évidence de recherche) : appelé à
/// chaque changement d'onglet, comme `clear_detail_view_state` l'est à
/// chaque changement de sélection (`response-tabs`).
fn reset_response_tab_view_state(model: &mut Model) {
    model.response_scroll = 0;
    model.response_match = None;
    model.response_selection = None;
}

fn previous_response_tab(model: &mut Model) {
    model.response_tab = model.response_tab.previous();
    reset_response_tab_view_state(model);
}

fn next_response_tab(model: &mut Model) {
    model.response_tab = model.response_tab.next();
    reset_response_tab_view_state(model);
}

/// Hauteur utile du corps de l'écran (pour les panneaux plein corps).
fn body_height(model: &Model) -> usize {
    layout_for(model.size).map_or(1, |areas| usize::from(inner(areas.body).height).max(1))
}

fn navigate_diagnostics(model: &mut Model, message: Message) {
    let Some(collection) = model.loaded() else {
        return;
    };
    let entries = super::diagnostics::diagnostics(collection);
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

fn navigate_history(model: &mut Model, message: Message) {
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

/// Indice à sélectionner à l'ouverture du panneau, pour retrouver
/// l'environnement déjà courant (0 si `current` est `None` ou ne
/// correspond à aucune entrée valide).
fn environment_index_for(
    collection: &crate::collection::Collection,
    current: Option<&str>,
) -> usize {
    let Some(name) = current else {
        return 0;
    };
    collection
        .environments
        .iter()
        .position(|entry| matches!(entry, Ok(env) if env.name == name))
        .map_or(0, |i| i + 1)
}

/// `↑`/`↓`/`Début`/`Fin`/`Entrée` dans le panneau de sélection
/// d'environnement.
fn navigate_environment_picker(model: &mut Model, message: Message) {
    let Some(collection) = model.loaded() else {
        return;
    };
    let count = 1 + collection.environments.len();
    let before = model.environment_selected.min(count - 1);
    match message {
        Message::Up => model.environment_selected = before.saturating_sub(1),
        Message::Down => model.environment_selected = (before + 1).min(count - 1),
        Message::Home => model.environment_selected = 0,
        Message::End => model.environment_selected = count - 1,
        Message::Right => select_environment_picker(model, before),
        _ => {}
    }
}

/// `Entrée` sur l'entrée courante du panneau : devient l'environnement
/// courant et referme le panneau, sauf si l'entrée est en erreur ou hors
/// limites (sans effet, cf. spec « Environnement invalide non
/// sélectionnable »).
fn select_environment_picker(model: &mut Model, index: usize) {
    let Some(collection) = model.loaded() else {
        return;
    };
    let Some(name_opt) = environment_name_at(collection, index) else {
        return;
    };
    let name = name_opt.map(str::to_owned);
    model.current_environment = name;
    model.focus = Focus::Tree;
}

/// Navigation croisée depuis le panneau de diagnostics vers l'arbre :
/// déplie les dossiers ancêtres (et le nœud lui-même si dossier), sélectionne
/// le nœud et repasse le focus à l'arbre.
fn cross_navigate_to_tree(model: &mut Model, address: &[usize]) {
    for prefix_len in 1..=address.len() {
        let prefix = &address[..prefix_len];
        if let Some(TreeNode::Folder(folder)) = model.node_at(prefix) {
            model.tree.expanded.insert(folder.path.clone());
        }
    }
    refresh_rows(model);
    if let Some(index) = model.tree.rows.iter().position(|r| r.address == address) {
        model.tree.selected = index;
    }
    clear_detail_view_state(model);
    scroll_tree_into_view(model);
    model.focus = Focus::Tree;
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
        Focus::Response => SearchScope::Response,
        Focus::Diagnostics | Focus::History | Focus::EnvironmentPicker | Focus::Secrets => {
            SearchScope::Tree
        }
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

// --- Filtre de réponse ---------------------------------------------------

/// `|` : ouvre la saisie du filtre jq si la sélection courante a produit
/// une réponse HTTP avec un corps non nul (D4).
fn open_filter(model: &mut Model) {
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
fn confirm_filter(model: &mut Model) {
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
fn reset_filter_if_selection_changed(model: &mut Model, previous_path: Option<&std::path::Path>) {
    let current_path = model.selected_node().map(TreeNode::path);
    if previous_path != current_path {
        model.filter = None;
    }
}

// --- Sélection visuelle et copie -----------------------------------------

/// `v`, hors saisie, en focus Détail ou Réponse seulement (produit
/// inconditionnellement par `message.rs`, filtré ici selon le focus
/// courant). Chaque panneau porte sa propre sélection, indépendamment de
/// l'autre (`split-request-response-panels`).
fn toggle_visual(model: &mut Model) {
    match model.focus {
        Focus::Detail => {
            if model.detail_selection.take().is_some() {
                return;
            }
            model.detail_selection = Some(DetailSelection {
                anchor: model.detail_scroll,
            });
        }
        Focus::Response => {
            if model.response_selection.take().is_some() {
                return;
            }
            model.response_selection = Some(DetailSelection {
                anchor: model.response_scroll,
            });
        }
        _ => {}
    }
}

/// `y` : copie la sélection visuelle si elle est active, sinon la seule
/// ligne au sommet du panneau. Sans effet hors focus Détail ou Réponse
/// (l'arbre et la saisie de recherche n'ont pas de notion de ligne à
/// copier), à partir du panneau qui a le focus.
fn yank(model: &mut Model) -> Command {
    let (lines, range, scroll) = match model.focus {
        Focus::Detail => (
            plain_lines(model),
            selection_range(model),
            model.detail_scroll,
        ),
        Focus::Response => (
            response_plain_lines(model),
            response_selection_range(model),
            model.response_scroll,
        ),
        _ => return Command::None,
    };
    let text = match range {
        Some(range) => {
            let selected: Vec<&str> = lines
                .iter()
                .enumerate()
                .filter(|(index, _)| range.contains(&(*index as u16)))
                .map(|(_, line)| line.as_str())
                .collect();
            match model.focus {
                Focus::Detail => model.detail_selection = None,
                Focus::Response => model.response_selection = None,
                _ => {}
            }
            selected.join("\n")
        }
        None => lines.get(usize::from(scroll)).cloned().unwrap_or_default(),
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
    use std::path::{Path, PathBuf};

    use super::*;
    use crate::app::model::{
        ActiveRun, HistoryEntry, HistoryOutcome, PendingConfirm, RunFailure, SavedEdit,
        StatusMessage,
    };
    use crate::app::test_support::{loaded_model, runner_probe_model, select, selected_name};
    use crate::collection::{LoadError, RequestView};
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
    fn run_selected_from_history_replays_target() {
        let mut model = loaded_model((100, 30));
        model.focus = Focus::History;
        model.history.push_back(HistoryEntry {
            started_at: std::time::SystemTime::now(),
            target: std::path::PathBuf::from("grp"),
            recursive: true,
            outcome: HistoryOutcome::Completed {
                total: 2,
                failed: 0,
                duration_secs: 0.5,
            },
        });
        model.history_selected = 0;

        // Cas 1 : aucune exécution en cours -> Command::StartRun
        let command = update(&mut model, Message::RunSelected);
        match command {
            Command::StartRun { request, target } => {
                assert_eq!(target, Path::new("grp"));
                assert_eq!(request.targets, vec![std::path::PathBuf::from("grp")]);
                assert!(request.recursive);
            }
            other => panic!("attendu StartRun, obtenu {other:?}"),
        }

        // Cas 2 : exécution en cours -> Command::None
        model.run.active = Some(ActiveRun {
            id: runner::RunId(42),
            target: std::path::PathBuf::from("other"),
            recursive: false,
            handle: None,
        });
        let command = update(&mut model, Message::RunSelected);
        assert!(matches!(command, Command::None));
    }

    #[test]
    fn toggle_environment_picker_opens_and_closes() {
        let mut model = loaded_model((100, 30));
        assert_eq!(model.focus, Focus::Tree);

        // Ouverture
        update(&mut model, Message::ToggleEnvironmentPicker);
        assert_eq!(model.focus, Focus::EnvironmentPicker);
        assert_eq!(model.environment_selected, 0);

        // Second appui : fermeture vers l'arbre
        update(&mut model, Message::ToggleEnvironmentPicker);
        assert_eq!(model.focus, Focus::Tree);

        // Rouvrir retrouve l'indice de l'environnement déjà courant
        // (parser-cases/environments : local, malformed, staging)
        model.current_environment = Some("staging".into());
        update(&mut model, Message::ToggleEnvironmentPicker);
        assert_eq!(model.focus, Focus::EnvironmentPicker);
        assert_eq!(model.environment_selected, 3);

        // Sans collection chargée : aucun effet
        let mut loading_model = Model::new("/x".into(), (100, 30));
        update(&mut loading_model, Message::ToggleEnvironmentPicker);
        assert_eq!(loading_model.focus, Focus::Tree);
    }

    #[test]
    fn environment_picker_navigation_is_bounded() {
        // parser-cases/environments a 3 entrées (+ « Aucun ») -> indices 0..=3
        let mut model = loaded_model((100, 30));
        model.focus = Focus::EnvironmentPicker;
        assert_eq!(model.environment_selected, 0);

        update(&mut model, Message::Up);
        assert_eq!(model.environment_selected, 0, "borne haute (0)");

        update(&mut model, Message::Down);
        assert_eq!(model.environment_selected, 1);
        update(&mut model, Message::End);
        assert_eq!(model.environment_selected, 3);
        update(&mut model, Message::Down);
        assert_eq!(model.environment_selected, 3, "borne basse (3)");
        update(&mut model, Message::Home);
        assert_eq!(model.environment_selected, 0);
    }

    #[test]
    fn selecting_none_valid_or_invalid_environment_entry() {
        let mut model = loaded_model((100, 30));

        // « Aucun » (indice 0) : ferme le panneau, environnement courant None
        model.current_environment = Some("local".into());
        model.focus = Focus::EnvironmentPicker;
        model.environment_selected = 0;
        update(&mut model, Message::Right);
        assert_eq!(model.focus, Focus::Tree);
        assert_eq!(model.current_environment, None);

        // Environnement valide (indice 1 : local)
        model.focus = Focus::EnvironmentPicker;
        model.environment_selected = 1;
        update(&mut model, Message::Right);
        assert_eq!(model.focus, Focus::Tree);
        assert_eq!(model.current_environment.as_deref(), Some("local"));

        // Entrée en erreur (indice 2 : malformed) : sans effet
        model.focus = Focus::EnvironmentPicker;
        model.environment_selected = 2;
        update(&mut model, Message::Right);
        assert_eq!(model.focus, Focus::EnvironmentPicker, "reste ouvert");
        assert_eq!(
            model.current_environment.as_deref(),
            Some("local"),
            "inchangé"
        );
    }

    #[test]
    fn escape_closes_environment_picker_without_changing_selection() {
        let mut model = loaded_model((100, 30));
        model.current_environment = Some("local".into());
        model.focus = Focus::EnvironmentPicker;
        model.environment_selected = 3; // survole `staging` sans valider

        update(&mut model, Message::FocusTree);
        assert_eq!(model.focus, Focus::Tree);
        assert_eq!(model.current_environment.as_deref(), Some("local"));
    }

    #[test]
    fn run_selected_carries_the_current_environment() {
        // Sans environnement courant : `env` reste `None` (non-régression)
        let mut model = loaded_model((100, 30));
        select(&mut model, "simple-get.bru");
        match update(&mut model, Message::RunSelected) {
            Command::StartRun { request, .. } => assert_eq!(request.env, None),
            other => panic!("StartRun attendu, obtenu {other:?}"),
        }

        // Requête directe
        let mut model = loaded_model((100, 30));
        model.current_environment = Some("public".into());
        select(&mut model, "simple-get.bru");
        match update(&mut model, Message::RunSelected) {
            Command::StartRun { request, .. } => {
                assert_eq!(request.env.as_deref(), Some("public"));
            }
            other => panic!("StartRun attendu, obtenu {other:?}"),
        }

        // Dossier récursif
        let mut model = loaded_model((100, 30));
        model.current_environment = Some("public".into());
        select(&mut model, "grp");
        match update(&mut model, Message::RunSelected) {
            Command::StartRun { request, .. } => {
                assert_eq!(request.env.as_deref(), Some("public"));
                assert!(request.recursive);
            }
            other => panic!("StartRun attendu, obtenu {other:?}"),
        }

        // Rejeu depuis l'historique
        let mut model = loaded_model((100, 30));
        model.current_environment = Some("public".into());
        model.focus = Focus::History;
        model.history.push_back(HistoryEntry {
            started_at: std::time::SystemTime::now(),
            target: std::path::PathBuf::from("simple-get.bru"),
            recursive: false,
            outcome: HistoryOutcome::Completed {
                total: 1,
                failed: 0,
                duration_secs: 0.1,
            },
        });
        model.history_selected = 0;
        match update(&mut model, Message::RunSelected) {
            Command::StartRun { request, .. } => {
                assert_eq!(request.env.as_deref(), Some("public"));
            }
            other => panic!("StartRun attendu, obtenu {other:?}"),
        }
    }

    #[test]
    fn collection_reload_resets_current_environment_and_picker_focus() {
        let mut model = loaded_model((100, 30));
        model.current_environment = Some("local".into());
        model.focus = Focus::EnvironmentPicker;

        use crate::collection::CollectionLoader;
        let reloaded = crate::collection::BruLoader.load(&model.source.clone());
        update(&mut model, Message::CollectionLoaded(reloaded));

        assert_eq!(model.current_environment, None);
        assert_eq!(model.focus, Focus::Tree);
    }

    #[test]
    fn replay_entry_does_not_modify_existing_entry_and_adds_new() {
        let mut model = loaded_model((100, 30));
        model.focus = Focus::History;
        let original_entry = HistoryEntry {
            started_at: std::time::SystemTime::UNIX_EPOCH,
            target: std::path::PathBuf::from("simple-get.bru"),
            recursive: false,
            outcome: HistoryOutcome::Completed {
                total: 1,
                failed: 0,
                duration_secs: 0.1,
            },
        };
        model.history.push_back(original_entry.clone());
        model.history_selected = 0;

        let command = update(&mut model, Message::RunSelected);
        assert!(matches!(command, Command::StartRun { .. }));

        // L'entrée existante n'a pas été modifiée
        assert_eq!(model.history.len(), 1);
        assert_eq!(model.history[0], original_entry);

        // Simulation de la fin de l'exécution rejouée
        let new_id = runner::RunId(99);
        model.run.active = Some(ActiveRun {
            id: new_id,
            target: std::path::PathBuf::from("simple-get.bru"),
            recursive: false,
            handle: None,
        });
        let new_report = sample_report(&["simple-get.bru"]);
        update(
            &mut model,
            Message::RunFinished(runner::RunEvent {
                id: new_id,
                outcome: runner::RunOutcome::Completed {
                    report: new_report,
                    exit_code: Some(0),
                },
            }),
        );

        // Le journal a maintenant 2 entrées : la nouvelle en tête, l'ancienne préservée
        assert_eq!(model.history.len(), 2);
        assert_eq!(model.history[1], original_entry);
        assert_ne!(model.history[0].started_at, original_entry.started_at);
        assert_eq!(model.history[0].target, Path::new("simple-get.bru"));
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

    #[cfg(unix)]
    #[tokio::test]
    async fn run_finished_creates_history_entry_with_fake_bru() {
        let fake_bru =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake-bru/fake-bru.sh");
        let (tx, mut rx) = tokio::sync::mpsc::channel(8);
        let runner = runner::BruRunner::with_program(fake_bru, tx);

        let mut model = loaded_model((100, 30));

        // 1. Rapport contenant un `pass` avec assertion en échec (mode "ok") -> failed >= 1
        let handle = runner.start(runner::RunRequest {
            collection_root: Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/collections/runner-probe"),
            targets: vec![std::path::PathBuf::from("ok")],
            recursive: false,
            env: None,
            env_vars: Vec::new(),
        });
        model.run.active = Some(ActiveRun {
            id: handle.id(),
            target: std::path::PathBuf::from("ok.bru"),
            recursive: false,
            handle: None,
        });
        let event = rx.recv().await.expect("événement");
        update(&mut model, Message::RunFinished(event));

        assert_eq!(model.history.len(), 1);
        let entry = &model.history[0];
        assert_eq!(entry.target, Path::new("ok.bru"));
        assert!(!entry.recursive);
        match &entry.outcome {
            HistoryOutcome::Completed {
                total,
                failed,
                duration_secs,
            } => {
                assert_eq!(*total, 5);
                assert!(*failed >= 1, "failed doit être >= 1 : {failed}");
                assert!(*duration_secs > 0.0);
            }
            other => panic!("attendu Completed, obtenu {other:?}"),
        }

        // 2. Exécution réussie (mode "report" avec un rapport sans échec)
        let success_json = std::env::temp_dir().join(format!(
            "bruno-tui-test-success-{}.json",
            std::process::id()
        ));
        let success_content = r#"[
            {
                "iterationIndex": 0,
                "results": [
                    {
                        "name": "green",
                        "path": "green",
                        "test": { "filename": "green.bru" },
                        "request": { "method": "GET", "url": "http://x", "headers": {} },
                        "response": { "status": 200, "statusText": "OK", "headers": null, "data": null, "url": "http://x", "responseTime": 5 },
                        "error": null,
                        "status": "pass",
                        "assertionResults": [],
                        "testResults": [],
                        "preRequestTestResults": [],
                        "postResponseTestResults": [],
                        "shouldStopRunnerExecution": false,
                        "runDuration": 0.042,
                        "iterationIndex": 0
                    }
                ],
                "summary": { "totalRequests": 1, "passedRequests": 1, "failedRequests": 0, "errorRequests": 0, "skippedRequests": 0, "totalAssertions": 0, "passedAssertions": 0, "failedAssertions": 0, "totalTests": 0, "passedTests": 0, "failedTests": 0, "totalPreRequestTests": 0, "passedPreRequestTests": 0, "failedPreRequestTests": 0, "totalPostResponseTests": 0, "passedPostResponseTests": 0, "failedPostResponseTests": 0 }
            }
        ]"#;
        std::fs::write(&success_json, success_content).expect("écriture rapport succès");
        let handle = runner.start(runner::RunRequest {
            collection_root: Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/collections/runner-probe"),
            targets: vec![std::path::PathBuf::from("report"), success_json.clone()],
            recursive: false,
            env: None,
            env_vars: Vec::new(),
        });
        model.run.active = Some(ActiveRun {
            id: handle.id(),
            target: std::path::PathBuf::from("green.bru"),
            recursive: false,
            handle: None,
        });
        let event = rx.recv().await.expect("événement");
        let _ = std::fs::remove_file(&success_json);
        update(&mut model, Message::RunFinished(event));

        assert_eq!(model.history.len(), 2);
        let entry = &model.history[0];
        assert_eq!(entry.target, Path::new("green.bru"));
        match &entry.outcome {
            HistoryOutcome::Completed {
                total,
                failed,
                duration_secs,
            } => {
                assert_eq!(*total, 1);
                assert_eq!(*failed, 0);
                assert!((*duration_secs - 0.042).abs() < 1e-6);
            }
            other => panic!("attendu Completed, obtenu {other:?}"),
        }

        // 3. Annulation (mode "sleep", cancel)
        let pid_path =
            std::env::temp_dir().join(format!("bruno-tui-test-cancel-{}.pid", std::process::id()));
        let _ = std::fs::remove_file(&pid_path);
        let handle = runner.start(runner::RunRequest {
            collection_root: Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/collections/runner-probe"),
            targets: vec![std::path::PathBuf::from("sleep"), pid_path.clone()],
            recursive: false,
            env: None,
            env_vars: Vec::new(),
        });
        let id = handle.id();
        model.run.active = Some(ActiveRun {
            id,
            target: std::path::PathBuf::from("sleeping.bru"),
            recursive: false,
            handle: None,
        });
        for _ in 0..100 {
            if pid_path.exists() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        handle.cancel();
        let event = rx.recv().await.expect("événement");
        let _ = std::fs::remove_file(&pid_path);
        update(&mut model, Message::RunFinished(event));

        assert_eq!(model.history.len(), 3);
        let entry = &model.history[0];
        assert_eq!(entry.target, Path::new("sleeping.bru"));
        assert_eq!(entry.outcome, HistoryOutcome::Cancelled);

        // 4. Erreur de lancement (mode "none")
        let handle = runner.start(runner::RunRequest {
            collection_root: Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/collections/runner-probe"),
            targets: vec![std::path::PathBuf::from("none")],
            recursive: false,
            env: None,
            env_vars: Vec::new(),
        });
        model.run.active = Some(ActiveRun {
            id: handle.id(),
            target: std::path::PathBuf::from("failed.bru"),
            recursive: false,
            handle: None,
        });
        let event = rx.recv().await.expect("événement");
        update(&mut model, Message::RunFinished(event));

        assert_eq!(model.history.len(), 4);
        let entry = &model.history[0];
        assert_eq!(entry.target, Path::new("failed.bru"));
        assert!(matches!(entry.outcome, HistoryOutcome::Failed(_)));
    }

    #[test]
    fn history_limit_truncates_oldest_after_201_runs() {
        let mut model = loaded_model((100, 30));
        for i in 0..201 {
            let id = runner::RunId(i as u64);
            let target = std::path::PathBuf::from(format!("target_{i}.bru"));
            model.run.active = Some(ActiveRun {
                id,
                target: target.clone(),
                recursive: false,
                handle: None,
            });
            update(
                &mut model,
                Message::RunFinished(runner::RunEvent {
                    id,
                    outcome: runner::RunOutcome::Cancelled,
                }),
            );
        }
        assert_eq!(model.history.len(), 200);
        assert_eq!(
            model.history.front().unwrap().target,
            Path::new("target_200.bru")
        );
        assert_eq!(
            model.history.back().unwrap().target,
            Path::new("target_1.bru")
        );
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

    fn response_line_index_containing(model: &Model, needle: &str) -> u16 {
        response_plain_lines(model)
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

        let areas = crate::app::view::layout_for(model.size).expect("taille suffisante");
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
    fn toggle_visual_then_escape_in_response_does_not_change_focus_or_detail_selection() {
        use crate::app::test_support::runner_probe_model;

        let mut model = runner_probe_model();
        select(&mut model, "green.bru");
        update(&mut model, Message::NextFocus); // Détail
        update(&mut model, Message::NextFocus); // Réponse
        let scroll_before = model.response_scroll;
        update(&mut model, Message::ToggleVisual);
        assert!(model.response_selection.is_some());
        update(&mut model, Message::FocusTree);
        assert!(model.response_selection.is_none());
        assert_eq!(model.response_scroll, scroll_before);
        // Le premier Échap annule seulement la sélection : le focus reste
        // sur la réponse, et la sélection du détail n'est pas concernée.
        assert_eq!(model.focus, Focus::Response);
        assert!(model.detail_selection.is_none());
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
        update(&mut model, Message::NextFocus); // Tab : focus Réponse, sélection intacte
        update(&mut model, Message::NextFocus); // Tab : focus Arbre, sélection intacte
        assert_eq!(model.focus, Focus::Tree);
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
    fn visual_selections_in_detail_and_response_are_independent() {
        use crate::app::test_support::runner_probe_model;

        let mut model = runner_probe_model();
        select(&mut model, "green.bru");

        // Sélection visuelle dans le détail.
        update(&mut model, Message::NextFocus); // Détail
        update(&mut model, Message::ToggleVisual);
        assert!(model.detail_selection.is_some());

        // Bascule vers la réponse et active sa propre sélection.
        update(&mut model, Message::NextFocus); // Réponse
        assert!(model.response_selection.is_none());
        update(&mut model, Message::ToggleVisual);
        assert!(model.response_selection.is_some());

        // Les deux sélections coexistent, chacune bornée à son panneau.
        assert!(model.detail_selection.is_some());
        assert!(model.response_selection.is_some());

        // Copier depuis la réponse ne lève que la sélection de la réponse.
        update(&mut model, Message::Yank);
        assert!(model.response_selection.is_none());
        assert!(model.detail_selection.is_some());
    }

    #[test]
    fn changing_selection_clears_both_detail_and_response_view_state() {
        use crate::app::test_support::runner_probe_model;

        let mut model = runner_probe_model();
        select(&mut model, "green.bru");
        update(&mut model, Message::NextFocus); // Détail
        model.detail_scroll = 2;
        model.detail_match = Some((2, 0..1));
        update(&mut model, Message::NextFocus); // Réponse
        model.response_scroll = 3;
        model.response_match = Some((3, 0..1));

        update(&mut model, Message::FocusTree);
        assert_eq!(
            model.focus,
            Focus::Tree,
            "aucune sélection à annuler d'abord"
        );
        update(&mut model, Message::Down);

        assert_eq!(model.detail_scroll, 0);
        assert!(model.detail_selection.is_none());
        assert!(model.detail_match.is_none());
        assert_eq!(model.response_scroll, 0);
        assert!(model.response_selection.is_none());
        assert!(model.response_match.is_none());
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

    #[test]
    fn confirm_priority_and_responses() {
        use super::super::model::{EditMode, EditSession, EditableField, PendingConfirm};
        use std::path::PathBuf;

        let mut model = loaded_model((100, 30));
        let stamp = crate::writer::FileStamp::capture(
            &PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/collections/writer-cases/simple.bru"),
        )
        .expect("stamp");

        model.editing = Some(EditSession {
            path: PathBuf::from("req.bru"),
            stamp,
            fields: vec![EditableField::Url],
            cursor: 0,
            mode: EditMode::Normal,
            pending: vec![],
            dirty: true,
        });

        // 1. PendingConfirm::DiscardEdit
        model.confirm = Some(PendingConfirm::DiscardEdit);

        // Les touches ordinaires sont ignorées tant qu'une confirmation est en attente
        let cmd = update(&mut model, Message::Up);
        assert!(matches!(cmd, Command::None));
        assert!(model.confirm.is_some());
        assert!(model.editing.is_some());

        let cmd = update(&mut model, Message::Quit);
        assert!(matches!(cmd, Command::None));
        assert!(model.confirm.is_some());
        assert!(model.exit.is_none());

        // Refus de confirmation par ConfirmNo / 'n' / 'Échap'
        update(&mut model, Message::ConfirmNo);
        assert!(model.confirm.is_none());
        assert!(model.editing.is_some(), "session préservée");

        // Refus via Message::NextMatch ('n')
        model.confirm = Some(PendingConfirm::DiscardEdit);
        update(&mut model, Message::NextMatch);
        assert!(model.confirm.is_none());
        assert!(model.editing.is_some());

        // Refus via Message::FocusTree ('Échap')
        model.confirm = Some(PendingConfirm::DiscardEdit);
        update(&mut model, Message::FocusTree);
        assert!(model.confirm.is_none());
        assert!(model.editing.is_some());

        // Acceptation via ConfirmYes
        model.confirm = Some(PendingConfirm::DiscardEdit);
        update(&mut model, Message::ConfirmYes);
        assert!(model.confirm.is_none());
        assert!(model.editing.is_none(), "session fermée");

        // 2. PendingConfirm::QuitWithUnsavedEdit
        model.editing = Some(EditSession {
            path: PathBuf::from("req.bru"),
            stamp,
            fields: vec![EditableField::Url],
            cursor: 0,
            mode: EditMode::Normal,
            pending: vec![],
            dirty: true,
        });
        model.confirm = Some(PendingConfirm::QuitWithUnsavedEdit);

        // Refus de quitter
        update(&mut model, Message::ConfirmNo);
        assert!(model.confirm.is_none());
        assert!(model.exit.is_none());

        // Acceptation de quitter via ConfirmYes / 'y' (Message::Yank) / 'Entrée' (Message::Right)
        model.confirm = Some(PendingConfirm::QuitWithUnsavedEdit);
        update(&mut model, Message::Yank);
        assert!(model.confirm.is_none());
        assert!(matches!(model.exit, Some(Exit::Normal)));
    }

    #[test]
    fn start_edit_opens_session_only_in_detail_on_request() {
        let mut model = loaded_model((100, 30));

        // 1. Sur une requête mais avec Focus::Tree : sans effet
        select(&mut model, "simple-get.bru");
        assert_eq!(model.focus, Focus::Tree);
        update(&mut model, Message::StartEdit);
        assert!(model.editing.is_none());

        // 2. Sur un dossier avec Focus::Detail : sans effet
        select(&mut model, "grp");
        update(&mut model, Message::NextFocus);
        assert_eq!(model.focus, Focus::Detail);
        update(&mut model, Message::StartEdit);
        assert!(model.editing.is_none());

        // 3. Sur un nœud en erreur avec Focus::Detail : sans effet
        select(&mut model, "broken.bru");
        assert_eq!(model.focus, Focus::Detail);
        update(&mut model, Message::StartEdit);
        assert!(model.editing.is_none());

        // 4. Sur une requête valide avec Focus::Detail : ouverture
        select(&mut model, "simple-get.bru");
        assert_eq!(model.focus, Focus::Detail);
        update(&mut model, Message::StartEdit);
        assert!(model.editing.is_some());
        let session = model.editing.as_ref().unwrap();
        assert_eq!(session.path, std::path::PathBuf::from("simple-get.bru"));
        assert_eq!(session.cursor, 0);
        assert_eq!(session.mode, EditMode::Normal);
        assert!(session.pending.is_empty());
        assert!(!session.dirty);
        assert!(!session.fields.is_empty());

        // 5. Si une session est déjà ouverte, StartEdit ne fait rien
        let stamp = session.stamp;
        update(&mut model, Message::StartEdit);
        assert_eq!(model.editing.as_ref().unwrap().stamp, stamp);

        // 6. Sur une requête valide avec Focus::Response : sans effet
        // (l'édition ne porte que sur le panneau Détail,
        // `split-request-response-panels`). Focus déjà sur Détail depuis
        // le cas 4 : un seul Tab suffit pour atteindre la réponse.
        model.editing = None;
        update(&mut model, Message::NextFocus);
        assert_eq!(model.focus, Focus::Response);
        update(&mut model, Message::StartEdit);
        assert!(model.editing.is_none());
    }

    #[test]
    fn next_focus_cycles_through_tree_detail_and_response() {
        let mut model = loaded_model((100, 30));
        assert_eq!(model.focus, Focus::Tree);
        update(&mut model, Message::NextFocus);
        assert_eq!(model.focus, Focus::Detail);
        update(&mut model, Message::NextFocus);
        assert_eq!(model.focus, Focus::Response);
        update(&mut model, Message::NextFocus);
        assert_eq!(model.focus, Focus::Tree);
    }

    #[test]
    fn response_tab_cycles_forward_with_right() {
        use crate::app::test_support::runner_probe_model;

        let mut model = runner_probe_model();
        select(&mut model, "green.bru");
        update(&mut model, Message::NextFocus); // Détail
        update(&mut model, Message::NextFocus); // Réponse
        assert_eq!(model.focus, Focus::Response);
        assert_eq!(model.response_tab, ResponseTab::Body);

        update(&mut model, Message::Right);
        assert_eq!(model.response_tab, ResponseTab::Headers);
        update(&mut model, Message::Right);
        assert_eq!(model.response_tab, ResponseTab::Tests);
        // Cycle circulaire : un troisième Right revient à Corps.
        update(&mut model, Message::Right);
        assert_eq!(model.response_tab, ResponseTab::Body);
    }

    #[test]
    fn response_tab_cycles_backward_with_left() {
        use crate::app::test_support::runner_probe_model;

        let mut model = runner_probe_model();
        select(&mut model, "green.bru");
        update(&mut model, Message::NextFocus); // Détail
        update(&mut model, Message::NextFocus); // Réponse
        assert_eq!(model.response_tab, ResponseTab::Body);

        // Cycle circulaire vers l'arrière : depuis Corps, Left va à Tests.
        update(&mut model, Message::Left);
        assert_eq!(model.response_tab, ResponseTab::Tests);
    }

    #[test]
    fn changing_selection_resets_the_active_response_tab() {
        use crate::app::test_support::runner_probe_model;

        let mut model = runner_probe_model();
        select(&mut model, "green.bru");
        update(&mut model, Message::NextFocus); // Détail
        update(&mut model, Message::NextFocus); // Réponse
        update(&mut model, Message::Right);
        update(&mut model, Message::Right);
        assert_eq!(model.response_tab, ResponseTab::Tests);

        update(&mut model, Message::FocusTree);
        update(&mut model, Message::Down);
        assert_eq!(model.response_tab, ResponseTab::Body);
    }

    #[test]
    fn opening_the_filter_switches_to_the_body_tab() {
        use crate::app::test_support::runner_probe_model;

        let mut model = runner_probe_model();
        select(&mut model, "json.bru");
        update(&mut model, Message::NextFocus); // Détail
        update(&mut model, Message::NextFocus); // Réponse
        update(&mut model, Message::Right); // En-têtes
        assert_eq!(model.response_tab, ResponseTab::Headers);

        update(&mut model, Message::OpenFilter);
        assert_eq!(model.response_tab, ResponseTab::Body);
    }

    #[test]
    fn changing_response_tab_resets_scroll_match_and_selection() {
        use crate::app::test_support::runner_probe_model;

        let mut model = runner_probe_model();
        select(&mut model, "green.bru");
        update(&mut model, Message::NextFocus); // Détail
        update(&mut model, Message::NextFocus); // Réponse
        model.response_scroll = 2;
        model.response_match = Some((2, 0..1));
        model.response_selection = Some(DetailSelection { anchor: 2 });

        update(&mut model, Message::Right);

        assert_eq!(model.response_scroll, 0);
        assert!(model.response_match.is_none());
        assert!(model.response_selection.is_none());
    }

    #[test]
    fn move_field_cursor_navigation_and_bounds() {
        let mut model = loaded_model((100, 30));
        select(&mut model, "post-json.bru");
        update(&mut model, Message::NextFocus);
        update(&mut model, Message::StartEdit);
        let count = model.editing.as_ref().unwrap().fields.len();
        assert!(count >= 3, "au moins 3 champs attendus");

        // Curseur initial à 0
        assert_eq!(model.editing.as_ref().unwrap().cursor, 0);

        // Flèche haut sur la borne 0 : sans effet
        update(&mut model, Message::Up);
        assert_eq!(model.editing.as_ref().unwrap().cursor, 0);

        // Parcours vers le bas
        for expected in 1..count {
            update(&mut model, Message::Down);
            assert_eq!(model.editing.as_ref().unwrap().cursor, expected);
        }

        // Flèche bas sur la dernière borne : sans effet
        update(&mut model, Message::Down);
        assert_eq!(model.editing.as_ref().unwrap().cursor, count - 1);

        // Remontée vers le haut
        update(&mut model, Message::Up);
        assert_eq!(model.editing.as_ref().unwrap().cursor, count - 2);

        // Via Message::MoveFieldCursor direct
        update(&mut model, Message::MoveFieldCursor(1));
        assert_eq!(model.editing.as_ref().unwrap().cursor, count - 1);

        update(&mut model, Message::MoveFieldCursor(-1));
        assert_eq!(model.editing.as_ref().unwrap().cursor, count - 2);
    }

    #[test]
    fn toggle_field_toggles_header_and_params_and_marks_dirty() {
        let mut model = loaded_model((100, 30));
        select(&mut model, "scripted.bru");
        update(&mut model, Message::NextFocus);
        update(&mut model, Message::StartEdit);

        // Champ 0 : URL -> ToggleField sans effet
        assert_eq!(model.editing.as_ref().unwrap().cursor, 0);
        assert!(matches!(
            model.editing.as_ref().unwrap().fields[0],
            EditableField::Url
        ));
        update(&mut model, Message::ToggleField);
        assert!(model.editing.as_ref().unwrap().pending.is_empty());
        assert!(!model.editing.as_ref().unwrap().dirty);

        // Champ 1 : En-tête Accept (initialement activé dans scripted.bru)
        update(&mut model, Message::Down);
        assert_eq!(model.editing.as_ref().unwrap().cursor, 1);
        assert!(matches!(
            model.editing.as_ref().unwrap().fields[1],
            EditableField::HeaderValue(0)
        ));

        let req = match model.selected_node().unwrap() {
            TreeNode::Request(r) => &r.view,
            _ => panic!("requête attendue"),
        };
        assert!(field_enabled(
            model.editing.as_ref().unwrap(),
            req,
            &EditableField::HeaderValue(0)
        ));

        // Bascule : devient désactivé
        update(&mut model, Message::ToggleField);
        assert!(model.editing.as_ref().unwrap().dirty);
        assert_eq!(model.editing.as_ref().unwrap().pending.len(), 1);
        let req = match model.selected_node().unwrap() {
            TreeNode::Request(r) => &r.view,
            _ => panic!("requête attendue"),
        };
        assert!(!field_enabled(
            model.editing.as_ref().unwrap(),
            req,
            &EditableField::HeaderValue(0)
        ));

        // Seconde bascule : redevient activé
        update(&mut model, Message::ToggleField);
        let req = match model.selected_node().unwrap() {
            TreeNode::Request(r) => &r.view,
            _ => panic!("requête attendue"),
        };
        assert!(field_enabled(
            model.editing.as_ref().unwrap(),
            req,
            &EditableField::HeaderValue(0)
        ));

        // Test sur paramètre de requête : multiline.bru a des query params
        let mut model2 = loaded_model((100, 30));
        select(&mut model2, "multiline.bru");
        update(&mut model2, Message::NextFocus);
        update(&mut model2, Message::StartEdit);
        // Trouver le premier QueryParamValue
        let param_cursor = model2
            .editing
            .as_ref()
            .unwrap()
            .fields
            .iter()
            .position(|f| matches!(f, EditableField::QueryParamValue(_)))
            .expect("query param attendu");
        model2.editing.as_mut().unwrap().cursor = param_cursor;

        let req2 = match model2.selected_node().unwrap() {
            TreeNode::Request(r) => &r.view,
            _ => panic!("requête attendue"),
        };
        let param_field = model2.editing.as_ref().unwrap().fields[param_cursor];
        let init_enabled = field_enabled(model2.editing.as_ref().unwrap(), req2, &param_field);
        update(&mut model2, Message::ToggleField);
        assert!(model2.editing.as_ref().unwrap().dirty);
        let req2 = match model2.selected_node().unwrap() {
            TreeNode::Request(r) => &r.view,
            _ => panic!("requête attendue"),
        };
        assert_eq!(
            field_enabled(model2.editing.as_ref().unwrap(), req2, &param_field),
            !init_enabled
        );

        // Test sur corps : post-json.bru a un corps
        let mut model3 = loaded_model((100, 30));
        select(&mut model3, "post-json.bru");
        update(&mut model3, Message::NextFocus);
        update(&mut model3, Message::StartEdit);
        let body_cursor = model3
            .editing
            .as_ref()
            .unwrap()
            .fields
            .iter()
            .position(|f| matches!(f, EditableField::BodyText))
            .expect("corps attendu");
        model3.editing.as_mut().unwrap().cursor = body_cursor;
        update(&mut model3, Message::ToggleField);
        assert!(!model3.editing.as_ref().unwrap().dirty);
        assert!(model3.editing.as_ref().unwrap().pending.is_empty());
    }

    #[test]
    fn insert_mode_transitions_and_value_preservation() {
        let mut model = loaded_model((100, 30));
        select(&mut model, "simple-get.bru");
        update(&mut model, Message::NextFocus);
        update(&mut model, Message::StartEdit);

        // Curseur sur URL
        assert_eq!(model.editing.as_ref().unwrap().cursor, 0);
        let initial_url = {
            let req = match model.selected_node().unwrap() {
                TreeNode::Request(r) => &r.view,
                _ => panic!("requête attendue"),
            };
            field_value(model.editing.as_ref().unwrap(), req, &EditableField::Url).to_string()
        };
        let expected_len = initial_url.chars().count();

        // Entrée en mode Insert
        update(&mut model, Message::EnterInsert);
        match &model.editing.as_ref().unwrap().mode {
            EditMode::Insert {
                text_cursor,
                buffer,
            } => {
                assert_eq!(*text_cursor, expected_len);
                assert_eq!(buffer, &initial_url);
            }
            EditMode::Normal => panic!("Insert attendu"),
        }

        // Sortie sans frappe via LeaveInsert (Échap) : valeur conservée, pas de dirty
        update(&mut model, Message::LeaveInsert);
        assert_eq!(model.editing.as_ref().unwrap().mode, EditMode::Normal);
        assert!(!model.editing.as_ref().unwrap().dirty);
        assert!(model.editing.as_ref().unwrap().pending.is_empty());

        let req = match model.selected_node().unwrap() {
            TreeNode::Request(r) => &r.view,
            _ => panic!("requête attendue"),
        };
        assert_eq!(
            field_value(model.editing.as_ref().unwrap(), req, &EditableField::Url,),
            initial_url.as_str()
        );

        // Entrée en mode Insert puis sortie via InsertEnter sur champ à valeur unique (URL)
        update(&mut model, Message::EnterInsert);
        update(&mut model, Message::InsertEnter);
        assert_eq!(model.editing.as_ref().unwrap().mode, EditMode::Normal);
        assert!(!model.editing.as_ref().unwrap().dirty);
    }

    #[test]
    fn insert_typing_cursor_movement_and_commit_at_exit() {
        let mut model = loaded_model((100, 30));
        select(&mut model, "simple-get.bru");
        update(&mut model, Message::NextFocus);
        update(&mut model, Message::StartEdit);

        // 1. Aller-retour sans modification de valeur mais avec déplacement curseur : dirty inchangé
        update(&mut model, Message::EnterInsert);
        update(&mut model, Message::InsertCursorLeft);
        update(&mut model, Message::InsertCursorRight);
        update(&mut model, Message::LeaveInsert);
        assert!(!model.editing.as_ref().unwrap().dirty);
        assert!(model.editing.as_ref().unwrap().pending.is_empty());

        // 2. Frappe, suppression, insertion en milieu de chaîne
        update(&mut model, Message::EnterInsert);
        // Curseur initial à la fin de "https://{{host}}/ping"
        // Ajout d'un caractère à la fin
        update(&mut model, Message::InsertChar('s'));
        // Déplacement à gauche
        update(&mut model, Message::InsertCursorLeft);
        update(&mut model, Message::InsertCursorLeft);
        // Insertion en milieu de chaîne
        update(&mut model, Message::InsertChar('X'));
        // Retour arrière : supprime le X qu'on vient d'insérer
        update(&mut model, Message::InsertBackspace);

        // Tant qu'on est en Insert, pending n'est PAS mis à jour à chaque frappe
        assert!(model.editing.as_ref().unwrap().pending.is_empty());

        // Sortie d'Insert
        update(&mut model, Message::LeaveInsert);
        assert!(model.editing.as_ref().unwrap().dirty);
        assert_eq!(model.editing.as_ref().unwrap().pending.len(), 1);

        let req = match model.selected_node().unwrap() {
            TreeNode::Request(r) => &r.view,
            _ => panic!("requête attendue"),
        };
        assert_eq!(
            field_value(model.editing.as_ref().unwrap(), req, &EditableField::Url,),
            "https://{{host}}/pings"
        );
    }

    #[test]
    fn insert_enter_on_body_inserts_newline() {
        let mut model = loaded_model((100, 30));
        select(&mut model, "post-json.bru");
        update(&mut model, Message::NextFocus);
        update(&mut model, Message::StartEdit);

        let body_cursor = model
            .editing
            .as_ref()
            .unwrap()
            .fields
            .iter()
            .position(|f| matches!(f, EditableField::BodyText))
            .expect("corps attendu");
        model.editing.as_mut().unwrap().cursor = body_cursor;

        // Entrée en Insert sur le corps
        update(&mut model, Message::EnterInsert);

        // InsertEnter insère un saut de ligne et RESTE en Insert
        update(&mut model, Message::InsertEnter);
        match &model.editing.as_ref().unwrap().mode {
            EditMode::Insert { buffer, .. } => {
                assert!(buffer.ends_with('\n'), "saut de ligne inséré");
            }
            EditMode::Normal => panic!("devrait rester en mode Insert sur le corps"),
        }

        // Sortie par LeaveInsert (Échap)
        update(&mut model, Message::LeaveInsert);
        assert_eq!(model.editing.as_ref().unwrap().mode, EditMode::Normal);
        assert!(model.editing.as_ref().unwrap().dirty);
        assert_eq!(model.editing.as_ref().unwrap().pending.len(), 1);
    }

    #[test]
    fn save_edit_when_clean_returns_none_when_dirty_returns_command() {
        let mut model = loaded_model((100, 30));
        select(&mut model, "simple-get.bru");
        update(&mut model, Message::NextFocus);
        update(&mut model, Message::StartEdit);

        // 1. Session propre -> Command::None
        let cmd = update(&mut model, Message::SaveEdit);
        assert!(matches!(cmd, Command::None));

        // 2. Modification -> session dirty
        update(&mut model, Message::EnterInsert);
        update(&mut model, Message::InsertChar('x'));
        update(&mut model, Message::LeaveInsert);
        assert!(model.editing.as_ref().unwrap().dirty);

        // 3. Session modifiée -> Command::SaveEdit avec path, ast, stamp, edits
        let cmd = update(&mut model, Message::SaveEdit);
        match cmd {
            Command::SaveEdit {
                path,
                ast,
                stamp,
                edits,
            } => {
                assert_eq!(path, std::path::Path::new("simple-get.bru"));
                assert_eq!(stamp, model.editing.as_ref().unwrap().stamp);
                assert_eq!(edits, model.editing.as_ref().unwrap().pending);
                assert!(!edits.is_empty());
                // ast correspond bien à la requête
                let req_node = match model.selected_node().unwrap() {
                    TreeNode::Request(r) => r,
                    _ => panic!("requête attendue"),
                };
                assert_eq!(Some(&ast), req_node.ast.as_ref());
            }
            _ => panic!("Command::SaveEdit attendu"),
        }

        // L'émission de Command::SaveEdit ne vide PAS dirty ni pending
        assert!(model.editing.as_ref().unwrap().dirty);
        assert!(!model.editing.as_ref().unwrap().pending.is_empty());
    }

    #[test]
    fn edit_saved_success_updates_node_and_resets_dirty_and_pending() {
        let mut model = loaded_model((100, 30));
        select(&mut model, "simple-get.bru");
        update(&mut model, Message::NextFocus);
        update(&mut model, Message::StartEdit);

        // Modification
        update(&mut model, Message::EnterInsert);
        update(&mut model, Message::InsertChar('x'));
        update(&mut model, Message::LeaveInsert);

        let initial_stamp = model.editing.as_ref().unwrap().stamp;
        let new_source = "get {\n  url: https://updated.example.com\n}\n";
        let new_ast = crate::collection::BruFile::parse(new_source.to_string()).expect("parse");
        let new_view = RequestView::from_ast(&new_ast).expect("view");

        // Simuler un nouveau FileStamp avec une date différente
        let new_stamp = {
            let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/collections/writer-cases/headers.bru");
            crate::writer::FileStamp::capture(&path).expect("stamp")
        };

        let saved = SavedEdit {
            stamp: new_stamp,
            ast: new_ast.clone(),
            view: new_view.clone(),
        };

        let path = std::path::PathBuf::from("simple-get.bru");
        update(
            &mut model,
            Message::EditSaved {
                path,
                result: Ok(saved),
            },
        );

        let session = model.editing.as_ref().unwrap();
        assert!(!session.dirty);
        assert!(session.pending.is_empty());
        assert_eq!(session.stamp, new_stamp);
        assert_ne!(session.stamp, initial_stamp);

        // Le nœud dans le modèle est bien mis à jour
        let req_node = match model.selected_node().unwrap() {
            TreeNode::Request(r) => r,
            _ => panic!("requête attendue"),
        };
        assert_eq!(req_node.view.url, "https://updated.example.com");
        assert_eq!(req_node.ast.as_ref(), Some(&new_ast));
    }

    #[test]
    fn edit_saved_failure_retains_modifications_and_sets_status() {
        let mut model = loaded_model((100, 30));
        select(&mut model, "simple-get.bru");
        update(&mut model, Message::NextFocus);
        update(&mut model, Message::StartEdit);

        // Modification
        update(&mut model, Message::EnterInsert);
        update(&mut model, Message::InsertChar('x'));
        update(&mut model, Message::LeaveInsert);

        let pending_before = model.editing.as_ref().unwrap().pending.clone();
        let path = std::path::PathBuf::from("simple-get.bru");
        let error = crate::writer::WriteError::Stale { path: path.clone() };

        update(
            &mut model,
            Message::EditSaved {
                path,
                result: Err(error),
            },
        );

        // Session inchangée : dirty et pending conservés
        let session = model.editing.as_ref().unwrap();
        assert!(session.dirty);
        assert_eq!(session.pending, pending_before);

        // Message d'erreur dans last_status
        match &model.last_status {
            Some(StatusMessage::SaveError(msg)) => {
                assert!(msg.contains("modifié depuis son chargement"));
            }
            _ => panic!("StatusMessage::SaveError attendu"),
        }
    }

    #[test]
    fn edit_saved_stale_event_ignored() {
        let mut model = loaded_model((100, 30));
        select(&mut model, "simple-get.bru");
        update(&mut model, Message::NextFocus);
        update(&mut model, Message::StartEdit);

        // Modification
        update(&mut model, Message::EnterInsert);
        update(&mut model, Message::InsertChar('x'));
        update(&mut model, Message::LeaveInsert);

        let pending_before = model.editing.as_ref().unwrap().pending.clone();
        let stamp_before = model.editing.as_ref().unwrap().stamp;

        // Événement pour un autre chemin que la session active
        let other_path = std::path::PathBuf::from("other.bru");
        let new_source = "get {\n  url: https://other.example.com\n}\n";
        let new_ast = crate::collection::BruFile::parse(new_source.to_string()).expect("parse");
        let new_view = RequestView::from_ast(&new_ast).expect("view");
        let saved = SavedEdit {
            stamp: stamp_before,
            ast: new_ast,
            view: new_view,
        };

        update(
            &mut model,
            Message::EditSaved {
                path: other_path,
                result: Ok(saved),
            },
        );

        // Session inchangée
        let session = model.editing.as_ref().unwrap();
        assert!(session.dirty);
        assert_eq!(session.pending, pending_before);
        assert_eq!(session.stamp, stamp_before);
        assert!(model.last_status.is_none());
    }

    #[test]
    fn escape_in_session_closes_immediately_if_clean_and_asks_confirm_if_dirty() {
        let mut model = loaded_model((100, 30));
        select(&mut model, "simple-get.bru");
        update(&mut model, Message::NextFocus);
        update(&mut model, Message::StartEdit);

        // 1. Session non modifiée (!dirty) -> Échap (FocusTree) ferme immédiatement la session
        assert!(model.editing.is_some());
        assert!(!model.editing.as_ref().unwrap().dirty);
        update(&mut model, Message::FocusTree);
        assert!(model.editing.is_none());
        assert!(model.confirm.is_none());

        // 2. Session modifiée (dirty) -> Échap (FocusTree) demande confirmation DiscardEdit
        update(&mut model, Message::StartEdit);
        update(&mut model, Message::EnterInsert);
        update(&mut model, Message::InsertChar('a'));
        update(&mut model, Message::LeaveInsert);
        assert!(model.editing.as_ref().unwrap().dirty);

        update(&mut model, Message::FocusTree);
        assert_eq!(model.confirm, Some(PendingConfirm::DiscardEdit));
        assert!(model.editing.is_some(), "session toujours active");

        // Refus de confirmation -> session reste intacte
        update(&mut model, Message::ConfirmNo);
        assert!(model.confirm.is_none());
        assert!(model.editing.is_some());
        assert!(model.editing.as_ref().unwrap().dirty);

        // Nouvelle tentative d'Échap puis acceptation
        update(&mut model, Message::FocusTree);
        assert_eq!(model.confirm, Some(PendingConfirm::DiscardEdit));
        update(&mut model, Message::ConfirmYes);
        assert!(model.confirm.is_none());
        assert!(model.editing.is_none(), "session fermée après confirmation");
    }

    #[test]
    fn quit_and_force_quit_ask_confirm_if_dirty_and_exit_directly_otherwise() {
        let mut model = loaded_model((100, 30));

        // 1. Sans session d'édition : Quit ferme directement
        update(&mut model, Message::Quit);
        assert!(matches!(model.exit, Some(Exit::Normal)));
        assert!(model.confirm.is_none());

        // Réinitialisation de exit
        model.exit = None;

        // 2. Session ouverte propre (!dirty) : Quit ferme directement
        select(&mut model, "simple-get.bru");
        update(&mut model, Message::NextFocus);
        update(&mut model, Message::StartEdit);
        assert!(!model.editing.as_ref().unwrap().dirty);

        update(&mut model, Message::Quit);
        assert!(matches!(model.exit, Some(Exit::Normal)));
        assert!(model.confirm.is_none());

        model.exit = None;

        // 3. Session modifiée (dirty) : Quit demande confirmation QuitWithUnsavedEdit
        update(&mut model, Message::EnterInsert);
        update(&mut model, Message::InsertChar('a'));
        update(&mut model, Message::LeaveInsert);
        assert!(model.editing.as_ref().unwrap().dirty);

        update(&mut model, Message::Quit);
        assert!(model.exit.is_none(), "ne doit pas quitter immédiatement");
        assert_eq!(model.confirm, Some(PendingConfirm::QuitWithUnsavedEdit));

        // Refus : application et session restent actives
        update(&mut model, Message::ConfirmNo);
        assert!(model.confirm.is_none());
        assert!(model.exit.is_none());
        assert!(model.editing.is_some());

        // 4. Session modifiée avec ForceQuit (Ctrl+C hors confirm) : demande confirmation
        update(&mut model, Message::ForceQuit);
        assert!(model.exit.is_none());
        assert_eq!(model.confirm, Some(PendingConfirm::QuitWithUnsavedEdit));

        // Acceptation : sortie
        update(&mut model, Message::ConfirmYes);
        assert!(matches!(model.exit, Some(Exit::Normal)));
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
        use super::super::filter::FilterResult;

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

    // --- Variables secrètes (`secret-env-vars`) ---

    use crate::app::message::{MaskedChar, TextCapture};
    use crate::app::model::{SecretInput as Input, secret_rows as rows};
    use crate::secrets::{Resolved, SecretMapping, SecretSource};

    fn mapping(name: &str, key: Option<&str>) -> SecretMapping {
        SecretMapping {
            name: name.to_owned(),
            key: key.map(str::to_owned),
        }
    }

    fn resolved_dotenv(name: &str, key: &str, value: &str) -> Resolved {
        Resolved {
            name: name.to_owned(),
            source: SecretSource::DotEnv {
                key: key.to_owned(),
            },
            value: Some(SecretString::new(value)),
        }
    }

    fn type_text(model: &mut Model, text: &str) {
        for c in text.chars() {
            update(model, Message::SecretInput(MaskedChar(c)));
        }
    }

    fn env_vars_of(command: Command) -> Vec<(String, String)> {
        match command {
            Command::StartRun { request, .. } => request
                .env_vars
                .iter()
                .map(|(name, value)| (name.clone(), value.expose().to_owned()))
                .collect(),
            other => panic!("StartRun attendu, obtenu {other:?}"),
        }
    }

    /// `parser-cases` chargée, environnement `local` (`vars:secret [ token ]`).
    fn local_model() -> Model {
        let mut model = loaded_model((100, 30));
        model.current_environment = Some("local".into());
        model
    }

    #[test]
    fn collection_loaded_requests_resolution_of_all_known_names() {
        use crate::collection::CollectionLoader;
        let mut model = Model::new(crate::app::test_support::fixture(), (100, 30));
        model.secrets.mappings = vec![mapping("oktaClientSecret", None)];
        let loaded = crate::collection::BruLoader.load(&model.source.clone());
        match update(&mut model, Message::CollectionLoaded(loaded)) {
            Command::ResolveSecrets { root, lookups } => {
                assert_eq!(Some(&root), model.loaded().map(|c| &c.root));
                let found: Vec<(String, Vec<String>)> = lookups
                    .into_iter()
                    .map(|lookup| (lookup.name, lookup.keys))
                    .collect();
                assert_eq!(
                    found,
                    [
                        (
                            "oktaClientSecret".to_owned(),
                            vec![
                                "oktaClientSecret".to_owned(),
                                "OKTA_CLIENT_SECRET".to_owned()
                            ]
                        ),
                        (
                            "token".to_owned(),
                            vec!["token".to_owned(), "TOKEN".to_owned()]
                        ),
                    ]
                );
            }
            other => panic!("ResolveSecrets attendu, obtenu {other:?}"),
        }

        // Collection sans `vars:secret` ni `--secret` : aucune I/O demandée.
        let probe =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/collections/runner-probe");
        let mut model = Model::new(probe.clone(), (100, 30));
        let loaded = crate::collection::BruLoader.load(&probe);
        assert!(matches!(
            update(&mut model, Message::CollectionLoaded(loaded)),
            Command::None
        ));
    }

    #[test]
    fn secrets_resolved_for_another_root_is_ignored() {
        let mut model = local_model();
        update(
            &mut model,
            Message::SecretsResolved {
                root: PathBuf::from("/ailleurs"),
                resolved: vec![resolved_dotenv("token", "TOKEN", "x")],
            },
        );
        assert!(model.secrets.resolved.is_empty());
        let root = model.loaded().map(|c| c.root.clone()).expect("racine");
        update(
            &mut model,
            Message::SecretsResolved {
                root,
                resolved: vec![resolved_dotenv("token", "TOKEN", "x")],
            },
        );
        assert_eq!(model.secrets.resolved.len(), 1);
    }

    #[test]
    fn toggle_secrets_opens_closes_and_needs_a_collection() {
        let mut model = local_model();
        update(&mut model, Message::ToggleSecrets);
        assert_eq!(model.focus, Focus::Secrets);
        update(&mut model, Message::ToggleSecrets);
        assert_eq!(model.focus, Focus::Tree);
        update(&mut model, Message::ToggleSecrets);
        update(&mut model, Message::FocusTree);
        assert_eq!(model.focus, Focus::Tree);

        let mut loading = Model::new(PathBuf::from("/x"), (100, 30));
        update(&mut loading, Message::ToggleSecrets);
        assert_eq!(loading.focus, Focus::Tree);
    }

    #[test]
    fn secrets_navigation_is_bounded() {
        let mut model = local_model();
        model.secrets.mappings = vec![mapping("a", None), mapping("b", None)];
        update(&mut model, Message::ToggleSecrets);
        update(&mut model, Message::Up);
        assert_eq!(model.secrets.selected, 0);
        for _ in 0..5 {
            update(&mut model, Message::Down);
        }
        // `a`, `b`, puis `token` de `local`.
        assert_eq!(model.secrets.selected, 2);
    }

    #[test]
    fn typing_a_value_is_masked_and_takes_priority() {
        let mut model = local_model();
        let root = model.loaded().map(|c| c.root.clone()).expect("racine");
        update(
            &mut model,
            Message::SecretsResolved {
                root,
                resolved: vec![resolved_dotenv("token", "TOKEN", "from-env")],
            },
        );
        update(&mut model, Message::ToggleSecrets);
        update(&mut model, Message::Right);
        assert!(matches!(model.secrets.input, Some(Input::Value { .. })));
        assert_eq!(model.text_capture(), Some(TextCapture::SecretValue));
        type_text(&mut model, "s3cr3tX");
        update(&mut model, Message::SecretBackspace);
        assert!(!format!("{model:?}").contains("s3cr3t"));
        update(&mut model, Message::ConfirmSecretInput);
        assert!(model.secrets.input.is_none());
        let row = rows(&model).remove(0);
        assert_eq!(row.source, SecretSource::Typed);
        assert_eq!(
            row.value.map(|v| v.expose().to_owned()).as_deref(),
            Some("s3cr3t")
        );
        assert!(!format!("{model:?}").contains("s3cr3t"));

        // `d` oublie la saisie : la valeur `.env` revient.
        update(&mut model, Message::ForgetSecret);
        assert_eq!(
            rows(&model)[0].source,
            SecretSource::DotEnv {
                key: "TOKEN".into()
            }
        );
    }

    #[test]
    fn cancelling_or_confirming_empty_input() {
        let mut model = local_model();
        model.secrets.typed = vec![("token".into(), SecretString::new("keep"))];
        update(&mut model, Message::ToggleSecrets);
        update(&mut model, Message::Right);
        type_text(&mut model, "other");
        update(&mut model, Message::CancelSecretInput);
        assert_eq!(model.secrets.typed[0].1.expose(), "keep");

        // Une valeur validée vide vaut oubli.
        update(&mut model, Message::Right);
        update(&mut model, Message::ConfirmSecretInput);
        assert!(model.secrets.typed.is_empty());
    }

    #[test]
    fn adding_a_name_then_its_value() {
        let mut model = local_model();
        update(&mut model, Message::ToggleSecrets);
        update(&mut model, Message::AddSecret);
        assert_eq!(model.text_capture(), Some(TextCapture::SecretName));

        // Nom invalide puis doublon : refusés, la saisie reste ouverte.
        type_text(&mut model, "a b");
        update(&mut model, Message::ConfirmSecretInput);
        assert_eq!(model.secrets.error, Some(SecretError::InvalidName));
        for _ in 0..3 {
            update(&mut model, Message::SecretBackspace);
        }
        type_text(&mut model, "token");
        update(&mut model, Message::ConfirmSecretInput);
        assert_eq!(model.secrets.error, Some(SecretError::DuplicateName));
        for _ in 0..5 {
            update(&mut model, Message::SecretBackspace);
        }

        type_text(&mut model, "oktaClientSecret");
        update(&mut model, Message::ConfirmSecretInput);
        assert_eq!(model.secrets.error, None);
        assert_eq!(model.text_capture(), Some(TextCapture::SecretValue));
        // Abandon pendant la valeur : rien n'est ajouté.
        update(&mut model, Message::CancelSecretInput);
        assert_eq!(rows(&model).len(), 1);

        update(&mut model, Message::AddSecret);
        type_text(&mut model, "oktaClientSecret");
        update(&mut model, Message::ConfirmSecretInput);
        type_text(&mut model, "abc");
        update(&mut model, Message::ConfirmSecretInput);
        let all = rows(&model);
        assert_eq!(all.len(), 2);
        assert_eq!(all[1].name, "oktaClientSecret");
        assert_eq!(all[1].source, SecretSource::Typed);
        assert_eq!(model.secrets.selected, 1);

        // `d` sur un nom ajouté le retire de la liste.
        update(&mut model, Message::ForgetSecret);
        assert_eq!(rows(&model).len(), 1);
        assert_eq!(model.secrets.selected, 0);
    }

    #[test]
    fn run_transmits_resolved_secrets() {
        // Cas oktaClientSecret : `--secret` sans clé, valeur du `.env`.
        let mut model = local_model();
        model.secrets.mappings = vec![mapping("oktaClientSecret", None)];
        let root = model.loaded().map(|c| c.root.clone()).expect("racine");
        update(
            &mut model,
            Message::SecretsResolved {
                root,
                resolved: vec![
                    resolved_dotenv("oktaClientSecret", "OKTA_CLIENT_SECRET", "abc"),
                    resolved_dotenv("token", "TOKEN", "t"),
                ],
            },
        );
        select(&mut model, "simple-get.bru");
        let command = update(&mut model, Message::RunSelected);
        if let Command::StartRun { request, .. } = &command {
            assert_eq!(request.env.as_deref(), Some("local"));
        }
        assert_eq!(
            env_vars_of(command),
            [
                ("oktaClientSecret".to_owned(), "abc".to_owned()),
                ("token".to_owned(), "t".to_owned())
            ]
        );

        // Aucune variable secrète : aucune surcharge.
        let mut model = loaded_model((100, 30));
        select(&mut model, "simple-get.bru");
        assert!(env_vars_of(update(&mut model, Message::RunSelected)).is_empty());
    }

    #[test]
    fn replay_uses_values_current_at_launch() {
        let mut model = local_model();
        model.secrets.acknowledged.insert("local".into());
        model.focus = Focus::History;
        model.history.push_back(HistoryEntry {
            started_at: std::time::SystemTime::now(),
            target: PathBuf::from("simple-get.bru"),
            recursive: false,
            outcome: HistoryOutcome::Completed {
                total: 1,
                failed: 0,
                duration_secs: 0.1,
            },
        });
        assert!(env_vars_of(update(&mut model, Message::RunSelected)).is_empty());

        model.secrets.typed = vec![("token".into(), SecretString::new("typed"))];
        assert_eq!(
            env_vars_of(update(&mut model, Message::RunSelected)),
            [("token".to_owned(), "typed".to_owned())]
        );
    }

    #[test]
    fn launch_proposes_input_for_missing_declared_secrets() {
        let mut model = local_model();
        model.secrets.mappings = vec![mapping("other", None)];
        select(&mut model, "simple-get.bru");
        assert!(matches!(
            update(&mut model, Message::RunSelected),
            Command::None
        ));
        assert_eq!(model.focus, Focus::Secrets);
        assert_eq!(
            model.secrets.pending_run,
            Some((PathBuf::from("simple-get.bru"), false))
        );
        // Première variable déclarée non fournie : `token`, après `other`.
        assert_eq!(model.secrets.selected, 1);

        update(&mut model, Message::Right);
        type_text(&mut model, "v");
        update(&mut model, Message::ConfirmSecretInput);
        let command = update(&mut model, Message::RunSelected);
        assert_eq!(env_vars_of(command), [("token".to_owned(), "v".to_owned())]);
        assert_eq!(model.focus, Focus::Tree);
        assert!(model.secrets.pending_run.is_none());
        assert!(model.secrets.acknowledged.contains("local"));
    }

    #[test]
    fn escape_abandons_pending_run_and_acknowledges() {
        let mut model = local_model();
        select(&mut model, "simple-get.bru");
        update(&mut model, Message::RunSelected);
        assert_eq!(model.focus, Focus::Secrets);
        update(&mut model, Message::FocusTree);
        assert_eq!(model.focus, Focus::Tree);
        assert!(model.secrets.pending_run.is_none());
        // Lancement suivant : direct, sans surcharge pour `token`.
        assert!(env_vars_of(update(&mut model, Message::RunSelected)).is_empty());

        // `r` dans un panneau ouvert à la main : rien à lancer.
        update(&mut model, Message::ToggleSecrets);
        assert!(matches!(
            update(&mut model, Message::RunSelected),
            Command::None
        ));
    }

    #[test]
    fn no_proposal_when_found_mapped_only_or_run_active() {
        // Recherche automatique réussie : lancement direct.
        let mut model = local_model();
        let root = model.loaded().map(|c| c.root.clone()).expect("racine");
        update(
            &mut model,
            Message::SecretsResolved {
                root,
                resolved: vec![resolved_dotenv("token", "TOKEN", "t")],
            },
        );
        select(&mut model, "simple-get.bru");
        assert!(matches!(
            update(&mut model, Message::RunSelected),
            Command::StartRun { .. }
        ));

        // Nom seulement déclaré par `--secret` et non fourni : pas de panneau.
        let mut model = loaded_model((100, 30));
        model.current_environment = Some("staging".into());
        model.secrets.mappings = vec![mapping("missing", None)];
        select(&mut model, "simple-get.bru");
        assert!(matches!(
            update(&mut model, Message::RunSelected),
            Command::StartRun { .. }
        ));

        // Exécution en cours : ni exécution ni panneau.
        let mut model = local_model();
        select(&mut model, "simple-get.bru");
        model.run.active = Some(ActiveRun {
            id: runner::RunId(1),
            target: PathBuf::from("x"),
            recursive: false,
            handle: None,
        });
        assert!(matches!(
            update(&mut model, Message::RunSelected),
            Command::None
        ));
        assert_eq!(model.focus, Focus::Tree);
    }

    #[test]
    fn collection_reload_resets_proposals_but_keeps_typed_values() {
        use crate::collection::CollectionLoader;
        let mut model = local_model();
        model.secrets.typed = vec![("token".into(), SecretString::new("kept"))];
        model.secrets.acknowledged.insert("local".into());
        model.secrets.pending_run = Some((PathBuf::from("x"), false));
        model.focus = Focus::Secrets;
        let reloaded = crate::collection::BruLoader.load(&model.source.clone());
        update(&mut model, Message::CollectionLoaded(reloaded));
        assert_eq!(model.focus, Focus::Tree);
        assert!(model.secrets.acknowledged.is_empty());
        assert!(model.secrets.pending_run.is_none());
        assert_eq!(model.secrets.typed.len(), 1);
    }
}
