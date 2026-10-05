//! Transitions du modèle.
//!
//! `update` est la seule fonction qui modifie le modèle. Elle ne fait
//! aucune I/O : le chargement et le rendu sont pilotés par la boucle.

use std::ops::RangeInclusive;
use std::time::{Duration, Instant};

use super::filter::{FilterState, evaluate};
use super::message::{InputKey, Message, MouseInput, MouseKind, TextCapture};
use super::model::{
    CampaignFailure, CampaignSummary, CollectionState, DetailSelection, Drag, DragPanel,
    EditSession, EditState, EditableField, EnvironmentEditSession, EnvironmentEditState, Exit,
    Focus, HistoryEntry, HistoryOutcome, InputTarget, Model, ResponseTab, SecretError, SecretInput,
    StatusMessage, all_rows, environment_name_at, field_enabled, field_value,
    field_value_committed, secret_env_vars, secret_lookups, secret_rows, section_entries,
    selected_response_body, tree_node_at, visible_rows,
};
use super::search::{SearchScope, SearchState, find_detail_match, find_tree_match};
use super::text_input::TextInput;
use super::view::detail::{
    field_at_line, plain_lines, request_text_and_fields, response_plain_lines,
};
use super::view::hit::{DragRow, Hit, drag_row, hit_test};
use super::view::{
    detail::{detail_text, response_text},
    environment_edit_popup_area, inner, layout_for,
};
use crate::collection::TreeNode;
use crate::runner::report::ResponseStatus;
use crate::runner::{RunRequest, SecretString};
use crate::secrets::{SecretLookup, is_valid_name};
use crate::writer::{EntrySection, FieldEdit, FileStamp};
use ratatui::layout::Rect;
use ratatui::text::Line;

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
    /// Sauvegarder les modifications d'un environnement sur disque ; la boucle
    /// l'exécute dans `spawn_blocking` via `writer::write_environment` et renvoie
    /// le résultat à `update` via `Message::EnvironmentSaved`.
    SaveEnvironment {
        path: std::path::PathBuf,
        ast: crate::collection::BruFile,
        stamp: crate::writer::FileStamp,
        edits: Vec<crate::writer::EnvironmentVarEdit>,
    },
    /// Résoudre les variables secrètes depuis `<root>/.env` et le shell ;
    /// la boucle l'exécute dans `spawn_blocking` et renvoie le résultat à
    /// `update` via `Message::SecretsResolved`.
    ResolveSecrets {
        root: std::path::PathBuf,
        lookups: Vec<SecretLookup>,
    },
    /// Activer (`true`) ou désactiver la capture souris ; la boucle
    /// l'exécute de façon synchrone et renvoie le résultat à `update` via
    /// `Message::MouseCaptureChanged`.
    SetMouseCapture(bool),
    /// Ouvrir le corps de réponse dans un éditeur externe ; la boucle
    /// l'exécute de façon bloquante (`D1`).
    OpenInEditor {
        program: String,
        text: String,
    },
}

/// Applique un message au modèle.
pub fn update(model: &mut Model, message: Message) -> Command {
    if let Some(confirm) = model.confirm {
        return match message {
            Message::ConfirmYes | Message::Yank | Message::Right | Message::Enter => {
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
            if model.editing.as_ref().is_some_and(EditSession::has_unsaved) {
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
        Message::Add => {
            if model.focus == Focus::Secrets {
                model.secrets.error = None;
                model.secrets.input = Some(SecretInput::Name(String::new()));
            } else if model.focus == Focus::Detail {
                add_entry(model);
            } else if model.focus == Focus::EnvironmentPicker
                && let Some(session) = &mut model.environment_editing
            {
                session.begin_add();
            }
            Command::None
        }
        Message::Delete => {
            if model.focus == Focus::Secrets {
                forget_secret(model);
            } else if model.focus == Focus::Detail {
                delete_entry(model);
            } else if model.focus == Focus::EnvironmentPicker
                && let Some(session) = &mut model.environment_editing
            {
                session.remove_current();
            }
            Command::None
        }
        Message::Rename => {
            if model.focus == Focus::Detail {
                start_rename(model);
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
        Message::FocusTree
            if model.focus == Focus::EnvironmentPicker && model.environment_editing.is_some() =>
        {
            model.environment_editing = None;
            Command::None
        }
        Message::FocusTree => {
            if let Some(session) = &model.editing {
                model.last_status = None;
                if session.has_unsaved() {
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
        Message::ToggleCampaign => {
            if model.loaded().is_some() {
                model.focus = match model.focus {
                    Focus::Campaign => Focus::Tree,
                    _ => Focus::Campaign,
                };
            }
            Command::None
        }
        Message::NextFailedRequest => {
            navigate_failed_request(model, true);
            Command::None
        }
        Message::PreviousFailedRequest => {
            navigate_failed_request(model, false);
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
            if model.focus == Focus::Detail {
                start_edit(model);
            } else if model.focus == Focus::EnvironmentPicker {
                if let Some(session) = &mut model.environment_editing {
                    session.begin_input();
                } else {
                    open_environment_editing(model, model.environment_selected);
                }
            }
            Command::None
        }
        Message::Enter => {
            if model.focus == Focus::Detail {
                enter_in_detail(model);
                Command::None
            } else if model.focus == Focus::EnvironmentPicker {
                navigate_environment_picker(model, Message::Enter);
                Command::None
            } else {
                update(model, Message::Right)
            }
        }
        Message::MoveFieldCursor(delta) => {
            move_field_cursor(model, delta);
            Command::None
        }
        Message::ToggleField => {
            toggle_field(model);
            Command::None
        }
        Message::InputKey(key) => {
            input_key(model, key);
            Command::None
        }
        Message::ValidateInput => {
            if model.environment_editing.is_some() {
                environment_commit_or_advance(model);
            } else {
                validate_input(model, false);
            }
            Command::None
        }
        Message::CancelInput => {
            cancel_input(model);
            Command::None
        }
        Message::SaveEdit => {
            if model.focus == Focus::EnvironmentPicker
                || (model.environment_editing.is_some() && model.editing.is_none())
            {
                if let Some(session) = &mut model.environment_editing {
                    session.commit_input();
                }
                save_environment(model)
            } else {
                match validate_input(model, true) {
                    Validation::Done => save_edit(model),
                    Validation::Continued | Validation::Refused => Command::None,
                }
            }
        }
        Message::EditSaved { path, result } => {
            edit_saved(model, path, result);
            Command::None
        }
        Message::EnvironmentSaved { path, result } => {
            environment_saved(model, path, result);
            Command::None
        }
        Message::OpenResponseInEditor => match selected_response_body(model) {
            Some(text) => Command::OpenInEditor {
                program: resolve_editor(),
                text,
            },
            None => {
                model.last_status = Some(StatusMessage::NoResponseBody);
                Command::None
            }
        },
        Message::Mouse(input) => {
            mouse(model, input);
            Command::None
        }
        Message::ToggleMouseCapture => Command::SetMouseCapture(!model.mouse.capture),
        Message::MouseCaptureChanged { enabled, result } => {
            match result {
                Ok(()) => {
                    model.mouse.capture = enabled;
                    if !enabled {
                        model.mouse.drag = None;
                    }
                    model.last_status = Some(StatusMessage::MouseCapture(enabled));
                }
                Err(reason) => {
                    model.last_status = Some(StatusMessage::MouseCaptureError(reason));
                }
            }
            Command::None
        }
        navigation => {
            match model.focus {
                Focus::Tree => navigate_tree(model, navigation),
                Focus::Detail => match model.editing.as_ref().map(|s| &s.state) {
                    Some(EditState::FieldSelect) => match navigation {
                        Message::Up => move_field_cursor(model, -1),
                        Message::Down => move_field_cursor(model, 1),
                        other => scroll_detail(model, other),
                    },
                    Some(EditState::MethodPicker { .. }) => match navigation {
                        Message::Up => move_method_picker(model, -1),
                        Message::Down => move_method_picker(model, 1),
                        _ => {}
                    },
                    _ => scroll_detail(model, navigation),
                },
                Focus::Response => match navigation {
                    Message::Left => previous_response_tab(model),
                    Message::Right => next_response_tab(model),
                    other => scroll_response(model, other),
                },
                Focus::Diagnostics => navigate_diagnostics(model, navigation),
                Focus::History => navigate_history(model, navigation),
                Focus::EnvironmentPicker => navigate_environment_picker(model, navigation),
                Focus::Secrets => navigate_secrets(model, navigation),
                Focus::Campaign => navigate_campaign(model, navigation),
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
        Focus::Diagnostics | Focus::EnvironmentPicker | Focus::Secrets | Focus::Campaign => None,
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

    if active.recursive
        && let crate::runner::RunOutcome::Completed { ref report, .. } = event.outcome
    {
        let results: Vec<_> = report
            .iterations()
            .iter()
            .flat_map(|it| &it.results)
            .collect();
        let total = results.len();
        let failed = results.iter().filter(|r| r.is_failure()).count();
        let skipped = results
            .iter()
            .filter(|r| r.skipped && !r.is_failure())
            .count();
        let passed = total.saturating_sub(failed + skipped);
        let duration_secs: f64 = results.iter().map(|r| r.run_duration).sum();

        let failures = results
            .iter()
            .filter(|r| r.is_failure())
            .map(|r| CampaignFailure {
                path: std::path::PathBuf::from(&r.test.filename),
                name: r.name.clone(),
                http_code: match r.response.status {
                    ResponseStatus::Http(code) => Some(code),
                    _ => None,
                },
                reason: extract_failure_reason(r),
            })
            .collect();

        model.campaign = Some(CampaignSummary {
            target: active.target.clone(),
            total,
            passed,
            failed,
            skipped,
            duration_secs,
            failures,
        });
        model.campaign_selected = 0;

        let failures_text = if failed <= 1 {
            format!("{failed} échec")
        } else {
            format!("{failed} échecs")
        };
        let status_text = format!(
            "Campagne {} : {passed}/{total} réussis · {failures_text} — C pour le détail",
            active.target.display()
        );
        model.last_status = Some(StatusMessage::CampaignFinished(status_text));
    }

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

/// Détermine la première raison d'échec d'une requête exécutée.
fn extract_failure_reason(result: &crate::runner::report::RequestResult) -> String {
    if let Some(error) = &result.error {
        return error.clone();
    }
    for assertion in &result.assertion_results {
        if assertion.status.is_failure() {
            let expr = format!(
                "{} {} {}",
                assertion.lhs_expr, assertion.operator, assertion.rhs_expr
            );
            return match &assertion.error {
                Some(err) => format!("{expr} : {err}"),
                None => expr,
            };
        }
    }
    for test in result
        .test_results
        .iter()
        .chain(&result.pre_request_test_results)
        .chain(&result.post_response_test_results)
    {
        if test.status.is_failure() {
            return match &test.error {
                Some(err) => format!("{} : {err}", test.description),
                None => test.description.clone(),
            };
        }
    }
    "échec d'exécution".to_owned()
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
    finish_tree_selection(model, before, previous_path);
}

/// Sélectionne la ligne `index` de l'arbre, avec les mêmes effets et
/// restrictions que la navigation au clavier (clic dans l'arbre,
/// `mouse-support`).
fn select_row(model: &mut Model, index: usize) {
    if index >= model.tree.rows.len() {
        return;
    }
    let previous_path = model.selected_node().map(|node| node.path().to_path_buf());
    let before = model.tree.selected;
    model.tree.selected = index;
    finish_tree_selection(model, before, previous_path);
}

/// Suite commune à tout déplacement de la sélection de l'arbre : refus si
/// la session porte des modifications non enregistrées et que le nœud
/// change, sinon remise à zéro de l'état d'affichage du nœud.
fn finish_tree_selection(
    model: &mut Model,
    before: usize,
    previous_path: Option<std::path::PathBuf>,
) {
    if model.tree.selected != before {
        let changed_node =
            model.selected_node().map(|node| node.path().to_path_buf()) != previous_path;
        if changed_node && selection_locked(model) {
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

/// Ouvre une session d'édition sur la requête sélectionnée, dans l'état
/// Sélection de champ.
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
    model.editing = Some(EditSession::new(
        request.path.clone(),
        stamp,
        request.view.clone(),
    ));
    model.last_status = None;
    scroll_edit_into_view(model);
}

/// `Entrée` dans le détail : ouvre la session, ou commence la saisie du
/// champ sous le curseur (`improve-direct-editing`, D3).
fn enter_in_detail(model: &mut Model) {
    match &model.editing {
        None => start_edit(model),
        Some(session) if session.state == EditState::FieldSelect => begin_input(model),
        Some(session) if matches!(session.state, EditState::MethodPicker { .. }) => {
            confirm_method_picker(model);
        }
        Some(_) => {}
    }
}

/// Déplace la présélection du sélecteur de méthode, bornée aux
/// extrémités de `ast::METHODS` (`add-method-editing`).
fn move_method_picker(model: &mut Model, delta: i8) {
    let Some(session) = &mut model.editing else {
        return;
    };
    let EditState::MethodPicker { selected } = &mut session.state else {
        return;
    };
    let count = crate::collection::ast::METHODS.len();
    if delta < 0 {
        *selected = selected.saturating_sub(1);
    } else if delta > 0 {
        *selected = (*selected + 1).min(count - 1);
    }
}

/// Valide la présélection du sélecteur de méthode : applique le
/// changement si elle diffère de la méthode courante, puis referme le
/// sélecteur vers l'état Sélection de champ (`add-method-editing`).
fn confirm_method_picker(model: &mut Model) {
    let Some(session) = &model.editing else {
        return;
    };
    let EditState::MethodPicker { selected } = session.state else {
        return;
    };
    let Some(&method) = crate::collection::ast::METHODS.get(selected) else {
        return;
    };
    if !method.eq_ignore_ascii_case(&session.preview.method) {
        try_commit(model, FieldEdit::Method(method.to_owned()));
    }
    close_input(model);
}

/// Déplace le curseur de champ en sélection de champ.
fn move_field_cursor(model: &mut Model, delta: i8) {
    let Some(session) = &mut model.editing else {
        return;
    };
    if session.state != EditState::FieldSelect {
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
    model.last_status = None;
    scroll_edit_into_view(model);
}

/// Bascule l'état activé/désactivé de l'en-tête ou du paramètre courant.
fn toggle_field(model: &mut Model) {
    let Some(session) = &model.editing else {
        return;
    };
    if session.state != EditState::FieldSelect {
        return;
    }
    let Some(field) = session.current_field() else {
        return;
    };
    let enabled = !field_enabled(session, &field);
    let edit = match field {
        EditableField::HeaderValue(index) => FieldEdit::HeaderEnabled { index, enabled },
        EditableField::QueryParamValue(index) => FieldEdit::QueryParamEnabled { index, enabled },
        EditableField::PathParamValue(index) => FieldEdit::PathParamEnabled { index, enabled },
        EditableField::Method
        | EditableField::Url
        | EditableField::AddRow(_)
        | EditableField::BodyText => return,
    };
    try_commit(model, edit);
}

/// `Entrée` en sélection de champ : saisie de la valeur du champ sous le
/// curseur, ou ajout d'une entrée sur une ligne « + Ajouter ».
fn begin_input(model: &mut Model) {
    let Some(session) = &model.editing else {
        return;
    };
    if session.state != EditState::FieldSelect {
        return;
    }
    let Some(field) = session.current_field() else {
        return;
    };
    if let EditableField::AddRow(section) = field {
        start_add(model, section);
        return;
    }
    if field == EditableField::Method {
        open_method_picker(model);
        return;
    }
    let input = TextInput::new(
        field_value(session, &field),
        field == EditableField::BodyText,
    );
    open_input(model, input, InputTarget::Field);
}

/// Passe la session en saisie avec `target`.
fn open_input(model: &mut Model, input: TextInput, target: InputTarget) {
    if let Some(session) = &mut model.editing {
        session.state = EditState::Input(input);
        session.target = target;
        session.hscroll = 0;
    }
    model.last_status = None;
    scroll_edit_into_view(model);
}

/// Ouvre le sélecteur de méthode, la méthode courante de la session
/// présélectionnée (`add-method-editing`).
fn open_method_picker(model: &mut Model) {
    let Some(session) = &mut model.editing else {
        return;
    };
    let current = session.preview.method.as_str();
    let selected = crate::collection::ast::METHODS
        .iter()
        .position(|candidate| current.eq_ignore_ascii_case(candidate))
        .unwrap_or(0);
    session.state = EditState::MethodPicker { selected };
    model.last_status = None;
    scroll_edit_into_view(model);
}

/// Revient à la sélection de champ, sans rien valider.
fn close_input(model: &mut Model) {
    if let Some(session) = &mut model.editing {
        session.state = EditState::FieldSelect;
        session.target = InputTarget::Field;
        session.hscroll = 0;
    }
    scroll_edit_into_view(model);
}

/// `a` en sélection de champ, ou `Entrée` sur une ligne d'ajout : commence
/// la saisie de la clé d'une nouvelle entrée de `section`. Le curseur de
/// champ se place sur la ligne d'ajout, où s'affiche l'entrée provisoire.
fn start_add(model: &mut Model, section: EntrySection) {
    let Some(session) = &mut model.editing else {
        return;
    };
    if session.state != EditState::FieldSelect {
        return;
    }
    let return_cursor = session.cursor;
    if let Some(position) = session
        .fields
        .iter()
        .position(|f| *f == EditableField::AddRow(section))
    {
        session.cursor = position;
    }
    open_input(
        model,
        TextInput::new("", false),
        InputTarget::NewKey {
            section,
            return_cursor,
        },
    );
}

/// `a` : ajout dans la section de l'entrée ou de la ligne d'ajout sous le
/// curseur ; sans effet sur l'URL et le corps.
fn add_entry(model: &mut Model) {
    let Some(section) = model
        .editing
        .as_ref()
        .filter(|s| s.state == EditState::FieldSelect)
        .and_then(EditSession::current_field)
        .and_then(|field| field.section())
    else {
        return;
    };
    start_add(model, section);
}

/// `d` : supprime l'entrée sous le curseur, sans confirmation ; le curseur
/// garde son indice, borné à la dernière position.
fn delete_entry(model: &mut Model) {
    let Some((section, index)) = model
        .editing
        .as_ref()
        .filter(|s| s.state == EditState::FieldSelect)
        .and_then(EditSession::current_field)
        .and_then(|field| field.entry())
    else {
        return;
    };
    try_commit(model, FieldEdit::RemoveEntry { section, index });
    scroll_edit_into_view(model);
}

/// `c` : commence la saisie de la clé de l'entrée sous le curseur,
/// pré-remplie avec la clé actuelle.
fn start_rename(model: &mut Model) {
    let Some(session) = model
        .editing
        .as_ref()
        .filter(|s| s.state == EditState::FieldSelect)
    else {
        return;
    };
    let Some((section, index)) = session.current_field().and_then(|field| field.entry()) else {
        return;
    };
    let Some(key) = section_entries(&session.preview, section)
        .get(index)
        .map(|kv| kv.key.clone())
    else {
        return;
    };
    open_input(
        model,
        TextInput::new(&key, false),
        InputTarget::RenameKey { section, index },
    );
}

/// Issue d'une validation de saisie.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Validation {
    /// Aucune saisie en cours, ou saisie validée : retour en sélection.
    Done,
    /// Clé d'ajout validée : la saisie continue sur la valeur.
    Continued,
    /// Modification refusée : la saisie reste ouverte.
    Refused,
}

/// Valide la saisie en cours (`Tab`, `Entrée` sur une ligne, `Ctrl+S`),
/// selon sa cible. `key_only` : à l'étape de la clé d'un ajout, ajoute
/// l'entrée avec une valeur vide au lieu de passer à la valeur.
fn validate_input(model: &mut Model, key_only: bool) -> Validation {
    let Some(session) = &model.editing else {
        return Validation::Done;
    };
    let EditState::Input(input) = &session.state else {
        return Validation::Done;
    };
    let text = input.text().to_owned();
    match session.target.clone() {
        InputTarget::Field => {
            let Some(field) = session.current_field() else {
                close_input(model);
                return Validation::Done;
            };
            if text != field_value_committed(session, &field)
                && let Some(edit) = value_edit(field, text)
                && !try_commit(model, edit)
            {
                return Validation::Refused;
            }
            close_input(model);
            Validation::Done
        }
        InputTarget::NewKey {
            section,
            return_cursor,
        } => {
            let add = FieldEdit::AddEntry {
                section,
                key: text.clone(),
                value: String::new(),
                enabled: true,
            };
            if key_only {
                if !try_commit(model, add) {
                    return Validation::Refused;
                }
                finish_add(model, section);
                return Validation::Done;
            }
            if let Err(error) = check_edit(model, add) {
                refuse(model, &error);
                return Validation::Refused;
            }
            open_input(
                model,
                TextInput::new("", false),
                InputTarget::NewValue {
                    section,
                    key: text,
                    return_cursor,
                },
            );
            Validation::Continued
        }
        InputTarget::NewValue { section, key, .. } => {
            let add = FieldEdit::AddEntry {
                section,
                key,
                value: text,
                enabled: true,
            };
            if !try_commit(model, add) {
                return Validation::Refused;
            }
            finish_add(model, section);
            Validation::Done
        }
        InputTarget::RenameKey { section, index } => {
            let unchanged = section_entries(&session.preview, section)
                .get(index)
                .is_some_and(|kv| kv.key == text);
            if !unchanged
                && !try_commit(
                    model,
                    FieldEdit::RenameKey {
                        section,
                        index,
                        key: text,
                    },
                )
            {
                return Validation::Refused;
            }
            close_input(model);
            Validation::Done
        }
    }
}

/// Termine un ajout réussi : sélection de champ, curseur sur la nouvelle
/// entrée (la dernière de sa section).
fn finish_add(model: &mut Model, section: EntrySection) {
    if let Some(session) = &mut model.editing {
        let count = section_entries(&session.preview, section).len();
        let field = match section {
            EntrySection::Headers => EditableField::HeaderValue(count.saturating_sub(1)),
            EntrySection::QueryParams => EditableField::QueryParamValue(count.saturating_sub(1)),
            EntrySection::PathParams => EditableField::PathParamValue(count.saturating_sub(1)),
        };
        if let Some(position) = session.fields.iter().position(|f| *f == field) {
            session.cursor = position;
        }
    }
    close_input(model);
}

/// Modification de la valeur d'un champ.
fn value_edit(field: EditableField, value: String) -> Option<FieldEdit> {
    Some(match field {
        EditableField::Url => FieldEdit::Url(value),
        EditableField::HeaderValue(index) => FieldEdit::HeaderValue { index, value },
        EditableField::QueryParamValue(index) => FieldEdit::QueryParamValue { index, value },
        EditableField::PathParamValue(index) => FieldEdit::PathParamValue { index, value },
        EditableField::BodyText => FieldEdit::BodyText(value),
        EditableField::Method | EditableField::AddRow(_) => return None,
    })
}

/// Annule la saisie en cours : `pending` n'est pas touché, le champ
/// reprend la valeur qu'il avait au début de la saisie ; un ajout est
/// abandonné en entier et le curseur revient d'où il était parti.
fn cancel_input(model: &mut Model) {
    if let Some(session) = &mut model.environment_editing {
        session.cancel_input();
        model.last_status = None;
        return;
    }
    let Some(session) = &mut model.editing else {
        return;
    };
    match session.state {
        EditState::Input(_) => {
            if let InputTarget::NewKey { return_cursor, .. }
            | InputTarget::NewValue { return_cursor, .. } = session.target
            {
                session.cursor = return_cursor.min(session.fields.len().saturating_sub(1));
            }
        }
        EditState::MethodPicker { .. } => {}
        EditState::FieldSelect => return,
    }
    model.last_status = None;
    close_input(model);
}

/// Touche d'édition du tampon pendant une saisie.
fn input_key(model: &mut Model, key: InputKey) {
    if model.environment_editing.is_some() {
        environment_input_key(model, key);
        return;
    }
    let Some(session) = &mut model.editing else {
        return;
    };
    let EditState::Input(input) = &mut session.state else {
        return;
    };
    match key {
        InputKey::Char(c) => input.insert(c),
        InputKey::Backspace => input.backspace(),
        InputKey::Delete => input.delete(),
        InputKey::Left => input.left(),
        InputKey::Right => input.right(),
        InputKey::Up => input.up(),
        InputKey::Down => input.down(),
        InputKey::Home => input.home(),
        InputKey::End => input.end(),
        InputKey::Enter if input.is_multiline() => input.newline(),
        InputKey::Enter => {
            validate_input(model, false);
            return;
        }
    }
    model.last_status = None;
    scroll_edit_into_view(model);
}

/// Touche d'édition de tampon pendant une saisie du popup d'environnement
/// (valeur, clé ou valeur d'un ajout en cours) — `Entrée` est traitée à
/// part, selon l'état courant (`add-environment-entry-management`).
fn environment_input_key(model: &mut Model, key: InputKey) {
    if matches!(key, InputKey::Enter) {
        environment_commit_or_advance(model);
        model.last_status = None;
        return;
    }
    let Some(session) = &mut model.environment_editing else {
        return;
    };
    let Some(input) = session.state.text_input_mut() else {
        return;
    };
    match key {
        InputKey::Char(c) => input.insert(c),
        InputKey::Backspace => input.backspace(),
        InputKey::Delete => input.delete(),
        InputKey::Left => input.left(),
        InputKey::Right => input.right(),
        InputKey::Up => input.up(),
        InputKey::Down => input.down(),
        InputKey::Home => input.home(),
        InputKey::End => input.end(),
        InputKey::Enter => {}
    }
    model.last_status = None;
}

/// `Entrée` ou `Tab` (`Message::ValidateInput`) pendant une saisie du
/// popup d'environnement : valide selon l'état courant — une valeur
/// existante, la clé d'un ajout (contre `bru-writer`, comme
/// `check_edit` pour une requête), ou la valeur d'un ajout.
fn environment_commit_or_advance(model: &mut Model) {
    let Some(session) = &model.environment_editing else {
        return;
    };
    match &session.state {
        EnvironmentEditState::Input(_) => {
            if let Some(session) = &mut model.environment_editing {
                session.commit_input();
            }
        }
        EnvironmentEditState::AddingKey(input) => {
            let key = input.text().to_owned();
            commit_environment_add_key(model, key);
        }
        EnvironmentEditState::AddingValue { .. } => {
            if let Some(session) = &mut model.environment_editing {
                session.finish_add();
            }
        }
        EnvironmentEditState::Select => {}
    }
}

/// Valide la clé d'un ajout en cours en l'essayant contre `bru-writer`
/// (`writer::environment::resolve`, sans écriture disque) ; avance vers
/// la saisie de la valeur si elle est acceptée, sinon affiche le refus
/// et laisse la saisie de la clé active — même mécanisme que
/// `check_edit`/`try_commit` pour l'ajout d'un en-tête de requête.
fn commit_environment_add_key(model: &mut Model, key: String) {
    let Some(session) = &model.environment_editing else {
        return;
    };
    let session_path = session.path.clone();
    let mut candidate = session.pending.clone();
    candidate.push(crate::writer::EnvironmentVarEdit::Add {
        key: key.clone(),
        value: String::new(),
        enabled: true,
    });
    let Some(collection) = model.loaded() else {
        return;
    };
    let ast = collection.environments.iter().find_map(|e| match e {
        Ok(env) if env.path == session_path => env.ast.as_ref(),
        _ => None,
    });
    let Some(ast) = ast else {
        return;
    };
    if let Err(error) = crate::writer::environment::resolve(ast, &candidate) {
        model.last_status = Some(StatusMessage::EditRefused(error.to_string()));
        return;
    }
    model.last_status = None;
    if let Some(session) = &mut model.environment_editing {
        session.advance_add_to_value(key);
    }
}

/// Journal de la session augmenté de `edit`, avec une seule coalescence :
/// deux modifications de valeur consécutives du même champ.
fn with_edit(pending: &[FieldEdit], edit: FieldEdit) -> Vec<FieldEdit> {
    let mut edits = pending.to_vec();
    let same_value_target = match (edits.last(), &edit) {
        (Some(FieldEdit::Url(_)), FieldEdit::Url(_))
        | (Some(FieldEdit::BodyText(_)), FieldEdit::BodyText(_)) => true,
        (
            Some(FieldEdit::HeaderValue { index: a, .. }),
            FieldEdit::HeaderValue { index: b, .. },
        )
        | (
            Some(FieldEdit::QueryParamValue { index: a, .. }),
            FieldEdit::QueryParamValue { index: b, .. },
        )
        | (
            Some(FieldEdit::PathParamValue { index: a, .. }),
            FieldEdit::PathParamValue { index: b, .. },
        ) => a == b,
        _ => false,
    };
    if same_value_target {
        edits.pop();
    }
    edits.push(edit);
    edits
}

/// Aperçu de la requête éditée avec `edit` ajouté au journal.
fn check_edit(
    model: &Model,
    edit: FieldEdit,
) -> Result<(Vec<FieldEdit>, crate::collection::RequestView), crate::writer::EditError> {
    let session = model
        .editing
        .as_ref()
        .ok_or(crate::writer::EditError::Unreadable)?;
    let ast = match model.selected_node() {
        Some(TreeNode::Request(request)) if request.path == session.path => request.ast.as_ref(),
        _ => None,
    }
    .ok_or(crate::writer::EditError::Unreadable)?;
    let edits = with_edit(&session.pending, edit);
    let view = crate::writer::preview(ast, &edits)?;
    Ok((edits, view))
}

/// Affiche le refus d'une modification, sans jamais citer le texte saisi.
fn refuse(model: &mut Model, error: &crate::writer::EditError) {
    model.last_status = Some(super::model::StatusMessage::EditRefused(error.to_string()));
}

/// Ajoute `edit` au journal s'il est accepté par `bru-writer` : aperçu et
/// positions recalculés, session marquée modifiée. Sinon, message de refus
/// et journal inchangé.
fn try_commit(model: &mut Model, edit: FieldEdit) -> bool {
    match check_edit(model, edit) {
        Ok((edits, view)) => {
            if let Some(session) = &mut model.editing {
                session.pending = edits;
                session.set_preview(view);
                session.dirty = true;
            }
            model.last_status = None;
            true
        }
        Err(error) => {
            refuse(model, &error);
            false
        }
    }
}

/// Fait défiler le détail pour que le champ sous le curseur (en sélection)
/// ou la ligne du curseur de texte (en saisie) soit visible, et ajuste le
/// décalage horizontal de la valeur en saisie (`improve-direct-editing`,
/// D7).
fn scroll_edit_into_view(model: &mut Model) {
    let Some(areas) = layout_for(model.size) else {
        return;
    };
    let area = inner(areas.detail);
    let (height, width) = (area.height.max(1), area.width);
    let Some(session) = &model.editing else {
        return;
    };
    let Some(TreeNode::Request(request)) = model.selected_node() else {
        return;
    };
    if request.path != session.path {
        return;
    }
    let Some(field) = session.current_field() else {
        return;
    };
    let (_, field_lines, _) = request_text_and_fields(request, Some(session));
    let Some(location) = field_lines.iter().find(|l| l.field == field) else {
        return;
    };
    let (target_line, hscroll) = match &session.state {
        EditState::FieldSelect | EditState::MethodPicker { .. } => (location.line, 0),
        EditState::Input(input) => {
            let (line, _) = input.cursor_line_col();
            let column = u16::try_from(Line::raw(input.text_before_cursor_on_line()).width())
                .unwrap_or(u16::MAX);
            let available = width
                .saturating_sub(u16::try_from(location.prefix_width).unwrap_or(u16::MAX))
                .max(1);
            let mut hscroll = session.hscroll;
            if column < hscroll {
                hscroll = column;
            } else if column >= hscroll.saturating_add(available) {
                hscroll = column - available + 1;
            }
            (location.line + line, hscroll)
        }
    };
    let target = u16::try_from(target_line).unwrap_or(u16::MAX);
    let mut scroll = model.detail_scroll;
    if target < scroll {
        scroll = target;
    } else if target >= scroll.saturating_add(height) {
        scroll = target - height + 1;
    }
    model.detail_scroll = scroll;
    if let Some(session) = &mut model.editing {
        session.hscroll = hscroll;
    }
}

/// Une session porte des modifications non enregistrées : tout changement
/// de requête sélectionnée est refusé (`improve-direct-editing`, D6).
fn selection_locked(model: &Model) -> bool {
    model.editing.as_ref().is_some_and(EditSession::has_unsaved)
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
            session.set_preview(saved.view.clone());
            model.replace_request_node(&path, saved.ast, saved.view);
        }
        Err(error) => {
            model.last_status = Some(super::model::StatusMessage::SaveError(error.to_string()));
        }
    }
}

/// Déclenche la sauvegarde des modifications de l'environnement si la session est modifiée.
fn save_environment(model: &mut Model) -> Command {
    let Some(session) = &model.environment_editing else {
        return Command::None;
    };
    if !session.dirty {
        return Command::None;
    }
    let session_path = session.path.clone();
    let session_stamp = session.stamp;
    // Journal ordonné des modifications déjà validées (design D3, note
    // « Journal des modifications ») : écrit tel quel, sans comparaison
    // indice par indice — les indices qu'il porte ne désignent plus
    // forcément l'AST d'origine dès qu'un ajout ou une suppression a eu
    // lieu.
    let edits = session.pending.clone();

    let Some(collection) = model.loaded() else {
        return Command::None;
    };
    let env = collection.environments.iter().find_map(|e| match e {
        Ok(env) if env.path == session_path => Some(env),
        _ => None,
    });
    let Some(env) = env else {
        return Command::None;
    };
    let Some(ast) = &env.ast else {
        return Command::None;
    };
    if edits.is_empty() {
        if let Some(session) = &mut model.environment_editing {
            session.dirty = false;
        }
        return Command::None;
    }
    Command::SaveEnvironment {
        path: session_path,
        ast: ast.clone(),
        stamp: session_stamp,
        edits,
    }
}

/// Applique le résultat d'une sauvegarde d'environnement.
fn environment_saved(
    model: &mut Model,
    path: std::path::PathBuf,
    result: Result<super::model::SavedEnvironment, crate::writer::WriteError>,
) {
    let new_data = match result {
        Ok(saved) => {
            let Some(session) = &mut model.environment_editing else {
                return;
            };
            if session.path != path {
                return;
            }
            session.stamp = saved.stamp;
            session.dirty = false;
            session.pending.clear();
            Some((saved.ast, session.variables.clone()))
        }
        Err(error) => {
            let Some(session) = &model.environment_editing else {
                return;
            };
            if session.path != path {
                return;
            }
            model.last_status = Some(super::model::StatusMessage::SaveError(error.to_string()));
            None
        }
    };

    if let Some((ast, variables)) = new_data
        && let Some(collection) = model.loaded_mut()
    {
        for env in collection.environments.iter_mut().flatten() {
            if env.path == path {
                env.variables = variables;
                env.ast = Some(ast);
                break;
            }
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
    let selection = model.detail_selection?;
    let bottom = selection.head.unwrap_or_else(|| bottom_of_viewport(model));
    Some(selection.anchor.min(bottom)..=selection.anchor.max(bottom))
}

/// Même principe que [`selection_range`], pour la réponse.
pub(crate) fn response_selection_range(model: &Model) -> Option<RangeInclusive<u16>> {
    let selection = model.response_selection?;
    let bottom = selection
        .head
        .unwrap_or_else(|| response_bottom_of_viewport(model));
    Some(selection.anchor.min(bottom)..=selection.anchor.max(bottom))
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

/// `↑`/`↓`/`Début`/`Fin`/`Entrée`/`→` dans le panneau de sélection
/// d'environnement ou dans la vue des variables.
fn navigate_environment_picker(model: &mut Model, message: Message) {
    if let Some(session) = &mut model.environment_editing {
        match message {
            Message::Up => session.cursor = session.cursor.saturating_sub(1),
            Message::Down => {
                session.cursor =
                    (session.cursor + 1).min(session.variables.len().saturating_sub(1));
            }
            Message::Home => session.cursor = 0,
            Message::End => session.cursor = session.variables.len().saturating_sub(1),
            Message::Enter => {
                session.begin_input();
            }
            _ => {}
        }
        return;
    }

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
        Message::Right | Message::Enter => select_environment_picker(model, before),
        _ => {}
    }
}

/// `e` sur une entrée valide de la liste : ouvre le popup d'édition des
/// variables de l'environnement, sans effet sur « Aucun » ni sur une
/// entrée en erreur (`add-environment-panel-and-edit-popup`).
fn open_environment_editing(model: &mut Model, index: usize) {
    if index == 0 {
        return;
    }
    let Some(collection) = model.loaded() else {
        return;
    };
    let Some(Ok(env)) = collection.environments.get(index - 1) else {
        return;
    };
    let full_path = collection.root.join(&env.path);
    let Ok(stamp) = FileStamp::capture(&full_path) else {
        return;
    };
    model.environment_editing = Some(EnvironmentEditSession::new(
        env.name.clone(),
        env.path.clone(),
        stamp,
        env.variables.clone(),
    ));
}

/// `Entrée` ou `Droite` sur l'entrée courante du panneau : devient
/// l'environnement courant, sans changer le focus (le panneau est
/// permanent — `add-environment-panel-and-edit-popup`) ; sans effet si
/// l'entrée est en erreur ou hors limites (cf. spec « Environnement
/// invalide non sélectionnable »).
fn select_environment_picker(model: &mut Model, index: usize) {
    let Some(collection) = model.loaded() else {
        return;
    };
    let Some(name_opt) = environment_name_at(collection, index) else {
        return;
    };
    let name = name_opt.map(str::to_owned);
    model.current_environment = name;
}

/// Navigation croisée depuis le panneau de diagnostics vers l'arbre :
/// déplie les dossiers ancêtres (et le nœud lui-même si dossier), sélectionne
/// le nœud et repasse le focus à l'arbre.
fn cross_navigate_to_tree(model: &mut Model, address: &[usize]) {
    let current = model
        .tree
        .rows
        .get(model.tree.selected)
        .map(|row| row.address.as_slice());
    if selection_locked(model) && current != Some(address) {
        model.last_status = Some(StatusMessage::EditLocked);
        return;
    }
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

fn navigate_campaign(model: &mut Model, message: Message) {
    let Some(campaign) = &model.campaign else {
        model.campaign_selected = 0;
        return;
    };
    let count = campaign.failures.len();
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
            if let Some(failure) = campaign.failures.get(before) {
                let target_path = failure.path.clone();
                select_request_and_focus_detail(model, &target_path);
            }
        }
        _ => {}
    }
}

/// Sélectionne une requête dans l'arbre en dépliant ses dossiers parents
/// et donne le focus au panneau Détail.
fn select_request_and_focus_detail(model: &mut Model, target_path: &std::path::Path) {
    if select_request_in_tree(model, target_path) {
        model.focus = Focus::Detail;
    }
}

/// Sélectionne une requête dans l'arbre en dépliant ses dossiers parents.
/// Renvoie `false` sans rien changer si une session d'édition verrouille
/// la sélection courante.
fn select_request_in_tree(model: &mut Model, target_path: &std::path::Path) -> bool {
    let current_path = model.selected_node().map(|node| node.path());
    if selection_locked(model) && current_path != Some(target_path) {
        model.last_status = Some(StatusMessage::EditLocked);
        return false;
    }
    for ancestor in target_path.ancestors().skip(1) {
        if !ancestor.as_os_str().is_empty() {
            model.tree.expanded.insert(ancestor.to_path_buf());
        }
    }
    refresh_rows(model);
    let previous_path = model.selected_node().map(|node| node.path().to_path_buf());
    if let Some(index) = model.tree.rows.iter().position(|row| {
        model
            .node_at(&row.address)
            .is_some_and(|node| node.path() == target_path)
    }) {
        model.tree.selected = index;
    }
    clear_detail_view_state(model);
    reset_filter_if_selection_changed(model, previous_path.as_deref());
    scroll_tree_into_view(model);
    true
}

/// Navigue vers la requête en échec suivante (`forward = true`) ou précédente
/// (`forward = false`) dans l'ordre de l'arbre.
fn navigate_failed_request(model: &mut Model, forward: bool) {
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

    let current_addr = model.tree.rows.get(model.tree.selected).map(|r| &r.address);
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
    if selection_locked(model) && current.as_ref() != Some(&address) {
        model.last_status = Some(StatusMessage::EditLocked);
        return true;
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

// --- Souris (`mouse-support`) ---------------------------------------------

/// Lignes défilées par cran de molette.
const WHEEL_LINES: u16 = 3;

/// Traite un événement souris déjà traduit par `to_message`.
fn mouse(model: &mut Model, input: MouseInput) {
    if !mouse_accepted(model) {
        model.mouse.drag = None;
        return;
    }
    match input.kind {
        MouseKind::Press => mouse_press(model, input),
        MouseKind::Drag => mouse_drag(model, input),
        MouseKind::Release => mouse_release(model),
        MouseKind::WheelUp => mouse_wheel(model, input, true),
        MouseKind::WheelDown => mouse_wheel(model, input, false),
    }
}

/// États où la souris est sans effet : capture inactive, confirmation en
/// attente, saisie autre que celle d'un champ, panneau superposé,
/// collection non chargée, terminal trop petit (design D9).
fn mouse_accepted(model: &Model) -> bool {
    model.mouse.capture
        && model.confirm.is_none()
        && matches!(
            model.text_capture(),
            None | Some(TextCapture::Input) | Some(TextCapture::MethodPicker)
        )
        && matches!(
            model.focus,
            Focus::Tree | Focus::Detail | Focus::Response | Focus::EnvironmentPicker
        )
        && model.loaded().is_some()
        && layout_for(model.size).is_some()
}

/// Lignes logiques du champ en cours de saisie, s'il y en a un.
fn input_field_lines(model: &Model) -> Option<std::ops::Range<usize>> {
    let session = model.editing.as_ref()?;
    if !matches!(session.state, EditState::Input(_)) {
        return None;
    }
    let Some(TreeNode::Request(request)) = model.selected_node() else {
        return None;
    };
    if request.path != session.path {
        return None;
    }
    let field = session.current_field()?;
    let (_, fields, _) = request_text_and_fields(request, Some(session));
    let location = fields.into_iter().find(|l| l.field == field)?;
    Some(location.line..location.line + location.count)
}

fn mouse_press(model: &mut Model, input: MouseInput) {
    // Le sélecteur de méthode se pilote au clavier seulement (aucune
    // entrée n'y est cliquable) : un clic ailleurs le referme sans rien
    // changer, comme `Échap`, plutôt que de rendre la souris inerte
    // jusqu'à une touche (`add-method-editing`). Le clic s'arrête là,
    // sans agir sur une nouvelle cible dans le même geste : la boîte du
    // sélecteur occupe des lignes qui disparaissent de la séquence une
    // fois refermée, ce qui rendrait la position cliquée incohérente
    // avec ce que l'utilisateur voyait.
    if model
        .editing
        .as_ref()
        .is_some_and(|s| matches!(s.state, EditState::MethodPicker { .. }))
    {
        cancel_input(model);
        return;
    }
    // Panneau Environnement (permanent) et popup d'édition : résolution
    // de clic auto-contenue, comme le sélecteur de méthode, plutôt
    // qu'intégrée à `hit_test` (`add-environment-panel-and-edit-popup`,
    // design D6). Vérifiée avant `hit_test` : un popup ouvert capture le
    // clic même si sa zone recouvre l'arbre/le détail dessinés dessous.
    if model.environment_editing.is_some() {
        environment_popup_mouse_press(model, input);
        return;
    }
    if let Some(areas) = layout_for(model.size)
        && let Some(index) = environment_panel_row_at(areas.environment, input)
    {
        environment_panel_mouse_press(model, index);
        return;
    }
    // Cible calculée avant toute validation : elle correspond à l'écran
    // que l'utilisateur voyait (sans retour à la ligne pendant la saisie).
    let Some(hit) = hit_test(model, input.column, input.row) else {
        return;
    };
    if let Some(range) = input_field_lines(model) {
        if let Hit::Detail { line: Some(line) } = hit
            && range.contains(&usize::from(line))
        {
            return;
        }
        // Comme Tab : si la saisie reste ouverte (refus, ou clé d'ajout
        // validée et saisie de la valeur), le clic s'arrête là.
        if !matches!(validate_input(model, false), Validation::Done) {
            return;
        }
    }
    model.mouse.drag = None;
    match hit {
        Hit::Tree { row } => {
            model.focus = Focus::Tree;
            if let Some(index) = row {
                click_tree_row(model, index);
            }
        }
        Hit::Detail { line } => {
            model.focus = Focus::Detail;
            model.mouse.drag = line.map(|anchor| Drag {
                panel: DragPanel::Detail,
                anchor,
                moved: false,
            });
        }
        Hit::Response { line } => {
            model.focus = Focus::Response;
            model.mouse.drag = line.map(|anchor| Drag {
                panel: DragPanel::Response,
                anchor,
                moved: false,
            });
        }
    }
}

/// Délai maximal entre deux clics sur la même entrée du panneau
/// Environnement pour qu'ils comptent comme un double-clic
/// (`add-environment-panel-and-edit-popup`, design D4).
const DOUBLE_CLICK_WINDOW: Duration = Duration::from_millis(400);

/// Ligne de la liste du panneau Environnement sous `row`, ou `None` hors
/// de la zone intérieure (résolution par position, sans tenir compte
/// d'un éventuel défilement interne — design D6).
fn environment_panel_row_at(area: Rect, input: MouseInput) -> Option<usize> {
    let inner_area = inner(area);
    if input.column < inner_area.x
        || input.column >= inner_area.x + inner_area.width
        || input.row < inner_area.y
        || input.row >= inner_area.y + inner_area.height
    {
        return None;
    }
    Some(usize::from(input.row - inner_area.y))
}

/// Clic gauche sur la ligne `index` du panneau Environnement (aucun
/// popup ouvert) : donne le focus au panneau, active l'entrée comme le
/// ferait `Entrée`, et ouvre en plus le popup d'édition si c'est un
/// double-clic sur la même entrée (`add-environment-panel-and-edit-popup`,
/// design D4).
fn environment_panel_mouse_press(model: &mut Model, index: usize) {
    model.focus = Focus::EnvironmentPicker;
    let Some(collection) = model.loaded() else {
        return;
    };
    let count = 1 + collection.environments.len();
    if index >= count {
        return;
    }
    model.environment_selected = index;
    select_environment_picker(model, index);

    let now = Instant::now();
    let is_double_click = model
        .mouse
        .last_environment_click
        .is_some_and(|(time, clicked)| {
            clicked == index && now.saturating_duration_since(time) < DOUBLE_CLICK_WINDOW
        });
    model.mouse.last_environment_click = Some((now, index));
    if is_double_click {
        open_environment_editing(model, index);
    }
}

/// Clic gauche dans le popup d'édition d'un environnement : démarre la
/// saisie de la valeur cliquée si elle est activée, comme le clic sur un
/// en-tête de requête (`mouse-support`). Une saisie déjà en cours sur une
/// autre variable est d'abord validée, y compris si le clic ne démarre
/// pas de nouvelle saisie (hors du tableau, sur l'en-tête, ou sur une
/// variable désactivée) — même règle que « Clic pendant une session
/// d'édition » pour une requête.
fn environment_popup_mouse_press(model: &mut Model, input: MouseInput) {
    let Some(session) = &mut model.environment_editing else {
        return;
    };
    if matches!(session.state, EnvironmentEditState::Input(_)) {
        session.commit_input();
    }
    let popup = environment_edit_popup_area(Rect::new(0, 0, model.size.0, model.size.1), session);
    let Some(row) = environment_panel_row_at(popup, input) else {
        return;
    };
    // La première ligne intérieure est l'en-tête de colonnes, jamais
    // sélectionnable.
    let Some(index) = row.checked_sub(1) else {
        return;
    };
    if !session.variables.get(index).is_some_and(|v| v.enabled) {
        return;
    }
    session.cursor = index;
    session.begin_input();
}

/// Clic sur une ligne de l'arbre : sélection, ou dépliage/repli du dossier
/// déjà sélectionné.
fn click_tree_row(model: &mut Model, index: usize) {
    if index != model.tree.selected {
        select_row(model, index);
        return;
    }
    if let Some((path, expanded, _)) = selected_folder(model) {
        if expanded {
            model.tree.expanded.remove(&path);
        } else {
            model.tree.expanded.insert(path);
        }
        refresh_rows(model);
        scroll_tree_into_view(model);
    }
}

fn mouse_drag(model: &mut Model, input: MouseInput) {
    let Some(drag) = model.mouse.drag else {
        return;
    };
    let Some(position) = drag_row(model, drag.panel, input.row) else {
        return;
    };
    let line = match (position, drag.panel) {
        (DragRow::Line(line), _) => line,
        (DragRow::Above, DragPanel::Detail) => {
            scroll_detail(model, Message::Up);
            model.detail_scroll
        }
        (DragRow::Above, DragPanel::Response) => {
            scroll_response(model, Message::Up);
            model.response_scroll
        }
        (DragRow::Below, DragPanel::Detail) => {
            scroll_detail(model, Message::Down);
            bottom_of_viewport(model)
        }
        (DragRow::Below, DragPanel::Response) => {
            scroll_response(model, Message::Down);
            response_bottom_of_viewport(model)
        }
    };
    if line == drag.anchor && !drag.moved {
        return;
    }
    let selection = Some(DetailSelection {
        anchor: drag.anchor,
        head: Some(line),
    });
    match drag.panel {
        DragPanel::Detail => model.detail_selection = selection,
        DragPanel::Response => model.response_selection = selection,
    }
    model.mouse.drag = Some(Drag {
        moved: true,
        ..drag
    });
}

fn mouse_release(model: &mut Model) {
    let Some(drag) = model.mouse.drag.take() else {
        return;
    };
    if drag.moved {
        return;
    }
    match drag.panel {
        DragPanel::Response => model.response_selection = None,
        DragPanel::Detail => {
            model.detail_selection = None;
            if let Some(field) = field_at_line(model, drag.anchor) {
                begin_input_at(model, field);
            }
        }
    }
}

/// Clic sur un champ éditable : ouvre la session si besoin, place le
/// curseur de champ et commence la saisie, comme `Entrée`.
fn begin_input_at(model: &mut Model, field: EditableField) {
    model.focus = Focus::Detail;
    if model.editing.is_none() {
        start_edit(model);
    }
    let Some(session) = &mut model.editing else {
        return;
    };
    if session.state != EditState::FieldSelect {
        return;
    }
    let Some(index) = session.fields.iter().position(|f| *f == field) else {
        return;
    };
    session.cursor = index;
    begin_input(model);
}

fn mouse_wheel(model: &mut Model, input: MouseInput, up: bool) {
    let Some(hit) = hit_test(model, input.column, input.row) else {
        return;
    };
    let step = || if up { Message::Up } else { Message::Down };
    match hit {
        Hit::Tree { .. } => {
            let max = model.tree.rows.len().saturating_sub(tree_height(model));
            let lines = usize::from(WHEEL_LINES);
            model.tree.offset = if up {
                model.tree.offset.saturating_sub(lines)
            } else {
                (model.tree.offset + lines).min(max)
            };
        }
        Hit::Detail { .. } => {
            for _ in 0..WHEEL_LINES {
                scroll_detail(model, step());
            }
        }
        Hit::Response { .. } => {
            for _ in 0..WHEEL_LINES {
                scroll_response(model, step());
            }
        }
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
                head: None,
            });
        }
        Focus::Response => {
            if model.response_selection.take().is_some() {
                return;
            }
            model.response_selection = Some(DetailSelection {
                anchor: model.response_scroll,
                head: None,
            });
        }
        _ => {}
    }
}

/// `y` : copie la sélection visuelle si elle est active, sinon la seule
/// ligne au sommet du panneau. La sélection copiée n'est levée qu'à la
/// réception d'une copie réussie : un échec la laisse intacte. Sans effet hors focus Détail ou Réponse
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
    let selection = range.is_some().then_some(model.focus);
    let text = match range {
        Some(range) => {
            let selected: Vec<&str> = lines
                .iter()
                .enumerate()
                .filter(|(index, _)| range.contains(&(*index as u16)))
                .map(|(_, line)| line.as_str())
                .collect();
            selected.join("\n")
        }
        None => lines.get(usize::from(scroll)).cloned().unwrap_or_default(),
    };
    let token = model.next_clipboard_token;
    model.next_clipboard_token += 1;
    model.pending_clipboard_token = Some(token);
    model.pending_clipboard_selection = selection;
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
    let selection = model.pending_clipboard_selection.take();
    if result.is_ok() {
        match selection {
            Some(Focus::Detail) => model.detail_selection = None,
            Some(Focus::Response) => model.response_selection = None,
            _ => {}
        }
    }
    model.last_status = Some(match result {
        Ok(()) => StatusMessage::Copied,
        Err(error) => StatusMessage::ClipboardError(error.to_string()),
    });
}

/// Résout le programme éditeur à lancer : `VISUAL` si définie et non vide,
/// sinon `EDITOR` si définie et non vide, sinon `"nano"`.
///
/// Paramétrée par une fonction de recherche de variable d'environnement
/// pour rester pure et testable sans dépendre de l'état du processus.
pub fn resolve_editor_with<F>(lookup: F) -> String
where
    F: Fn(&str) -> Option<String>,
{
    if let Some(visual) = lookup("VISUAL").filter(|v| !v.is_empty()) {
        return visual;
    }
    if let Some(editor) = lookup("EDITOR").filter(|e| !e.is_empty()) {
        return editor;
    }
    "nano".to_string()
}

/// Résout le programme éditeur à lancer depuis l'environnement réel du processus.
pub fn resolve_editor() -> String {
    resolve_editor_with(|key| std::env::var(key).ok())
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

    #[test]
    fn resolve_editor_precedence() {
        // VISUAL est prioritaire si définie et non vide
        assert_eq!(
            resolve_editor_with(|key| match key {
                "VISUAL" => Some("code -w".into()),
                "EDITOR" => Some("vim".into()),
                _ => None,
            }),
            "code -w"
        );

        // EDITOR est utilisé si VISUAL n'est pas définie ou est vide
        assert_eq!(
            resolve_editor_with(|key| match key {
                "VISUAL" => None,
                "EDITOR" => Some("vim".into()),
                _ => None,
            }),
            "vim"
        );
        assert_eq!(
            resolve_editor_with(|key| match key {
                "VISUAL" => Some("".into()),
                "EDITOR" => Some("vim".into()),
                _ => None,
            }),
            "vim"
        );

        // "nano" par défaut si ni VISUAL ni EDITOR ne sont définies ou sont vides
        assert_eq!(
            resolve_editor_with(|key| match key {
                "VISUAL" => None,
                "EDITOR" => None,
                _ => None,
            }),
            "nano"
        );
        assert_eq!(
            resolve_editor_with(|key| match key {
                "VISUAL" => Some("".into()),
                "EDITOR" => Some("".into()),
                _ => None,
            }),
            "nano"
        );
    }

    #[test]
    fn open_response_in_editor_command_and_status() {
        let mut model = runner_probe_model();

        // Avec corps exploitable (ok.bru)
        select(&mut model, "ok.bru");
        match update(&mut model, Message::OpenResponseInEditor) {
            Command::OpenInEditor { program, text } => {
                assert_eq!(program, resolve_editor());
                assert_eq!(text, "<html><body>probe</body></html>\n");
            }
            other => panic!("Command::OpenInEditor attendu, obtenu {other:?}"),
        }

        // Sans corps exploitable : dossier sélectionné
        select(&mut model, "folder");
        assert!(matches!(
            update(&mut model, Message::OpenResponseInEditor),
            Command::None
        ));
        assert!(matches!(
            model.last_status,
            Some(StatusMessage::NoResponseBody)
        ));

        // Sans corps exploitable : corps null (folder/down.bru)
        select(&mut model, "folder/down.bru");
        model.last_status = None;
        assert!(matches!(
            update(&mut model, Message::OpenResponseInEditor),
            Command::None
        ));
        assert!(matches!(
            model.last_status,
            Some(StatusMessage::NoResponseBody)
        ));

        // Sans corps exploitable : requête jamais exécutée
        select(&mut model, "ok.bru");
        model.run.outcomes.clear();
        model.last_status = None;
        assert!(matches!(
            update(&mut model, Message::OpenResponseInEditor),
            Command::None
        ));
        assert!(matches!(
            model.last_status,
            Some(StatusMessage::NoResponseBody)
        ));
    }

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
                method: Some("GET".into()),
                url: Some("http://x".into()),
                headers: Some(Default::default()),
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

        // « Aucun » (indice 0) : environnement courant None, le focus ne
        // change pas — le panneau est permanent
        // (`add-environment-panel-and-edit-popup`).
        model.current_environment = Some("local".into());
        model.focus = Focus::EnvironmentPicker;
        model.environment_selected = 0;
        update(&mut model, Message::Right);
        assert_eq!(model.focus, Focus::EnvironmentPicker);
        assert_eq!(model.current_environment, None);

        // Environnement valide (indice 1 : local)
        model.environment_selected = 1;
        update(&mut model, Message::Right);
        assert_eq!(model.focus, Focus::EnvironmentPicker);
        assert_eq!(model.current_environment.as_deref(), Some("local"));

        // Entrée en erreur (indice 2 : malformed) : sans effet
        model.environment_selected = 2;
        update(&mut model, Message::Right);
        assert_eq!(
            model.current_environment.as_deref(),
            Some("local"),
            "inchangé"
        );

        // `Entrée` produit le même effet que `Droite`
        // (`add-environment-panel-and-edit-popup`, design D2).
        model.environment_selected = 0;
        update(&mut model, Message::Enter);
        assert_eq!(model.current_environment, None);
        model.environment_selected = 1;
        update(&mut model, Message::Enter);
        assert_eq!(model.current_environment.as_deref(), Some("local"));
        model.environment_selected = 2;
        update(&mut model, Message::Enter);
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
    fn start_edit_on_valid_environment_opens_edit_popup_and_none_or_error_does_nothing() {
        let mut model = loaded_model((100, 30));
        model.focus = Focus::EnvironmentPicker;

        // Indice 0 (« Aucun ») : `e` ne fait rien
        model.environment_selected = 0;
        update(&mut model, Message::StartEdit);
        assert!(model.environment_editing.is_none());
        assert_eq!(model.focus, Focus::EnvironmentPicker);

        // Indice 2 (malformed, en erreur) : `e` ne fait rien
        model.environment_selected = 2;
        update(&mut model, Message::StartEdit);
        assert!(model.environment_editing.is_none());
        assert_eq!(model.focus, Focus::EnvironmentPicker);

        // Indice 1 (local, valide) : `e` ouvre le popup d'édition
        // (`add-environment-panel-and-edit-popup`, design D2).
        model.environment_selected = 1;
        update(&mut model, Message::StartEdit);
        assert!(model.environment_editing.is_some());
        let session = model.environment_editing.as_ref().unwrap();
        assert_eq!(session.name, "local");
        assert_eq!(session.variables.len(), 2);
        assert_eq!(session.cursor, 0);
        assert_eq!(model.focus, Focus::EnvironmentPicker);
        assert_eq!(
            model.current_environment, None,
            "l'environnement actif ne doit pas changer"
        );
    }

    /// `a` puis `d` dans le popup d'édition d'environnement
    /// (`add-environment-entry-management`).
    #[test]
    fn a_and_d_add_and_remove_a_variable_in_the_environment_popup() {
        let mut model = loaded_model((100, 30));
        model.focus = Focus::EnvironmentPicker;
        model.environment_selected = 1; // "local"
        update(&mut model, Message::StartEdit);
        assert_eq!(
            model.environment_editing.as_ref().unwrap().variables.len(),
            2
        );

        // `a` : ajout en deux temps, clé puis valeur.
        update(&mut model, Message::Add);
        assert!(matches!(
            model.environment_editing.as_ref().unwrap().state,
            EnvironmentEditState::AddingKey(_)
        ));
        for c in "region".chars() {
            update(&mut model, Message::InputKey(InputKey::Char(c)));
        }
        update(&mut model, Message::ValidateInput);
        assert!(matches!(
            &model.environment_editing.as_ref().unwrap().state,
            EnvironmentEditState::AddingValue { key, .. } if key == "region"
        ));
        for c in "eu-west".chars() {
            update(&mut model, Message::InputKey(InputKey::Char(c)));
        }
        update(&mut model, Message::ValidateInput);

        let session = model.environment_editing.as_ref().unwrap();
        assert_eq!(session.state, EnvironmentEditState::Select);
        assert_eq!(session.variables.len(), 3);
        assert_eq!(session.variables[2].key, "region");
        assert_eq!(session.variables[2].value, "eu-west");
        assert_eq!(session.cursor, 2);
        assert!(session.dirty);

        // `d` : suppression immédiate de la variable sous le curseur.
        update(&mut model, Message::Delete);
        let session = model.environment_editing.as_ref().unwrap();
        assert_eq!(session.variables.len(), 2);
        assert!(
            session.variables.iter().all(|v| v.key != "region"),
            "region supprimée"
        );
    }

    #[test]
    fn invalid_key_on_add_is_refused_and_keeps_the_key_input_open() {
        let mut model = loaded_model((100, 30));
        model.focus = Focus::EnvironmentPicker;
        model.environment_selected = 1; // "local"
        update(&mut model, Message::StartEdit);

        update(&mut model, Message::Add);
        for c in "in valid".chars() {
            update(&mut model, Message::InputKey(InputKey::Char(c)));
        }
        update(&mut model, Message::ValidateInput);

        // Refusée : la saisie de la clé reste active, rien n'est ajouté.
        assert!(matches!(
            model.environment_editing.as_ref().unwrap().state,
            EnvironmentEditState::AddingKey(_)
        ));
        assert_eq!(
            model.environment_editing.as_ref().unwrap().variables.len(),
            2
        );
        assert!(matches!(
            model.last_status,
            Some(StatusMessage::EditRefused(_))
        ));
    }

    #[test]
    fn escape_cancels_input_if_active_or_closes_environment_editing_to_picker() {
        let mut model = loaded_model((100, 30));
        model.focus = Focus::EnvironmentPicker;
        model.environment_selected = 1;
        update(&mut model, Message::StartEdit);
        assert!(model.environment_editing.is_some());

        // 1. Commencer la saisie sur la variable 0 ('host')
        update(&mut model, Message::StartEdit);
        assert!(matches!(
            model.environment_editing.as_ref().map(|s| &s.state),
            Some(EnvironmentEditState::Input(_))
        ));

        // Taper un caractère
        update(&mut model, Message::InputKey(InputKey::Char('z')));

        // Échap pendant la saisie annule la saisie (CancelInput via capture de texte)
        update(&mut model, Message::CancelInput);
        assert!(matches!(
            model.environment_editing.as_ref().map(|s| &s.state),
            Some(EnvironmentEditState::Select)
        ));
        assert!(model.environment_editing.is_some());
        assert_eq!(
            model.environment_editing.as_ref().unwrap().variables[0].value,
            "localhost:3000"
        );

        // Échap sans saisie en cours (FocusTree) referme la vue variables et revient à la liste
        update(&mut model, Message::FocusTree);
        assert!(model.environment_editing.is_none());
        assert_eq!(model.focus, Focus::EnvironmentPicker);
        assert_eq!(
            model.environment_selected, 1,
            "sélection conservée sur l'environnement ouvert"
        );

        // Un nouvel Échap ferme le panneau vers Focus::Tree
        update(&mut model, Message::FocusTree);
        assert_eq!(model.focus, Focus::Tree);
    }

    #[test]
    fn save_environment_when_clean_returns_none_when_dirty_returns_command() {
        let mut model = loaded_model((100, 30));
        model.focus = Focus::EnvironmentPicker;
        model.environment_selected = 1; // "local"
        update(&mut model, Message::StartEdit);
        assert!(model.environment_editing.is_some());

        // 1. Session propre -> SaveEdit retourne Command::None
        assert!(matches!(
            update(&mut model, Message::SaveEdit),
            Command::None
        ));

        // 2. Modification de la première variable
        update(&mut model, Message::StartEdit);
        update(&mut model, Message::InputKey(InputKey::Char('!')));
        update(&mut model, Message::ValidateInput);

        assert!(model.environment_editing.as_ref().unwrap().dirty);

        // 3. Session modifiée -> Command::SaveEnvironment avec path, ast, stamp, edits
        match update(&mut model, Message::SaveEdit) {
            Command::SaveEnvironment {
                path,
                ast,
                stamp,
                edits,
            } => {
                let session = model.environment_editing.as_ref().unwrap();
                assert_eq!(path, session.path);
                assert_eq!(stamp, session.stamp);
                assert_eq!(edits.len(), 1);
                assert_eq!(
                    edits[0],
                    crate::writer::EnvironmentVarEdit::Value {
                        index: 0,
                        value: "localhost:3000!".into(),
                    }
                );
                let env = model
                    .loaded()
                    .unwrap()
                    .environments
                    .iter()
                    .find_map(|e| match e {
                        Ok(env) if env.path == path => Some(env),
                        _ => None,
                    })
                    .unwrap();
                assert_eq!(Some(&ast), env.ast.as_ref());
            }
            _ => panic!("Command::SaveEnvironment attendu"),
        }

        // L'émission de Command::SaveEnvironment ne vide PAS dirty
        assert!(model.environment_editing.as_ref().unwrap().dirty);
    }

    #[test]
    fn save_environment_from_input_commits_then_saves() {
        let mut model = loaded_model((100, 30));
        model.focus = Focus::EnvironmentPicker;
        model.environment_selected = 1; // "local"
        update(&mut model, Message::StartEdit);

        // Saisie en cours (sans ValidateInput)
        update(&mut model, Message::StartEdit);
        update(&mut model, Message::InputKey(InputKey::Char('x')));

        // Ctrl+S pendant la saisie valide la saisie et sauvegarde
        match update(&mut model, Message::SaveEdit) {
            Command::SaveEnvironment { edits, .. } => {
                assert_eq!(edits.len(), 1);
                assert_eq!(
                    edits[0],
                    crate::writer::EnvironmentVarEdit::Value {
                        index: 0,
                        value: "localhost:3000x".into(),
                    }
                );
            }
            _ => panic!("Command::SaveEnvironment attendu"),
        }

        let session = model.environment_editing.as_ref().unwrap();
        assert_eq!(session.state, EnvironmentEditState::Select);
        assert!(session.dirty);
        assert_eq!(session.variables[0].value, "localhost:3000x");
    }

    #[test]
    fn environment_saved_success_updates_collection_and_resets_dirty() {
        let mut model = loaded_model((100, 30));
        model.focus = Focus::EnvironmentPicker;
        model.environment_selected = 1; // "local"
        update(&mut model, Message::StartEdit);

        // Modifier
        update(&mut model, Message::StartEdit);
        update(&mut model, Message::InputKey(InputKey::Char('z')));
        update(&mut model, Message::ValidateInput);

        let initial_stamp = model.environment_editing.as_ref().unwrap().stamp;
        let env_path = model.environment_editing.as_ref().unwrap().path.clone();

        let new_stamp = {
            let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/collections/writer-cases/headers.bru");
            crate::writer::FileStamp::capture(&path).expect("stamp")
        };
        let new_source = "vars {\n  host: localhost:3000z\n  token: secret\n}\n";
        let new_ast = crate::collection::BruFile::parse(new_source.to_string()).expect("parse");

        let saved = crate::app::model::SavedEnvironment {
            stamp: new_stamp,
            ast: new_ast.clone(),
        };

        update(
            &mut model,
            Message::EnvironmentSaved {
                path: env_path.clone(),
                result: Ok(saved),
            },
        );

        let session = model.environment_editing.as_ref().unwrap();
        assert!(!session.dirty);
        assert!(!session.has_unsaved());
        assert_eq!(session.stamp, new_stamp);
        assert_ne!(session.stamp, initial_stamp);
        assert_eq!(session.variables[0].value, "localhost:3000z");

        // Dans collection.environments, les variables et l'AST sont mis à jour
        let env = model
            .loaded()
            .unwrap()
            .environments
            .iter()
            .find_map(|e| match e {
                Ok(env) if env.path == env_path => Some(env),
                _ => None,
            })
            .unwrap();
        assert_eq!(env.variables[0].value, "localhost:3000z");
        assert_eq!(env.ast.as_ref(), Some(&new_ast));
    }

    #[test]
    fn environment_saved_failure_retains_modifications_and_sets_status() {
        let mut model = loaded_model((100, 30));
        model.focus = Focus::EnvironmentPicker;
        model.environment_selected = 1; // "local"
        update(&mut model, Message::StartEdit);

        // Modifier
        update(&mut model, Message::StartEdit);
        update(&mut model, Message::InputKey(InputKey::Char('z')));
        update(&mut model, Message::ValidateInput);

        let env_path = model.environment_editing.as_ref().unwrap().path.clone();
        let error = crate::writer::WriteError::Stale {
            path: env_path.clone(),
        };

        update(
            &mut model,
            Message::EnvironmentSaved {
                path: env_path.clone(),
                result: Err(error),
            },
        );

        let session = model.environment_editing.as_ref().unwrap();
        assert!(session.dirty);
        assert_eq!(session.variables[0].value, "localhost:3000z");

        // Message d'erreur dans last_status
        match &model.last_status {
            Some(StatusMessage::SaveError(msg)) => {
                assert!(msg.contains("modifié depuis son chargement"));
            }
            _ => panic!("StatusMessage::SaveError attendu"),
        }
    }

    #[test]
    fn environment_saved_stale_event_ignored() {
        let mut model = loaded_model((100, 30));
        model.focus = Focus::EnvironmentPicker;
        model.environment_selected = 1; // "local"
        update(&mut model, Message::StartEdit);

        update(&mut model, Message::StartEdit);
        update(&mut model, Message::InputKey(InputKey::Char('z')));
        update(&mut model, Message::ValidateInput);

        let other_path = PathBuf::from("environments/other.bru");
        let new_stamp = {
            let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/collections/writer-cases/headers.bru");
            crate::writer::FileStamp::capture(&path).expect("stamp")
        };
        let new_source = "vars {\n  host: other\n}\n";
        let new_ast = crate::collection::BruFile::parse(new_source.to_string()).expect("parse");

        update(
            &mut model,
            Message::EnvironmentSaved {
                path: other_path,
                result: Ok(crate::app::model::SavedEnvironment {
                    stamp: new_stamp,
                    ast: new_ast,
                }),
            },
        );

        // La session active n'a pas été touchée
        let session = model.environment_editing.as_ref().unwrap();
        assert!(session.dirty);
        assert_ne!(session.stamp, new_stamp);
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
        let token = match update(&mut model, Message::Yank) {
            Command::CopyToClipboard { text, token } => {
                assert_eq!(text, expected);
                token
            }
            other => panic!("CopyToClipboard attendu, obtenu {other:?}"),
        };
        // Levée seulement une fois la copie réussie.
        assert!(model.detail_selection.is_some());
        update(
            &mut model,
            Message::ClipboardResult {
                token,
                result: Ok(()),
            },
        );
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

        // Copier depuis la réponse ne lève, une fois la copie réussie, que
        // la sélection de la réponse.
        let Command::CopyToClipboard { token, .. } = update(&mut model, Message::Yank) else {
            panic!("copie attendue");
        };
        update(
            &mut model,
            Message::ClipboardResult {
                token,
                result: Ok(()),
            },
        );
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
        use super::super::model::{EditSession, EditState, EditableField, PendingConfirm};
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
            state: EditState::FieldSelect,
            target: super::super::model::InputTarget::Field,
            pending: vec![],
            preview: crate::collection::RequestView::from_ast(
                &crate::collection::BruFile::parse("get {\n  url: http://a\n}\n".to_owned())
                    .expect("AST"),
            )
            .expect("vue"),
            dirty: true,
            hscroll: 0,
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
            state: EditState::FieldSelect,
            target: super::super::model::InputTarget::Field,
            pending: vec![],
            preview: crate::collection::RequestView::from_ast(
                &crate::collection::BruFile::parse("get {\n  url: http://a\n}\n".to_owned())
                    .expect("AST"),
            )
            .expect("vue"),
            dirty: true,
            hscroll: 0,
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
        assert_eq!(session.state, EditState::FieldSelect);
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
        model.response_selection = Some(DetailSelection {
            anchor: 2,
            head: None,
        });

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

        // Champ 0 : Méthode -> ToggleField sans effet
        assert_eq!(model.editing.as_ref().unwrap().cursor, 0);
        assert!(matches!(
            model.editing.as_ref().unwrap().fields[0],
            EditableField::Method
        ));
        update(&mut model, Message::ToggleField);
        assert!(model.editing.as_ref().unwrap().pending.is_empty());
        assert!(!model.editing.as_ref().unwrap().dirty);

        // Champ 2 : En-tête Accept (initialement activé dans scripted.bru) ;
        // Méthode puis URL précèdent les en-têtes.
        update(&mut model, Message::Down);
        update(&mut model, Message::Down);
        assert_eq!(model.editing.as_ref().unwrap().cursor, 2);
        assert!(matches!(
            model.editing.as_ref().unwrap().fields[2],
            EditableField::HeaderValue(0)
        ));

        assert!(field_enabled(
            model.editing.as_ref().unwrap(),
            &EditableField::HeaderValue(0)
        ));

        // Bascule : devient désactivé
        update(&mut model, Message::ToggleField);
        assert!(model.editing.as_ref().unwrap().dirty);
        assert_eq!(model.editing.as_ref().unwrap().pending.len(), 1);
        assert!(!field_enabled(
            model.editing.as_ref().unwrap(),
            &EditableField::HeaderValue(0)
        ));

        // Seconde bascule : redevient activé
        update(&mut model, Message::ToggleField);
        assert!(field_enabled(
            model.editing.as_ref().unwrap(),
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

        let param_field = model2.editing.as_ref().unwrap().fields[param_cursor];
        let init_enabled = field_enabled(model2.editing.as_ref().unwrap(), &param_field);
        update(&mut model2, Message::ToggleField);
        assert!(model2.editing.as_ref().unwrap().dirty);
        assert_eq!(
            field_enabled(model2.editing.as_ref().unwrap(), &param_field),
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
    fn input_transitions_and_value_preservation() {
        let mut model = loaded_model((100, 30));
        select(&mut model, "simple-get.bru");
        update(&mut model, Message::NextFocus);
        update(&mut model, Message::StartEdit);

        // Curseur sur Méthode par défaut ; on se place sur URL.
        assert_eq!(model.editing.as_ref().unwrap().cursor, 0);
        update(&mut model, Message::Down);
        assert_eq!(model.editing.as_ref().unwrap().cursor, 1);
        let initial_url =
            { field_value(model.editing.as_ref().unwrap(), &EditableField::Url).to_string() };
        let expected_len = initial_url.chars().count();

        // Début de la saisie (Entrée)
        update(&mut model, Message::Enter);
        match &model.editing.as_ref().unwrap().state {
            EditState::Input(input) => {
                assert_eq!(input.cursor(), expected_len);
                assert_eq!(input.text(), initial_url);
            }
            EditState::FieldSelect | EditState::MethodPicker { .. } => panic!("saisie attendue"),
        }

        // Validation sans frappe (Tab) : valeur conservée, pas de dirty
        update(&mut model, Message::ValidateInput);
        assert_eq!(
            model.editing.as_ref().unwrap().state,
            EditState::FieldSelect
        );
        assert!(!model.editing.as_ref().unwrap().dirty);
        assert!(model.editing.as_ref().unwrap().pending.is_empty());

        assert_eq!(
            field_value(model.editing.as_ref().unwrap(), &EditableField::Url,),
            initial_url.as_str()
        );

        // Saisie puis validation par Entrée sur un champ à une ligne (URL)
        update(&mut model, Message::Enter);
        update(&mut model, Message::InputKey(InputKey::Enter));
        assert_eq!(
            model.editing.as_ref().unwrap().state,
            EditState::FieldSelect
        );
        assert!(!model.editing.as_ref().unwrap().dirty);
    }

    #[test]
    fn input_typing_cursor_movement_and_commit_at_validation() {
        let mut model = loaded_model((100, 30));
        select(&mut model, "simple-get.bru");
        update(&mut model, Message::NextFocus);
        update(&mut model, Message::StartEdit);
        update(&mut model, Message::Down); // Méthode -> URL

        // 1. Aller-retour sans modification de valeur mais avec déplacement curseur : dirty inchangé
        update(&mut model, Message::Enter);
        update(&mut model, Message::InputKey(InputKey::Left));
        update(&mut model, Message::InputKey(InputKey::Right));
        update(&mut model, Message::ValidateInput);
        assert!(!model.editing.as_ref().unwrap().dirty);
        assert!(model.editing.as_ref().unwrap().pending.is_empty());

        // 2. Frappe, suppression, insertion en milieu de chaîne
        update(&mut model, Message::Enter);
        // Curseur initial à la fin de "https://{{host}}/ping"
        // Ajout d'un caractère à la fin
        update(&mut model, Message::InputKey(InputKey::Char('s')));
        // Déplacement à gauche
        update(&mut model, Message::InputKey(InputKey::Left));
        update(&mut model, Message::InputKey(InputKey::Left));
        // Insertion en milieu de chaîne
        update(&mut model, Message::InputKey(InputKey::Char('X')));
        // Retour arrière : supprime le X qu'on vient d'insérer
        update(&mut model, Message::InputKey(InputKey::Backspace));

        // Pendant la saisie, pending n'est PAS mis à jour à chaque frappe
        assert!(model.editing.as_ref().unwrap().pending.is_empty());

        // Validation de la saisie (Tab)
        update(&mut model, Message::ValidateInput);
        assert!(model.editing.as_ref().unwrap().dirty);
        assert_eq!(model.editing.as_ref().unwrap().pending.len(), 1);

        assert_eq!(
            field_value(model.editing.as_ref().unwrap(), &EditableField::Url,),
            "https://{{host}}/pings"
        );
    }

    #[test]
    fn input_enter_on_body_inserts_newline() {
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

        // Début de la saisie du corps
        update(&mut model, Message::Enter);

        // Entrée insère un saut de ligne et la saisie continue
        update(&mut model, Message::InputKey(InputKey::Enter));
        match &model.editing.as_ref().unwrap().state {
            EditState::Input(input) => {
                assert!(input.text().ends_with('\n'), "saut de ligne inséré");
            }
            EditState::FieldSelect | EditState::MethodPicker { .. } => {
                panic!("devrait rester en saisie sur le corps")
            }
        }

        // Validation par Tab
        update(&mut model, Message::ValidateInput);
        assert_eq!(
            model.editing.as_ref().unwrap().state,
            EditState::FieldSelect
        );
        assert!(model.editing.as_ref().unwrap().dirty);
        assert_eq!(model.editing.as_ref().unwrap().pending.len(), 1);
    }

    #[test]
    fn save_edit_when_clean_returns_none_when_dirty_returns_command() {
        let mut model = loaded_model((100, 30));
        select(&mut model, "simple-get.bru");
        update(&mut model, Message::NextFocus);
        update(&mut model, Message::StartEdit);
        update(&mut model, Message::Down); // Méthode -> URL

        // 1. Session propre -> Command::None
        let cmd = update(&mut model, Message::SaveEdit);
        assert!(matches!(cmd, Command::None));

        // 2. Modification -> session dirty
        update(&mut model, Message::Enter);
        update(&mut model, Message::InputKey(InputKey::Char('x')));
        update(&mut model, Message::ValidateInput);
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
        update(&mut model, Message::Enter);
        update(&mut model, Message::InputKey(InputKey::Char('x')));
        update(&mut model, Message::ValidateInput);

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
        update(&mut model, Message::Down); // Méthode -> URL

        // Modification
        update(&mut model, Message::Enter);
        update(&mut model, Message::InputKey(InputKey::Char('x')));
        update(&mut model, Message::ValidateInput);

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
        update(&mut model, Message::Down); // Méthode -> URL

        // Modification
        update(&mut model, Message::Enter);
        update(&mut model, Message::InputKey(InputKey::Char('x')));
        update(&mut model, Message::ValidateInput);

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
        update(&mut model, Message::Down); // Méthode -> URL
        update(&mut model, Message::Enter);
        update(&mut model, Message::InputKey(InputKey::Char('a')));
        update(&mut model, Message::ValidateInput);
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
        update(&mut model, Message::Down); // Méthode -> URL
        update(&mut model, Message::Enter);
        update(&mut model, Message::InputKey(InputKey::Char('a')));
        update(&mut model, Message::ValidateInput);
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

    // --- Édition directe (`improve-direct-editing`) ---------------------

    /// Modèle avec une session ouverte sur `path`, focus Détail.
    fn edit_session(path: &str, size: (u16, u16)) -> Model {
        let mut model = loaded_model(size);
        select(&mut model, path);
        update(&mut model, Message::NextFocus);
        update(&mut model, Message::Enter);
        assert!(model.editing.is_some(), "session ouverte sur {path}");
        model
    }

    fn move_to_field(model: &mut Model, field: EditableField) {
        let index = model
            .editing
            .as_ref()
            .and_then(|s| s.fields.iter().position(|f| *f == field))
            .expect("champ présent");
        while model.editing.as_ref().map(|s| s.cursor) != Some(index) {
            update(model, Message::Down);
        }
    }

    fn type_input(model: &mut Model, text: &str) {
        for c in text.chars() {
            update(model, Message::InputKey(InputKey::Char(c)));
        }
    }

    fn press(model: &mut Model, key: InputKey, times: usize) {
        for _ in 0..times {
            update(model, Message::InputKey(key));
        }
    }

    fn input_of(model: &Model) -> &TextInput {
        match &model.editing.as_ref().expect("session").state {
            EditState::Input(input) => input,
            EditState::FieldSelect | EditState::MethodPicker { .. } => panic!("saisie attendue"),
        }
    }

    fn shown_value(model: &Model, field: EditableField) -> String {
        field_value(model.editing.as_ref().expect("session"), &field).to_owned()
    }

    fn session(model: &Model) -> &EditSession {
        model.editing.as_ref().expect("session")
    }

    #[test]
    fn enter_opens_the_session_and_e_is_an_alias() {
        // Ouverture par Entrée, sur le premier champ, en sélection de champ.
        let model = edit_session("simple-get.bru", (100, 30));
        assert_eq!(session(&model).cursor, 0);
        assert_eq!(session(&model).state, EditState::FieldSelect);

        // Alias `e`.
        let mut model = loaded_model((100, 30));
        select(&mut model, "simple-get.bru");
        update(&mut model, Message::NextFocus);
        update(&mut model, Message::StartEdit);
        assert_eq!(session(&model).state, EditState::FieldSelect);

        // Sans effet sur un dossier ni sur un nœud en erreur.
        let mut model = loaded_model((100, 30));
        select(&mut model, "grp");
        update(&mut model, Message::NextFocus);
        update(&mut model, Message::Enter);
        assert!(model.editing.is_none());
        select(&mut model, "broken.bru");
        update(&mut model, Message::Enter);
        assert!(model.editing.is_none());
    }

    #[test]
    fn enter_keeps_its_meaning_outside_the_detail() {
        // Arbre : déplie le dossier, comme `→`.
        let mut model = loaded_model((100, 30));
        select(&mut model, "grp");
        update(&mut model, Message::Enter);
        assert!(model.tree.expanded.contains(Path::new("grp")));

        // Panneau Environnement : `→` active l'entrée, sans changer le
        // focus (`add-environment-panel-and-edit-popup`).
        let mut model = loaded_model((100, 30));
        model.current_environment = Some("local".into());
        model.focus = Focus::EnvironmentPicker;
        model.environment_selected = 0;
        update(&mut model, Message::Right);
        assert_eq!(model.focus, Focus::EnvironmentPicker);
        assert_eq!(model.current_environment, None);

        // Réponse : onglet suivant, comme `→`.
        let mut model = loaded_model((100, 30));
        model.focus = Focus::Response;
        update(&mut model, Message::Enter);
        let mut expected = loaded_model((100, 30));
        expected.focus = Focus::Response;
        update(&mut expected, Message::Right);
        assert_eq!(model.response_tab, expected.response_tab);
    }

    #[test]
    fn validated_url_edit_marks_the_session_modified() {
        let mut model = edit_session("simple-get.bru", (100, 30));
        move_to_field(&mut model, EditableField::Url);
        let url_cursor = session(&model).cursor;
        update(&mut model, Message::Enter);
        assert_eq!(
            input_of(&model).cursor(),
            "https://{{host}}/ping".chars().count()
        );
        type_input(&mut model, "/v2");
        // Rien n'est versé dans `pending` avant la validation.
        assert!(session(&model).pending.is_empty());
        update(&mut model, Message::InputKey(InputKey::Enter));
        assert_eq!(session(&model).state, EditState::FieldSelect);
        assert_eq!(
            shown_value(&model, EditableField::Url),
            "https://{{host}}/ping/v2"
        );
        assert!(session(&model).dirty);
        assert_eq!(session(&model).cursor, url_cursor, "même champ");
    }

    /// Ouverture avec présélection, extrémité haute, navigation, validation,
    /// réouverture avec la nouvelle présélection, annulation sans effet
    /// (`add-method-editing`).
    #[test]
    fn method_picker_opens_navigates_confirms_and_cancels() {
        let mut model = edit_session("simple-get.bru", (100, 30));
        assert_eq!(session(&model).cursor, 0);
        assert!(matches!(session(&model).fields[0], EditableField::Method));

        // Ouverture : présélection sur la méthode courante (GET, indice 0).
        update(&mut model, Message::Enter);
        assert_eq!(
            session(&model).state,
            EditState::MethodPicker { selected: 0 }
        );
        assert!(!session(&model).has_unsaved());

        // Raccourci global sans effet pendant le sélecteur.
        update(&mut model, Message::RunSelected);
        assert!(model.run.active.is_none());
        assert_eq!(
            session(&model).state,
            EditState::MethodPicker { selected: 0 }
        );

        // Extrémité haute : Up ne bouge pas depuis la première entrée.
        update(&mut model, Message::Up);
        assert_eq!(
            session(&model).state,
            EditState::MethodPicker { selected: 0 }
        );

        // Navigation vers POST (indice 1) puis validation.
        update(&mut model, Message::Down);
        assert_eq!(
            session(&model).state,
            EditState::MethodPicker { selected: 1 }
        );
        update(&mut model, Message::Enter);
        assert_eq!(session(&model).state, EditState::FieldSelect);
        assert_eq!(shown_value(&model, EditableField::Method), "POST");
        assert!(session(&model).dirty);

        // Réouverture : la nouvelle méthode est présélectionnée.
        update(&mut model, Message::Enter);
        assert_eq!(
            session(&model).state,
            EditState::MethodPicker { selected: 1 }
        );

        // Annulation : le sélecteur se ferme sans changer la méthode, quelle
        // que soit la présélection au moment de la fermeture.
        update(&mut model, Message::Down);
        update(&mut model, Message::CancelInput);
        assert_eq!(session(&model).state, EditState::FieldSelect);
        assert_eq!(shown_value(&model, EditableField::Method), "POST");
    }

    /// Valider la présélection déjà en place ne marque pas la session
    /// modifiée (`add-method-editing`).
    #[test]
    fn method_picker_validation_without_change_does_not_mark_modified() {
        let mut model = edit_session("simple-get.bru", (100, 30));
        update(&mut model, Message::Enter);
        update(&mut model, Message::Enter);
        assert_eq!(session(&model).state, EditState::FieldSelect);
        assert!(!session(&model).dirty);
        assert!(!session(&model).has_unsaved());
    }

    #[test]
    fn escape_cancels_the_input_without_confirmation() {
        let mut model = edit_session("post-json.bru", (100, 30));
        move_to_field(&mut model, EditableField::HeaderValue(0));
        update(&mut model, Message::Enter);
        type_input(&mut model, "xyz");
        update(&mut model, Message::CancelInput);
        assert!(model.confirm.is_none());
        assert_eq!(session(&model).state, EditState::FieldSelect);
        assert_eq!(
            shown_value(&model, EditableField::HeaderValue(0)),
            "application/json"
        );
        assert!(!session(&model).dirty);
        assert!(!session(&model).has_unsaved());
    }

    #[test]
    fn cancel_after_a_previous_validation_restores_the_validated_value() {
        let mut model = edit_session("simple-get.bru", (100, 30));
        move_to_field(&mut model, EditableField::Url);
        update(&mut model, Message::Enter);
        type_input(&mut model, "b");
        update(&mut model, Message::ValidateInput);
        update(&mut model, Message::Enter);
        type_input(&mut model, "zzz");
        update(&mut model, Message::CancelInput);
        assert_eq!(
            shown_value(&model, EditableField::Url),
            "https://{{host}}/pingb"
        );
        assert!(session(&model).dirty);
    }

    #[test]
    fn body_enter_inserts_a_line_and_tab_validates() {
        let mut model = edit_session("post-json.bru", (100, 30));
        move_to_field(&mut model, EditableField::BodyText);
        update(&mut model, Message::Enter);
        update(&mut model, Message::InputKey(InputKey::Enter));
        type_input(&mut model, "ajout");
        assert!(matches!(session(&model).state, EditState::Input(_)));
        update(&mut model, Message::ValidateInput);
        assert_eq!(session(&model).state, EditState::FieldSelect);
        assert!(shown_value(&model, EditableField::BodyText).ends_with("\najout"));
        assert!(session(&model).dirty);
    }

    #[test]
    fn unchanged_value_does_not_mark_the_session_modified() {
        let mut model = edit_session("simple-get.bru", (100, 30));
        update(&mut model, Message::Enter);
        type_input(&mut model, "x");
        press(&mut model, InputKey::Backspace, 1);
        update(&mut model, Message::ValidateInput);
        assert!(!session(&model).dirty);
        assert!(session(&model).pending.is_empty());
    }

    #[test]
    fn has_unsaved_covers_validated_and_in_progress_edits() {
        let mut model = edit_session("simple-get.bru", (100, 30));
        move_to_field(&mut model, EditableField::Url);
        update(&mut model, Message::Enter);
        assert!(!session(&model).has_unsaved(), "saisie inchangée");
        type_input(&mut model, "x");
        assert!(session(&model).has_unsaved(), "saisie modifiée");
        assert!(!session(&model).dirty);
        update(&mut model, Message::ValidateInput);
        assert!(session(&model).has_unsaved(), "modification validée");
    }

    #[test]
    fn free_text_cursor_through_update() {
        let mut model = edit_session("simple-get.bru", (100, 30));
        move_to_field(&mut model, EditableField::Url);
        update(&mut model, Message::Enter);

        // Insertion au milieu : après « https://{{host}} ».
        press(&mut model, InputKey::Home, 1);
        press(&mut model, InputKey::Right, "https://{{host}}".len());
        type_input(&mut model, "-v2");
        assert_eq!(input_of(&model).text(), "https://{{host}}-v2/ping");

        // Début et Fin.
        press(&mut model, InputKey::Home, 1);
        type_input(&mut model, "x");
        press(&mut model, InputKey::End, 1);
        type_input(&mut model, "y");
        assert_eq!(input_of(&model).text(), "xhttps://{{host}}-v2/pingy");

        // Suppression arrière et avant.
        press(&mut model, InputKey::Home, 1);
        press(&mut model, InputKey::Right, 2);
        press(&mut model, InputKey::Backspace, 1);
        press(&mut model, InputKey::Delete, 1);
        assert_eq!(input_of(&model).text(), "xtps://{{host}}-v2/pingy");
        assert_eq!(input_of(&model).cursor(), 1);

        // Extrémités.
        press(&mut model, InputKey::Home, 1);
        press(&mut model, InputKey::Left, 1);
        press(&mut model, InputKey::Backspace, 1);
        assert_eq!(input_of(&model).text(), "xtps://{{host}}-v2/pingy");
        assert_eq!(input_of(&model).cursor(), 0);

        // ↑/↓ sans effet sur un champ à une ligne.
        press(&mut model, InputKey::Down, 1);
        assert_eq!(input_of(&model).cursor(), 0);

        // Caractère multi-octet.
        press(&mut model, InputKey::End, 1);
        type_input(&mut model, "é");
        press(&mut model, InputKey::Backspace, 1);
        assert_eq!(input_of(&model).text(), "xtps://{{host}}-v2/pingy");
    }

    #[test]
    fn body_vertical_moves_and_line_join_through_update() {
        let mut model = edit_session("post-json.bru", (100, 30));
        move_to_field(&mut model, EditableField::BodyText);
        update(&mut model, Message::Enter);
        let lines_before = input_of(&model).text().split('\n').count();
        // Curseur en fin de corps (dernière ligne `}`) ; ↑ remonte d'une ligne.
        let (last, _) = input_of(&model).cursor_line_col();
        press(&mut model, InputKey::Up, 1);
        assert_eq!(input_of(&model).cursor_line_col().0, last - 1);
        press(&mut model, InputKey::Down, 1);
        assert_eq!(input_of(&model).cursor_line_col().0, last);
        // Jointure : début de la dernière ligne, Retour arrière.
        press(&mut model, InputKey::Home, 1);
        press(&mut model, InputKey::Backspace, 1);
        assert_eq!(
            input_of(&model).text().split('\n').count(),
            lines_before - 1
        );
    }

    #[test]
    fn save_from_input_validates_then_saves() {
        let mut model = edit_session("simple-get.bru", (100, 30));
        // Sauvegarde sans modification : aucune écriture.
        assert!(matches!(
            update(&mut model, Message::SaveEdit),
            Command::None
        ));
        move_to_field(&mut model, EditableField::Url);
        update(&mut model, Message::Enter);
        type_input(&mut model, "!");
        match update(&mut model, Message::SaveEdit) {
            Command::SaveEdit { edits, .. } => {
                assert_eq!(edits, vec![FieldEdit::Url("https://{{host}}/ping!".into())]);
            }
            other => panic!("Command::SaveEdit attendu : {other:?}"),
        }
        assert_eq!(session(&model).state, EditState::FieldSelect);
        // Hors session, Ctrl+S est sans effet.
        let mut model = loaded_model((100, 30));
        assert!(matches!(
            update(&mut model, Message::SaveEdit),
            Command::None
        ));
    }

    #[test]
    fn quitting_during_a_modified_input_asks_and_keeps_the_input() {
        let mut model = edit_session("simple-get.bru", (100, 30));
        move_to_field(&mut model, EditableField::Url);
        update(&mut model, Message::Enter);
        type_input(&mut model, "abc");
        press(&mut model, InputKey::Left, 1);

        // `q` en saisie est du texte.
        update(&mut model, Message::InputKey(InputKey::Char('q')));
        assert!(model.confirm.is_none());
        assert!(model.exit.is_none());
        assert_eq!(input_of(&model).text(), "https://{{host}}/pingabqc");
        let cursor = input_of(&model).cursor();

        update(&mut model, Message::ForceQuit);
        assert_eq!(model.confirm, Some(PendingConfirm::QuitWithUnsavedEdit));
        // La confirmation reprend le clavier : plus de capture de texte.
        assert_eq!(model.text_capture(), None);
        update(&mut model, Message::FocusTree);
        assert!(model.exit.is_none());
        assert_eq!(input_of(&model).text(), "https://{{host}}/pingabqc");
        assert_eq!(input_of(&model).cursor(), cursor);
        assert_eq!(
            model.text_capture(),
            Some(crate::app::message::TextCapture::Input)
        );
    }

    #[test]
    fn selection_change_is_refused_while_the_session_is_modified() {
        let mut model = edit_session("simple-get.bru", (100, 30));
        move_to_field(&mut model, EditableField::Url);
        update(&mut model, Message::Enter);
        type_input(&mut model, "!");
        update(&mut model, Message::ValidateInput);
        let selected = model.tree.selected;

        // Navigation dans l'arbre.
        update(&mut model, Message::NextFocus);
        update(&mut model, Message::NextFocus);
        assert_eq!(model.focus, Focus::Tree);
        update(&mut model, Message::Down);
        assert_eq!(model.tree.selected, selected);
        assert!(matches!(model.last_status, Some(StatusMessage::EditLocked)));
        assert!(session(&model).dirty);

        // Recherche dans l'arbre.
        model.last_status = None;
        update(&mut model, Message::StartSearch);
        for c in "inherit".chars() {
            update(&mut model, Message::SearchInput(c));
        }
        update(&mut model, Message::ConfirmSearch);
        assert_eq!(model.tree.selected, selected);
        assert!(matches!(model.last_status, Some(StatusMessage::EditLocked)));
        assert!(!model.tree.expanded.contains(Path::new("grp")));

        // Navigation croisée depuis les diagnostics.
        model.last_status = None;
        model.focus = Focus::Diagnostics;
        model.diagnostics_selected = 1;
        update(&mut model, Message::Right);
        assert_eq!(model.tree.selected, selected);
        assert!(matches!(model.last_status, Some(StatusMessage::EditLocked)));
        assert!(session(&model).dirty);

        // Une navigation qui ne change pas de nœud reste permise.
        model.last_status = None;
        model.focus = Focus::Tree;
        update(&mut model, Message::Left);
        assert!(model.last_status.is_none());
        assert!(model.editing.is_some());
    }

    #[test]
    fn clean_session_is_closed_on_selection_change() {
        let mut model = edit_session("simple-get.bru", (100, 30));
        update(&mut model, Message::Enter);
        update(&mut model, Message::CancelInput);
        model.focus = Focus::Tree;
        let before = model.tree.selected;
        update(&mut model, Message::Down);
        assert_ne!(model.tree.selected, before);
        assert!(model.editing.is_none());
    }

    #[test]
    fn field_cursor_and_text_cursor_stay_visible() {
        // Terminal bas : le corps de post-json est hors écran au départ.
        let mut model = edit_session("post-json.bru", (100, 12));
        let height = layout_for(model.size)
            .map(|a| inner(a.detail).height)
            .expect("layout");
        assert_eq!(model.detail_scroll, 0);
        move_to_field(&mut model, EditableField::BodyText);
        let Some(TreeNode::Request(request)) = model.selected_node() else {
            panic!("requête attendue");
        };
        let (_, lines, _) = request_text_and_fields(request, model.editing.as_ref());
        let body = lines
            .iter()
            .find(|l| l.field == EditableField::BodyText)
            .expect("corps")
            .line as u16;
        assert!(model.detail_scroll <= body && body < model.detail_scroll + height);

        // Nouvelle ligne en bas du corps : le détail suit le curseur.
        update(&mut model, Message::Enter);
        press(&mut model, InputKey::Enter, 3);
        let (line, _) = input_of(&model).cursor_line_col();
        let target = body + line as u16;
        assert!(
            model.detail_scroll <= target && target < model.detail_scroll + height,
            "ligne {target} visible (défilement {})",
            model.detail_scroll
        );

        // URL plus large que le panneau : décalage horizontal.
        let mut model = edit_session("simple-get.bru", (60, 20));
        let width = layout_for(model.size)
            .map(|a| inner(a.detail).width)
            .expect("layout");
        move_to_field(&mut model, EditableField::Url);
        update(&mut model, Message::Enter);
        type_input(&mut model, &"a".repeat(usize::from(width) * 2));
        let hscroll = session(&model).hscroll;
        assert!(hscroll > 0);
        press(&mut model, InputKey::Home, 1);
        assert_eq!(session(&model).hscroll, 0);
        press(&mut model, InputKey::End, 1);
        assert_eq!(session(&model).hscroll, hscroll);
        update(&mut model, Message::ValidateInput);
        assert_eq!(session(&model).hscroll, 0);
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
        update(&mut model, Message::Delete);
        assert_eq!(
            rows(&model)[0].source,
            SecretSource::DotEnv {
                key: "TOKEN".into()
            }
        );
    }

    #[test]
    fn enter_opens_the_secret_input_like_right() {
        let mut model = local_model();
        update(&mut model, Message::ToggleSecrets);
        update(&mut model, Message::Enter);
        assert!(model.secrets.input.is_some());
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
        update(&mut model, Message::Add);
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

        update(&mut model, Message::Add);
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
        update(&mut model, Message::Delete);
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
    // --- Ajout, suppression, renommage (`add-entry-management`) ---------

    /// Déplace le curseur de champ sur `field`, vers le haut ou le bas.
    fn go_to(model: &mut Model, field: EditableField) {
        let index = session(model)
            .fields
            .iter()
            .position(|f| *f == field)
            .expect("champ présent");
        while session(model).cursor > index {
            update(model, Message::Up);
        }
        while session(model).cursor < index {
            update(model, Message::Down);
        }
    }

    fn target_of(model: &Model) -> &InputTarget {
        &session(model).target
    }

    /// Ajoute `key: value` à `section` depuis sa ligne d'ajout.
    fn add_via_row(model: &mut Model, section: EntrySection, key: &str, value: &str) {
        go_to(model, EditableField::AddRow(section));
        update(model, Message::Enter);
        type_input(model, key);
        update(model, Message::ValidateInput);
        type_input(model, value);
        update(model, Message::InputKey(InputKey::Enter));
        assert_eq!(session(model).state, EditState::FieldSelect, "ajout validé");
    }

    #[test]
    fn a_on_an_existing_header_adds_at_the_end_of_the_section() {
        let mut model = edit_session("scripted.bru", (100, 30));
        go_to(&mut model, EditableField::HeaderValue(0));
        update(&mut model, Message::Add);
        assert!(matches!(
            target_of(&model),
            InputTarget::NewKey {
                section: EntrySection::Headers,
                ..
            }
        ));
        type_input(&mut model, "X-Trace");
        update(&mut model, Message::InputKey(InputKey::Enter));
        assert!(matches!(target_of(&model), InputTarget::NewValue { .. }));
        assert!(session(&model).has_unsaved());
        type_input(&mut model, "abc");
        update(&mut model, Message::InputKey(InputKey::Enter));

        let session = session(&model);
        assert_eq!(session.state, EditState::FieldSelect);
        let last = session.preview.headers.last().expect("en-tête");
        assert_eq!((last.key.as_str(), last.value.as_str()), ("X-Trace", "abc"));
        assert!(last.enabled);
        assert_eq!(session.current_field(), Some(EditableField::HeaderValue(2)));
        assert!(session.dirty);
    }

    #[test]
    fn adding_a_query_param_from_its_row_updates_the_url() {
        let mut model = edit_session("simple-get.bru", (100, 30));
        add_via_row(&mut model, EntrySection::QueryParams, "page", "2");
        let session = session(&model);
        assert_eq!(session.preview.url, "https://{{host}}/ping?page=2");
        assert_eq!(session.preview.query_params[0].key, "page");
        assert_eq!(
            session.current_field(),
            Some(EditableField::QueryParamValue(0))
        );
    }

    #[test]
    fn adding_into_an_empty_path_section() {
        let mut model = edit_session("simple-get.bru", (100, 30));
        add_via_row(&mut model, EntrySection::PathParams, "id", "42");
        let session = session(&model);
        assert_eq!(session.preview.path_params[0].key, "id");
        assert_eq!(
            session.current_field(),
            Some(EditableField::PathParamValue(0))
        );
        assert_eq!(session.preview.url, "https://{{host}}/ping");
    }

    #[test]
    fn escape_abandons_the_whole_add_and_restores_the_cursor() {
        let mut model = edit_session("scripted.bru", (100, 30));
        go_to(&mut model, EditableField::HeaderValue(1));
        let cursor = session(&model).cursor;
        update(&mut model, Message::Add);
        type_input(&mut model, "X-Trace");
        update(&mut model, Message::ValidateInput);

        // Une clé validée verrouille la sélection de requête.
        assert!(selection_locked(&model));

        type_input(&mut model, "abc");
        update(&mut model, Message::CancelInput);
        let session = session(&model);
        assert_eq!(session.state, EditState::FieldSelect);
        assert_eq!(session.cursor, cursor);
        assert_eq!(session.preview.headers.len(), 2);
        assert!(session.pending.is_empty());
        assert!(!session.dirty);
        assert!(!session.has_unsaved());
    }

    #[test]
    fn a_d_c_and_space_have_no_effect_where_not_applicable() {
        let mut model = edit_session("post-json.bru", (100, 30));
        // URL
        update(&mut model, Message::Add);
        update(&mut model, Message::Rename);
        update(&mut model, Message::Delete);
        assert_eq!(session(&model).state, EditState::FieldSelect);
        assert!(session(&model).pending.is_empty());
        // Corps
        go_to(&mut model, EditableField::BodyText);
        update(&mut model, Message::Add);
        update(&mut model, Message::Delete);
        update(&mut model, Message::Rename);
        assert_eq!(session(&model).state, EditState::FieldSelect);
        assert!(session(&model).pending.is_empty());
        // Ligne d'ajout : `a` ajoute, mais `d`, `c` et Espace sont sans effet.
        go_to(&mut model, EditableField::AddRow(EntrySection::Headers));
        update(&mut model, Message::Delete);
        update(&mut model, Message::Rename);
        update(&mut model, Message::ToggleField);
        assert_eq!(session(&model).state, EditState::FieldSelect);
        assert!(session(&model).pending.is_empty());
        update(&mut model, Message::Add);
        assert!(matches!(target_of(&model), InputTarget::NewKey { .. }));
    }

    #[test]
    fn d_deletes_immediately_and_keeps_the_cursor_index() {
        let mut model = edit_session("simple-get.bru", (100, 30));
        add_via_row(&mut model, EntrySection::Headers, "A", "1");
        add_via_row(&mut model, EntrySection::Headers, "B", "2");
        add_via_row(&mut model, EntrySection::Headers, "C", "3");
        go_to(&mut model, EditableField::HeaderValue(1));
        let cursor = session(&model).cursor;
        update(&mut model, Message::Delete);

        let session = session(&model);
        let keys: Vec<&str> = session
            .preview
            .headers
            .iter()
            .map(|h| h.key.as_str())
            .collect();
        assert_eq!(keys, ["A", "C"]);
        assert_eq!(session.cursor, cursor);
        assert_eq!(session.current_field(), Some(EditableField::HeaderValue(1)));
        assert!(session.dirty);
    }

    #[test]
    fn deleting_a_query_param_updates_the_url_and_locks_the_selection() {
        let mut model = edit_session("simple-get.bru", (100, 30));
        add_via_row(&mut model, EntrySection::QueryParams, "page", "2");
        add_via_row(&mut model, EntrySection::QueryParams, "size", "10");
        assert_eq!(
            session(&model).preview.url,
            "https://{{host}}/ping?page=2&size=10"
        );
        go_to(&mut model, EditableField::QueryParamValue(1));
        update(&mut model, Message::Delete);
        assert_eq!(session(&model).preview.url, "https://{{host}}/ping?page=2");

        let selected = model.tree.selected;
        update(&mut model, Message::NextFocus);
        update(&mut model, Message::NextFocus);
        update(&mut model, Message::Down);
        assert_eq!(model.tree.selected, selected);
        assert!(matches!(model.last_status, Some(StatusMessage::EditLocked)));
    }

    #[test]
    fn c_renames_a_query_param_and_updates_the_url() {
        let mut model = edit_session("simple-get.bru", (100, 30));
        add_via_row(&mut model, EntrySection::QueryParams, "p", "2");
        update(&mut model, Message::Rename);
        assert_eq!(input_of(&model).text(), "p");
        assert_eq!(input_of(&model).cursor(), 1);
        press(&mut model, InputKey::Backspace, 1);
        type_input(&mut model, "page");
        update(&mut model, Message::InputKey(InputKey::Enter));

        let session = session(&model);
        assert_eq!(session.state, EditState::FieldSelect);
        let param = &session.preview.query_params[0];
        assert_eq!((param.key.as_str(), param.value.as_str()), ("page", "2"));
        assert_eq!(session.preview.url, "https://{{host}}/ping?page=2");
    }

    #[test]
    fn cancelled_rename_keeps_the_key_and_the_session_clean() {
        let mut model = edit_session("scripted.bru", (100, 30));
        go_to(&mut model, EditableField::HeaderValue(1));
        update(&mut model, Message::Rename);
        type_input(&mut model, "zz");
        assert!(session(&model).has_unsaved());
        update(&mut model, Message::CancelInput);
        let session = session(&model);
        assert_eq!(session.preview.headers[1].key, "X-Debug");
        assert!(!session.preview.headers[1].enabled);
        assert!(!session.dirty);
        assert!(!session.has_unsaved());
    }

    #[test]
    fn rename_keeps_value_and_disabled_state() {
        let mut model = edit_session("scripted.bru", (100, 30));
        go_to(&mut model, EditableField::HeaderValue(1));
        update(&mut model, Message::Rename);
        press(&mut model, InputKey::Backspace, 5);
        type_input(&mut model, "Verbose");
        update(&mut model, Message::ValidateInput);
        let header = &session(&model).preview.headers[1];
        assert_eq!(header.key, "X-Verbose");
        assert_eq!(header.value, "1");
        assert!(!header.enabled);
    }

    #[test]
    fn ctrl_s_during_the_key_step_adds_an_empty_value_and_saves() {
        let mut model = edit_session("simple-get.bru", (100, 30));
        go_to(&mut model, EditableField::AddRow(EntrySection::Headers));
        update(&mut model, Message::Enter);
        type_input(&mut model, "X-Trace");
        let command = update(&mut model, Message::SaveEdit);
        let Command::SaveEdit { edits, .. } = command else {
            panic!("sauvegarde attendue");
        };
        assert_eq!(
            edits,
            [FieldEdit::AddEntry {
                section: EntrySection::Headers,
                key: "X-Trace".into(),
                value: String::new(),
                enabled: true,
            }]
        );
        assert_eq!(session(&model).state, EditState::FieldSelect);
    }

    #[test]
    fn ctrl_s_on_an_invalid_key_refuses_without_saving() {
        let mut model = edit_session("simple-get.bru", (100, 30));
        go_to(&mut model, EditableField::AddRow(EntrySection::Headers));
        update(&mut model, Message::Enter);
        type_input(&mut model, "X:Trace");
        let command = update(&mut model, Message::SaveEdit);
        assert!(matches!(command, Command::None));
        assert_eq!(input_of(&model).text(), "X:Trace");
        assert!(matches!(target_of(&model), InputTarget::NewKey { .. }));
        let Some(StatusMessage::EditRefused(reason)) = &model.last_status else {
            panic!("refus attendu");
        };
        assert!(!reason.contains("X:Trace"), "{reason}");
        assert!(session(&model).pending.is_empty());
        assert!(!session(&model).dirty);
    }

    #[test]
    fn invalid_key_on_enter_keeps_the_input_open() {
        let mut model = edit_session("scripted.bru", (100, 30));
        go_to(&mut model, EditableField::HeaderValue(0));
        update(&mut model, Message::Add);
        type_input(&mut model, "X Trace");
        update(&mut model, Message::InputKey(InputKey::Enter));
        assert!(matches!(target_of(&model), InputTarget::NewKey { .. }));
        assert_eq!(input_of(&model).text(), "X Trace");
        assert!(matches!(
            model.last_status,
            Some(StatusMessage::EditRefused(_))
        ));
        assert_eq!(session(&model).preview.headers.len(), 2);
    }

    #[test]
    fn refused_query_value_stays_in_input_then_escape_restores() {
        let mut model = edit_session("simple-get.bru", (100, 30));
        add_via_row(&mut model, EntrySection::QueryParams, "q", "2");
        let pending = session(&model).pending.clone();
        update(&mut model, Message::Enter);
        type_input(&mut model, "#b");
        update(&mut model, Message::InputKey(InputKey::Enter));
        assert_eq!(input_of(&model).text(), "2#b");
        assert!(matches!(
            model.last_status,
            Some(StatusMessage::EditRefused(_))
        ));
        assert_eq!(session(&model).pending, pending);

        update(&mut model, Message::CancelInput);
        assert_eq!(session(&model).state, EditState::FieldSelect);
        assert_eq!(shown_value(&model, EditableField::QueryParamValue(0)), "2");
    }

    #[test]
    fn duplicate_header_key_is_accepted() {
        let mut model = edit_session("scripted.bru", (100, 30));
        add_via_row(&mut model, EntrySection::Headers, "Accept", "text/plain");
        let accepts = session(&model)
            .preview
            .headers
            .iter()
            .filter(|h| h.key == "Accept")
            .count();
        assert_eq!(accepts, 2);
    }

    #[test]
    fn validating_the_url_resynchronises_query_params() {
        let mut model = edit_session("simple-get.bru", (100, 30));
        add_via_row(&mut model, EntrySection::QueryParams, "page", "2");
        go_to(&mut model, EditableField::Url);
        update(&mut model, Message::Enter);
        press(&mut model, InputKey::Backspace, 1);
        type_input(&mut model, "3");
        update(&mut model, Message::InputKey(InputKey::Enter));
        assert_eq!(session(&model).preview.query_params[0].value, "3");
    }

    #[test]
    fn edit_saved_after_an_add_replaces_the_preview() {
        let mut model = edit_session("simple-get.bru", (100, 30));
        add_via_row(&mut model, EntrySection::Headers, "A", "1");
        let Some(TreeNode::Request(request)) = model.selected_node() else {
            panic!("requête attendue");
        };
        let ast = request.ast.clone().expect("ast");
        let pending = session(&model).pending.clone();
        let bytes = crate::writer::preview(&ast, &pending).expect("aperçu");
        let source = "meta {\n  name: ping\n  type: http\n  seq: 7\n}\n\nget {\n  url: https://{{host}}/ping\n  body: none\n  auth: none\n}\n\nheaders {\n  A: 1\n}\n";
        let saved_ast = crate::collection::BruFile::parse(source.to_owned()).expect("AST");
        let saved_view = RequestView::from_ast(&saved_ast).expect("vue");
        assert_eq!(bytes, saved_view);
        let stamp = session(&model).stamp;
        update(
            &mut model,
            Message::EditSaved {
                path: "simple-get.bru".into(),
                result: Ok(SavedEdit {
                    stamp,
                    ast: saved_ast,
                    view: saved_view.clone(),
                }),
            },
        );
        let session = session(&model);
        assert!(session.pending.is_empty());
        assert!(!session.dirty);
        assert_eq!(session.preview, saved_view);
        assert!(session.fields.contains(&EditableField::HeaderValue(0)));
    }

    // --- Panneau Campagne & navigation dans les requêtes en échec ---

    #[test]
    fn toggle_campaign_panel() {
        let mut model = loaded_model((100, 30));
        assert_eq!(model.focus, Focus::Tree);

        // Ouverture puis fermeture campagne avec C
        update(&mut model, Message::ToggleCampaign);
        assert_eq!(model.focus, Focus::Campaign);
        update(&mut model, Message::ToggleCampaign);
        assert_eq!(model.focus, Focus::Tree);

        // Bascule directe depuis Diagnostics
        update(&mut model, Message::ToggleDiagnostics);
        assert_eq!(model.focus, Focus::Diagnostics);
        update(&mut model, Message::ToggleCampaign);
        assert_eq!(model.focus, Focus::Campaign);

        // Échap referme le panneau vers l'arbre
        update(&mut model, Message::FocusTree);
        assert_eq!(model.focus, Focus::Tree);

        // Sans collection chargée : aucun effet
        let mut loading_model = Model::new("/x".into(), (100, 30));
        update(&mut loading_model, Message::ToggleCampaign);
        assert_eq!(loading_model.focus, Focus::Tree);
    }

    #[test]
    fn campaign_summary_constructed_on_recursive_completed() {
        use runner::report::{AssertionResult, ResponseStatus, ResultStatus, TestResult};

        let mut model = loaded_model((100, 30));
        let id = runner::RunId(42);
        model.run.active = Some(ActiveRun {
            id,
            target: PathBuf::from("grp"),
            recursive: true,
            handle: None,
        });

        let mut res1 = sample_request_result("grp/ok.bru");
        res1.run_duration = 0.1;

        let mut res2 = sample_request_result("grp/skipped.bru");
        res2.skipped = true;
        res2.run_duration = 0.05;

        let mut res3 = sample_request_result("grp/down.bru");
        res3.error = Some("connect ECONNREFUSED 127.0.0.1:80".into());
        res3.status = ResultStatus::Error;
        res3.response.status = ResponseStatus::Error;
        res3.run_duration = 0.2;

        let mut res4 = sample_request_result("grp/bad_assert.bru");
        res4.response.status = ResponseStatus::Http(404);
        res4.assertion_results = vec![AssertionResult {
            uid: "1".into(),
            lhs_expr: "res.status".into(),
            rhs_expr: "200".into(),
            rhs_operand: "200".into(),
            operator: "eq".into(),
            status: ResultStatus::Fail,
            error: Some("expected 404 to equal 200".into()),
        }];
        res4.run_duration = 0.15;

        let mut res5 = sample_request_result("grp/bad_test.bru");
        res5.response.status = ResponseStatus::Http(401);
        res5.test_results = vec![TestResult {
            uid: "2".into(),
            description: "token test".into(),
            status: ResultStatus::Fail,
            error: Some("missing token".into()),
            actual: None,
            expected: None,
        }];
        res5.run_duration = 0.1;

        let report = runner::Report(vec![runner::report::Iteration {
            iteration_index: 0,
            results: vec![res1, res2, res3, res4, res5],
            summary: runner::report::Summary {
                total_requests: 5,
                passed_requests: 1,
                failed_requests: 3,
                error_requests: 1,
                skipped_requests: 1,
                total_assertions: 1,
                passed_assertions: 0,
                failed_assertions: 1,
                total_tests: 1,
                passed_tests: 0,
                failed_tests: 1,
                total_pre_request_tests: 0,
                passed_pre_request_tests: 0,
                failed_pre_request_tests: 0,
                total_post_response_tests: 0,
                passed_post_response_tests: 0,
                failed_post_response_tests: 0,
            },
        }]);

        update(
            &mut model,
            Message::RunFinished(runner::RunEvent {
                id,
                outcome: runner::RunOutcome::Completed {
                    report,
                    exit_code: Some(1),
                },
            }),
        );

        let campaign = model.campaign.as_ref().expect("campagne construite");
        assert_eq!(campaign.target, PathBuf::from("grp"));
        assert_eq!(campaign.total, 5);
        assert_eq!(campaign.passed, 1);
        assert_eq!(campaign.failed, 3);
        assert_eq!(campaign.skipped, 1);
        assert!((campaign.duration_secs - 0.6).abs() < 1e-6);
        assert_eq!(campaign.failures.len(), 3);

        // Première raison d'échec : message d'erreur bru
        assert_eq!(campaign.failures[0].path, PathBuf::from("grp/down.bru"));
        assert_eq!(campaign.failures[0].http_code, None);
        assert_eq!(
            campaign.failures[0].reason,
            "connect ECONNREFUSED 127.0.0.1:80"
        );

        // Deuxième raison d'échec : première assertion en échec avec son message
        assert_eq!(
            campaign.failures[1].path,
            PathBuf::from("grp/bad_assert.bru")
        );
        assert_eq!(campaign.failures[1].http_code, Some(404));
        assert_eq!(
            campaign.failures[1].reason,
            "res.status eq 200 : expected 404 to equal 200"
        );

        // Troisième raison d'échec : premier test en échec avec son message
        assert_eq!(campaign.failures[2].path, PathBuf::from("grp/bad_test.bru"));
        assert_eq!(campaign.failures[2].http_code, Some(401));
        assert_eq!(campaign.failures[2].reason, "token test : missing token");

        // Statut émis dans la barre d'état
        assert!(matches!(
            &model.last_status,
            Some(StatusMessage::CampaignFinished(msg))
                if msg == "Campagne grp : 1/5 réussis · 3 échecs — C pour le détail"
        ));

        // Une exécution non récursive ne remplace pas le résumé de campagne
        let non_rec_id = runner::RunId(43);
        model.run.active = Some(ActiveRun {
            id: non_rec_id,
            target: PathBuf::from("simple-get.bru"),
            recursive: false,
            handle: None,
        });
        let single_report = sample_report(&["simple-get.bru"]);
        update(
            &mut model,
            Message::RunFinished(runner::RunEvent {
                id: non_rec_id,
                outcome: runner::RunOutcome::Completed {
                    report: single_report,
                    exit_code: Some(0),
                },
            }),
        );
        assert_eq!(
            model.campaign.as_ref().unwrap().target,
            PathBuf::from("grp")
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
            super::super::model::RequestOutcome {
                result: res_fail1,
                exit_code: Some(1),
            },
        );

        let mut res_fail2 = sample_request_result("grp/sub/deep.bru");
        res_fail2.error = Some("fail 2".into());
        res_fail2.status = runner::report::ResultStatus::Error;
        model.run.outcomes.insert(
            PathBuf::from("grp/sub/deep.bru"),
            super::super::model::RequestOutcome {
                result: res_fail2,
                exit_code: Some(1),
            },
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

#[cfg(test)]
mod mouse_tests {
    //! Souris (`mouse-support`) : scénarios de la spécification, rejoués
    //! sur `update` avec des positions calculées par le hit-testing.

    use std::path::Path;

    use super::*;
    use crate::app::model::{MouseState, PendingConfirm};
    use crate::app::test_support::{loaded_model, runner_probe_model, select, selected_name};
    use crate::app::view::hit::{Hit, hit_test};

    fn model_on(path: &str, size: (u16, u16)) -> Model {
        let mut model = loaded_model(size);
        model.mouse.capture = true;
        select(&mut model, path);
        model
    }

    fn event(model: &mut Model, kind: MouseKind, (column, row): (u16, u16)) -> Command {
        update(model, Message::Mouse(MouseInput { kind, column, row }))
    }

    fn click(model: &mut Model, point: (u16, u16)) {
        event(model, MouseKind::Press, point);
        event(model, MouseKind::Release, point);
    }

    fn row_of(model: &Model, path: &str) -> usize {
        model
            .tree
            .rows
            .iter()
            .position(|row| {
                model
                    .node_at(&row.address)
                    .is_some_and(|node| node.path() == Path::new(path))
            })
            .unwrap_or_else(|| panic!("`{path}` non visible"))
    }

    fn tree_point(model: &Model, path: &str) -> (u16, u16) {
        let area = inner(layout_for(model.size).expect("taille").tree);
        let index = row_of(model, path) - model.tree.offset;
        (area.x + 2, area.y + u16::try_from(index).expect("petit"))
    }

    /// Position écran de la ligne logique `line` du détail ou de la réponse.
    fn line_point(model: &Model, detail: bool, line: u16) -> (u16, u16) {
        let areas = layout_for(model.size).expect("taille");
        let area = inner(if detail { areas.detail } else { areas.response });
        (area.y..area.bottom())
            .map(|y| (area.x + 1, y))
            .find(|(x, y)| {
                matches!(
                    hit_test(model, *x, *y),
                    Some(Hit::Detail { line: Some(l) } | Hit::Response { line: Some(l) }) if l == line
                )
            })
            .unwrap_or_else(|| panic!("ligne {line} non visible"))
    }

    fn field_line(model: &Model, field: EditableField) -> u16 {
        let Some(TreeNode::Request(request)) = model.selected_node() else {
            panic!("requête attendue");
        };
        let session = model.editing.as_ref().filter(|s| s.path == request.path);
        let (_, fields, _) = request_text_and_fields(request, session);
        let location = fields
            .into_iter()
            .find(|l| l.field == field)
            .expect("champ présent");
        u16::try_from(location.line).expect("petit")
    }

    fn field_point(model: &Model, field: EditableField) -> (u16, u16) {
        line_point(model, true, field_line(model, field))
    }

    fn input_text(model: &Model) -> Option<String> {
        match &model.editing.as_ref()?.state {
            EditState::Input(input) => Some(input.text().to_owned()),
            EditState::FieldSelect | EditState::MethodPicker { .. } => None,
        }
    }

    fn current_field(model: &Model) -> Option<EditableField> {
        model.editing.as_ref()?.current_field()
    }

    // --- Capture ----------------------------------------------------------

    #[test]
    fn toggle_emits_the_opposite_state_and_applies_only_on_success() {
        let mut model = model_on("post-json.bru", (100, 30));
        assert!(matches!(
            update(&mut model, Message::ToggleMouseCapture),
            Command::SetMouseCapture(false)
        ));
        model.mouse.drag = Some(Drag {
            panel: DragPanel::Detail,
            anchor: 1,
            moved: false,
        });
        update(
            &mut model,
            Message::MouseCaptureChanged {
                enabled: false,
                result: Ok(()),
            },
        );
        assert!(!model.mouse.capture);
        assert!(model.mouse.drag.is_none());
        assert!(matches!(
            model.last_status,
            Some(StatusMessage::MouseCapture(false))
        ));
        assert!(matches!(
            update(&mut model, Message::ToggleMouseCapture),
            Command::SetMouseCapture(true)
        ));
        update(
            &mut model,
            Message::MouseCaptureChanged {
                enabled: true,
                result: Err("refusé".into()),
            },
        );
        assert!(!model.mouse.capture);
        assert!(matches!(
            model.last_status,
            Some(StatusMessage::MouseCaptureError(_))
        ));
    }

    #[test]
    fn new_model_starts_without_capture() {
        let model = Model::new("x".into(), (100, 30));
        assert_eq!(model.mouse, MouseState::default());
        assert!(!model.mouse.capture);
    }

    // --- Garde --------------------------------------------------------------

    #[test]
    fn mouse_is_inert_in_guarded_states() {
        let base = || {
            let mut model = model_on("simple-get.bru", (100, 30));
            model.tree.selected = 0;
            model
        };
        let target = |model: &Model| tree_point(model, "post-json.bru");
        type Setup = Box<dyn Fn(&mut Model)>;
        let setups: Vec<(&str, Setup)> = vec![
            ("capture inactive", Box::new(|m| m.mouse.capture = false)),
            (
                "confirmation",
                Box::new(|m| m.confirm = Some(PendingConfirm::QuitWithUnsavedEdit)),
            ),
            (
                "recherche",
                Box::new(|m| {
                    update(m, Message::StartSearch);
                    update(m, Message::SearchInput('x'));
                }),
            ),
            ("diagnostics", Box::new(|m| m.focus = Focus::Diagnostics)),
        ];
        for (name, setup) in setups {
            let mut model = base();
            let point = target(&model);
            setup(&mut model);
            let focus = model.focus;
            for kind in [
                MouseKind::Press,
                MouseKind::Drag,
                MouseKind::Release,
                MouseKind::WheelDown,
            ] {
                event(&mut model, kind, point);
            }
            assert_eq!(model.tree.selected, 0, "{name}");
            assert_eq!(model.focus, focus, "{name}");
            assert_eq!(model.tree.offset, 0, "{name}");
        }

        // Chargement en cours et terminal trop petit.
        let mut loading = Model::new("x".into(), (100, 30));
        loading.mouse.capture = true;
        event(&mut loading, MouseKind::Press, (80, 5));
        assert_eq!(loading.focus, Focus::Tree);
        let mut small = base();
        small.size = (30, 8);
        event(&mut small, MouseKind::Press, (2, 4));
        assert_eq!(small.tree.selected, 0);
    }

    #[test]
    fn clicks_on_status_panel_title_and_status_bar_do_nothing() {
        let mut model = runner_probe_model();
        model.mouse.capture = true;
        select(&mut model, "green.bru");
        let areas = layout_for(model.size).expect("taille");
        let status = areas.response_status;
        for point in [
            (status.x + 2, status.y + 1),
            (1, areas.title.y),
            (1, areas.status.y),
        ] {
            click(&mut model, point);
            event(&mut model, MouseKind::WheelDown, point);
            assert_eq!(model.focus, Focus::Tree);
            assert_eq!(selected_name(&model), "green");
            assert_eq!(model.response_scroll, 0);
        }
    }

    // --- Arbre --------------------------------------------------------------

    #[test]
    fn click_selects_a_node_and_focuses_panels() {
        let mut model = model_on("grp", (100, 30));
        assert_eq!(selected_name(&model), "Groupe");
        let point = tree_point(&model, "post-json.bru");
        click(&mut model, point);
        assert_eq!(selected_name(&model), "post-json");
        assert!(plain_lines(&model).iter().any(|l| l.contains("POST")));

        let response = inner(layout_for(model.size).expect("taille").response);
        click(&mut model, (response.x, response.y));
        assert_eq!(model.focus, Focus::Response);
        let point = tree_point(&model, "simple-get.bru");
        click(&mut model, point);
        assert_eq!(model.focus, Focus::Tree);
    }

    #[test]
    fn click_on_the_selected_folder_toggles_it() {
        let mut model = model_on("grp", (100, 30));
        let point = tree_point(&model, "grp");
        click(&mut model, point);
        assert!(model.tree.expanded.contains(Path::new("grp")));
        assert_eq!(selected_name(&model), "Groupe");
        assert!(row_of(&model, "grp/x.bru") > 0);
        click(&mut model, point);
        assert!(!model.tree.expanded.contains(Path::new("grp")));
        assert_eq!(selected_name(&model), "Groupe");
    }

    #[test]
    fn click_in_a_scrolled_tree_and_on_borders() {
        let mut model = model_on("grp", (100, 11));
        let area = inner(layout_for(model.size).expect("taille").tree);
        model.tree.offset = 4;
        click(&mut model, (area.x + 1, area.y));
        assert_eq!(model.tree.selected, 4);

        let tree = layout_for(model.size).expect("taille").tree;
        model.focus = Focus::Detail;
        click(&mut model, (tree.x, area.y + 1));
        assert_eq!(model.focus, Focus::Tree);
        assert_eq!(model.tree.selected, 4);
    }

    // --- Session d'édition -------------------------------------------------

    fn session_on_post_json() -> Model {
        let mut model = model_on("post-json.bru", (140, 40));
        update(&mut model, Message::NextFocus);
        update(&mut model, Message::Enter);
        assert!(model.editing.is_some());
        model
    }

    #[test]
    fn clean_session_closes_when_another_node_is_clicked() {
        let mut model = session_on_post_json();
        let point = tree_point(&model, "simple-get.bru");
        click(&mut model, point);
        assert!(model.editing.is_none());
        assert_eq!(selected_name(&model), "ping");
        assert_eq!(model.focus, Focus::Tree);
    }

    #[test]
    fn modified_session_refuses_the_clicked_node() {
        let mut model = session_on_post_json();
        move_to(&mut model, EditableField::Url);
        update(&mut model, Message::Enter);
        update(&mut model, Message::InputKey(InputKey::Char('!')));
        update(&mut model, Message::ValidateInput);
        let point = tree_point(&model, "simple-get.bru");
        click(&mut model, point);
        assert_eq!(selected_name(&model), "post-json");
        assert_eq!(model.focus, Focus::Tree);
        assert!(model.editing.as_ref().is_some_and(|s| s.dirty));
        assert!(matches!(model.last_status, Some(StatusMessage::EditLocked)));
    }

    #[test]
    fn input_is_validated_before_a_click_in_the_tree() {
        let mut model = session_on_post_json();
        move_to(&mut model, EditableField::Url);
        update(&mut model, Message::Enter);
        update(&mut model, Message::InputKey(InputKey::Char('!')));
        let point = tree_point(&model, "simple-get.bru");
        click(&mut model, point);
        let session = model.editing.as_ref().expect("session");
        assert!(session.dirty);
        assert_eq!(session.state, EditState::FieldSelect);
        assert_eq!(selected_name(&model), "post-json");
        assert_eq!(model.focus, Focus::Tree);
        assert!(matches!(model.last_status, Some(StatusMessage::EditLocked)));
    }

    #[test]
    fn unchanged_input_ends_on_a_click_in_the_response() {
        let mut model = session_on_post_json();
        move_to(&mut model, EditableField::HeaderValue(0));
        update(&mut model, Message::Enter);
        let response = inner(layout_for(model.size).expect("taille").response);
        click(&mut model, (response.x, response.y));
        let session = model.editing.as_ref().expect("session");
        assert!(!session.dirty);
        assert_eq!(session.state, EditState::FieldSelect);
        assert_eq!(model.focus, Focus::Response);
    }

    #[test]
    fn click_on_the_field_being_edited_changes_nothing() {
        let mut model = session_on_post_json();
        move_to(&mut model, EditableField::Url);
        update(&mut model, Message::Enter);
        update(&mut model, Message::InputKey(InputKey::Left));
        let before = model.editing.clone().map(|s| s.state);
        let point = field_point(&model, EditableField::Url);
        click(&mut model, point);
        event(&mut model, MouseKind::Press, point);
        event(&mut model, MouseKind::Drag, (point.0, point.1 + 2));
        event(&mut model, MouseKind::Release, (point.0, point.1 + 2));
        assert_eq!(model.editing.map(|s| s.state), before);
        assert!(model.detail_selection.is_none());
    }

    fn move_to(model: &mut Model, field: EditableField) {
        let index = model
            .editing
            .as_ref()
            .and_then(|s| s.fields.iter().position(|f| *f == field))
            .expect("champ");
        model.editing.as_mut().expect("session").cursor = index;
    }

    // --- Molette ------------------------------------------------------------

    #[test]
    fn wheel_scrolls_the_hovered_panel_without_focus_change() {
        let mut model = runner_probe_model();
        model.mouse.capture = true;
        model.size = (100, 11);
        select(&mut model, "green.bru");
        assert!(response_max_scroll(&model) >= 3, "réponse assez longue");
        let response = inner(layout_for(model.size).expect("taille").response);
        event(&mut model, MouseKind::WheelDown, (response.x, response.y));
        assert_eq!(model.response_scroll, 3);
        assert_eq!(model.focus, Focus::Tree);
        event(&mut model, MouseKind::WheelUp, (response.x, response.y));
        assert_eq!(model.response_scroll, 0);
    }

    #[test]
    fn wheel_stops_at_the_end_of_the_detail() {
        let mut model = model_on("scripted.bru", (100, 12));
        let max = detail_max_scroll(&model);
        model.detail_scroll = max;
        let detail = inner(layout_for(model.size).expect("taille").detail);
        event(&mut model, MouseKind::WheelDown, (detail.x, detail.y));
        assert_eq!(model.detail_scroll, max);
    }

    #[test]
    fn wheel_in_the_tree_keeps_the_selection() {
        let mut model = model_on("grp", (100, 11));
        model.tree.selected = 0;
        model.tree.offset = 0;
        let area = inner(layout_for(model.size).expect("taille").tree);
        assert!(model.tree.rows.len() > usize::from(area.height) + 3);
        event(&mut model, MouseKind::WheelDown, (area.x, area.y));
        assert_eq!(model.tree.offset, 3);
        assert_eq!(model.tree.selected, 0);
        // Borne basse.
        for _ in 0..20 {
            event(&mut model, MouseKind::WheelDown, (area.x, area.y));
        }
        assert_eq!(
            model.tree.offset,
            model.tree.rows.len() - usize::from(area.height)
        );
        // `↓` ramène la sélection à l'écran.
        update(&mut model, Message::Down);
        assert_eq!(model.tree.selected, 1);
        assert!(model.tree.offset <= 1);
    }

    #[test]
    fn wheel_in_a_session_keeps_the_field_cursor() {
        let mut model = model_on("scripted.bru", (100, 12));
        update(&mut model, Message::NextFocus);
        update(&mut model, Message::Enter);
        let cursor = model.editing.as_ref().map(|s| s.cursor);
        let detail = inner(layout_for(model.size).expect("taille").detail);
        event(&mut model, MouseKind::WheelDown, (detail.x, detail.y));
        assert_eq!(model.detail_scroll, 3);
        assert_eq!(model.editing.as_ref().map(|s| s.cursor), cursor);
    }

    // --- Glisser --------------------------------------------------------------

    #[test]
    fn drag_selects_lines_and_keeps_fixed_bounds() {
        let mut model = model_on("scripted.bru", (140, 14));
        let start = line_point(&model, true, 1);
        let end = line_point(&model, true, 3);
        event(&mut model, MouseKind::Press, start);
        event(&mut model, MouseKind::Drag, end);
        event(&mut model, MouseKind::Release, end);
        assert_eq!(model.focus, Focus::Detail);
        assert_eq!(selection_range(&model), Some(1..=3));
        assert!(model.editing.is_none());

        let detail = inner(layout_for(model.size).expect("taille").detail);
        event(&mut model, MouseKind::WheelDown, (detail.x, detail.y));
        assert_eq!(model.detail_scroll, 3);
        assert_eq!(selection_range(&model), Some(1..=3));
        assert!(model.response_selection.is_none());

        // Un clic sans glisser lève la sélection.
        click(&mut model, (detail.x, detail.y));
        assert!(model.detail_selection.is_none());
    }

    #[test]
    fn drag_beyond_the_panel_scrolls_and_extends() {
        let mut model = runner_probe_model();
        model.mouse.capture = true;
        model.size = (100, 14);
        select(&mut model, "green.bru");
        let response = inner(layout_for(model.size).expect("taille").response);
        event(&mut model, MouseKind::Press, (response.x, response.y));
        event(&mut model, MouseKind::Drag, (response.x, response.bottom()));
        assert_eq!(model.response_scroll, 1);
        let bottom = response_bottom_of_viewport(&model);
        assert_eq!(response_selection_range(&model), Some(0..=bottom));
        event(
            &mut model,
            MouseKind::Drag,
            (response.x, response.bottom() + 3),
        );
        assert_eq!(model.response_scroll, 2);
        assert_eq!(model.focus, Focus::Response);

        // Au-dessus : remonte.
        event(&mut model, MouseKind::Drag, (response.x, response.y - 1));
        assert_eq!(model.response_scroll, 1);
        assert_eq!(response_selection_range(&model), Some(0..=1));
    }

    #[test]
    fn drag_started_on_a_field_opens_no_session() {
        let mut model = model_on("post-json.bru", (140, 40));
        let url = field_line(&model, EditableField::Url);
        let start = line_point(&model, true, url);
        let end = line_point(&model, true, url + 1);
        event(&mut model, MouseKind::Press, start);
        event(&mut model, MouseKind::Drag, end);
        event(&mut model, MouseKind::Release, end);
        assert!(model.editing.is_none());
        assert_eq!(selection_range(&model), Some(url..=url + 1));
    }

    #[test]
    fn orphan_release_does_nothing() {
        let mut model = model_on("post-json.bru", (140, 40));
        let point = field_point(&model, EditableField::Url);
        event(&mut model, MouseKind::Release, point);
        assert!(model.editing.is_none());
        assert_eq!(model.focus, Focus::Tree);
    }

    // --- Saisie d'un champ au clic ------------------------------------------

    #[test]
    fn click_on_the_url_opens_the_session_in_input() {
        let mut model = model_on("post-json.bru", (140, 40));
        let point = field_point(&model, EditableField::Url);
        click(&mut model, point);
        assert_eq!(model.focus, Focus::Detail);
        assert_eq!(current_field(&model), Some(EditableField::Url));
        assert_eq!(
            input_text(&model).as_deref(),
            Some("https://{{host}}/items")
        );
        let EditState::Input(input) = &model.editing.as_ref().expect("session").state else {
            panic!("saisie attendue");
        };
        assert_eq!(input.cursor(), "https://{{host}}/items".chars().count());
    }

    /// Un clic ailleurs referme le sélecteur de méthode sans rien changer,
    /// comme `Échap`, sans laisser la souris inerte tant qu'il est ouvert :
    /// un clic suivant agit normalement sur sa cible (`add-method-editing`).
    #[test]
    fn click_elsewhere_closes_the_method_picker_without_leaving_the_mouse_inert() {
        let mut model = model_on("post-json.bru", (140, 40));
        let method_point = field_point(&model, EditableField::Method);
        click(&mut model, method_point);
        assert!(matches!(
            model.editing.as_ref().expect("session").state,
            EditState::MethodPicker { .. }
        ));

        // Premier clic (même sur la ligne Méthode) : referme le sélecteur,
        // rien d'autre.
        click(&mut model, method_point);
        assert_eq!(
            model.editing.as_ref().expect("session").state,
            EditState::FieldSelect
        );
        assert_eq!(
            field_value(
                model.editing.as_ref().expect("session"),
                &EditableField::Method
            ),
            "POST",
            "méthode inchangée par le clic de fermeture"
        );

        // Second clic, sur l'URL : la souris n'est pas restée bloquée.
        let url_point = field_point(&model, EditableField::Url);
        click(&mut model, url_point);
        assert_eq!(current_field(&model), Some(EditableField::Url));
        assert_eq!(
            input_text(&model).as_deref(),
            Some("https://{{host}}/items")
        );
    }

    #[test]
    fn click_on_a_field_from_field_select_and_from_another_input() {
        let mut model = session_on_post_json();
        let point = field_point(&model, EditableField::HeaderValue(0));
        click(&mut model, point);
        assert_eq!(current_field(&model), Some(EditableField::HeaderValue(0)));
        assert_eq!(input_text(&model).as_deref(), Some("application/json"));

        // Saisie de l'URL modifiée, puis clic sur l'en-tête.
        let mut model = session_on_post_json();
        move_to(&mut model, EditableField::Url);
        update(&mut model, Message::Enter);
        update(&mut model, Message::InputKey(InputKey::Char('!')));
        let point = field_point(&model, EditableField::HeaderValue(0));
        click(&mut model, point);
        let session = model.editing.as_ref().expect("session");
        assert!(session.dirty);
        assert!(
            session
                .pending
                .contains(&FieldEdit::Url("https://{{host}}/items!".into()))
        );
        assert_eq!(current_field(&model), Some(EditableField::HeaderValue(0)));
        assert_eq!(input_text(&model).as_deref(), Some("application/json"));
    }

    #[test]
    fn click_on_the_body_and_on_non_field_lines() {
        let mut model = model_on("post-json.bru", (140, 40));
        let body = field_line(&model, EditableField::BodyText);
        let point = line_point(&model, true, body + 2);
        click(&mut model, point);
        assert_eq!(current_field(&model), Some(EditableField::BodyText));
        assert!(input_text(&model).is_some());

        // Titre de section des en-têtes : focus seul.
        let mut model = model_on("post-json.bru", (140, 40));
        let header = field_line(&model, EditableField::HeaderValue(0));
        let point = line_point(&model, true, header - 1);
        click(&mut model, point);
        assert_eq!(model.focus, Focus::Detail);
        assert!(model.editing.is_none());

        // Dossier : focus seul.
        let mut model = model_on("grp", (140, 40));
        let point = line_point(&model, true, 0);
        click(&mut model, point);
        assert_eq!(model.focus, Focus::Detail);
        assert!(model.editing.is_none());
    }

    #[test]
    fn click_on_a_non_editable_body_starts_no_input() {
        use crate::collection::{BruLoader, CollectionLoader};
        let root =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/collections/writer-cases");
        let mut model = Model::new(root.clone(), (140, 60));
        update(&mut model, Message::CollectionLoaded(BruLoader.load(&root)));
        model.mouse.capture = true;
        select(&mut model, "form-body.bru");
        let lines = plain_lines(&model);
        let body = lines
            .iter()
            .position(|l| l.contains("Corps"))
            .expect("corps");
        assert!(lines[body + 1].trim_start().contains(':'), "{lines:?}");
        let point = line_point(&model, true, u16::try_from(body + 1).expect("petit"));
        click(&mut model, point);
        assert_eq!(model.focus, Focus::Detail);
        assert!(model.editing.is_none());
        assert!(input_text(&model).is_none());
    }

    /// Clic sur la ligne « aucun » d'une section vide hors session :
    /// ouvre la session et démarre l'ajout sur cette section.
    #[test]
    fn click_on_empty_section_row_without_session_starts_add() {
        let mut model = model_on("simple-get.bru", (140, 40));
        let point = field_point(&model, EditableField::AddRow(EntrySection::Headers));
        click(&mut model, point);
        assert_eq!(model.focus, Focus::Detail);
        let session = model.editing.as_ref().expect("session");
        assert!(matches!(session.state, EditState::Input(_)));
        assert!(matches!(
            session.target,
            InputTarget::NewKey {
                section: EntrySection::Headers,
                ..
            }
        ));
        assert_eq!(input_text(&model).as_deref(), Some(""));
    }

    /// Clic sur la ligne « aucun » pour chacune des trois sections :
    /// chacune ouvre l'ajout sur la bonne section et non sur une autre.
    #[test]
    fn click_on_empty_section_row_starts_add_for_correct_section() {
        for section in [
            EntrySection::Headers,
            EntrySection::QueryParams,
            EntrySection::PathParams,
        ] {
            let mut model = model_on("simple-get.bru", (140, 40));
            let point = field_point(&model, EditableField::AddRow(section));
            click(&mut model, point);
            assert_eq!(model.focus, Focus::Detail);
            let session = model.editing.as_ref().expect("session");
            assert!(matches!(session.state, EditState::Input(_)));
            assert!(
                matches!(session.target, InputTarget::NewKey { section: s, .. } if s == section),
                "l'ajout doit démarrer sur {section:?}"
            );
            assert_eq!(input_text(&model).as_deref(), Some(""));
        }
    }

    /// Section ayant déjà une entrée, sans session ouverte :
    /// un clic sous la dernière entrée (cadre inférieur de section)
    /// ne fait que donner le focus au détail sans ouvrir de session.
    #[test]
    fn click_under_existing_entry_without_session_only_focuses_detail() {
        let mut model = model_on("post-json.bru", (140, 40));
        let header = field_line(&model, EditableField::HeaderValue(0));
        let point = line_point(&model, true, header + 1);
        click(&mut model, point);
        assert_eq!(model.focus, Focus::Detail);
        assert!(model.editing.is_none());
    }

    // --- Copie ----------------------------------------------------------------

    #[test]
    fn y_copies_the_dragged_lines() {
        let mut model = runner_probe_model();
        model.mouse.capture = true;
        model.size = (100, 30);
        select(&mut model, "green.bru");
        let start = line_point(&model, false, 1);
        let end = line_point(&model, false, 2);
        event(&mut model, MouseKind::Press, start);
        event(&mut model, MouseKind::Drag, end);
        // Relâchement : aucune copie.
        assert!(matches!(
            event(&mut model, MouseKind::Release, end),
            Command::None
        ));
        let lines = response_plain_lines(&model);
        let Command::CopyToClipboard { token, text } = update(&mut model, Message::Yank) else {
            panic!("copie attendue");
        };
        assert_eq!(text, format!("{}\n{}", lines[1], lines[2]));

        // Échec : sélection et défilement inchangés, échec signalé.
        let scroll = model.response_scroll;
        update(
            &mut model,
            Message::ClipboardResult {
                token,
                result: Err(crate::app::clipboard::test_support::error_for_test()),
            },
        );
        assert_eq!(model.response_scroll, scroll);
        assert_eq!(response_selection_range(&model), Some(1..=2));
        assert!(matches!(
            model.last_status,
            Some(StatusMessage::ClipboardError(_))
        ));

        // Nouvelle tentative réussie : sélection levée, confirmation.
        let Command::CopyToClipboard { token, .. } = update(&mut model, Message::Yank) else {
            panic!("copie attendue");
        };
        update(
            &mut model,
            Message::ClipboardResult {
                token,
                result: Ok(()),
            },
        );
        assert!(model.response_selection.is_none());
        assert_eq!(model.response_scroll, scroll);
        assert!(matches!(model.last_status, Some(StatusMessage::Copied)));
    }

    #[test]
    fn y_in_field_select_copies_without_starting_input() {
        let mut model = session_on_post_json();
        let start = line_point(&model, true, 0);
        let end = line_point(&model, true, 1);
        event(&mut model, MouseKind::Press, start);
        event(&mut model, MouseKind::Drag, end);
        event(&mut model, MouseKind::Release, end);
        assert!(matches!(
            update(&mut model, Message::Yank),
            Command::CopyToClipboard { .. }
        ));
        assert_eq!(
            model.editing.as_ref().map(|s| s.state.clone()),
            Some(EditState::FieldSelect)
        );
    }

    // --- Non-régression clavier ---------------------------------------------

    #[test]
    fn keyboard_navigation_is_unchanged_with_capture() {
        let mut with = loaded_model((100, 30));
        with.mouse.capture = true;
        let mut without = loaded_model((100, 30));
        for message in [
            || Message::Down,
            || Message::Right,
            || Message::Left,
            || Message::Down,
        ] {
            update(&mut with, message());
            update(&mut without, message());
            assert_eq!(with.tree.selected, without.tree.selected);
            assert_eq!(with.tree.expanded, without.tree.expanded);
        }
    }

    // --- Panneau et popup Environnement (`add-environment-panel-and-edit-popup`) --

    /// Position écran de l'entrée `index` du panneau permanent Environnement.
    fn environment_panel_point(model: &Model, index: usize) -> (u16, u16) {
        let areas = layout_for(model.size).expect("taille");
        let area = inner(areas.environment);
        (area.x + 1, area.y + u16::try_from(index).expect("petit"))
    }

    /// Position écran de la ligne `row` (0 = en-tête) du popup d'édition
    /// ouvert sur `session`.
    fn environment_popup_point(
        model: &Model,
        session: &EnvironmentEditSession,
        row: usize,
    ) -> (u16, u16) {
        let popup =
            environment_edit_popup_area(Rect::new(0, 0, model.size.0, model.size.1), session);
        let area = inner(popup);
        (area.x + 1, area.y + u16::try_from(row).expect("petit"))
    }

    fn environment_model() -> Model {
        let mut model = loaded_model((100, 30));
        model.mouse.capture = true;
        model
    }

    #[test]
    fn click_on_environment_panel_activates_without_opening_popup() {
        let mut model = environment_model();
        let point = environment_panel_point(&model, 1); // "local"
        click(&mut model, point);
        assert_eq!(model.focus, Focus::EnvironmentPicker);
        assert_eq!(model.current_environment.as_deref(), Some("local"));
        assert!(model.environment_editing.is_none());
    }

    #[test]
    fn double_click_on_environment_panel_activates_and_opens_popup() {
        let mut model = environment_model();
        let point = environment_panel_point(&model, 1); // "local"
        click(&mut model, point);
        click(&mut model, point);
        assert_eq!(model.current_environment.as_deref(), Some("local"));
        let session = model.environment_editing.as_ref().expect("popup ouvert");
        assert_eq!(session.name, "local");
    }

    #[test]
    fn clicks_far_apart_in_time_do_not_open_the_popup() {
        let mut model = environment_model();
        let point = environment_panel_point(&model, 1);
        click(&mut model, point);
        // Simule un premier clic ancien : hors de la fenêtre de double-clic.
        model.mouse.last_environment_click = Some((Instant::now() - Duration::from_secs(1), 1));
        click(&mut model, point);
        assert!(model.environment_editing.is_none());
    }

    #[test]
    fn rapid_clicks_on_different_entries_do_not_open_the_popup() {
        let mut model = environment_model();
        let local = environment_panel_point(&model, 1); // "local"
        click(&mut model, local);
        let staging = environment_panel_point(&model, 3); // "staging"
        click(&mut model, staging);
        assert!(model.environment_editing.is_none());
        assert_eq!(model.current_environment.as_deref(), Some("staging"));
    }

    #[test]
    fn click_on_none_or_error_entry_in_the_panel() {
        let mut model = environment_model();
        model.current_environment = Some("local".into());
        // « Aucun » (indice 0) : désactive l'environnement courant.
        let none_point = environment_panel_point(&model, 0);
        click(&mut model, none_point);
        assert_eq!(model.current_environment, None);
        // Entrée en erreur (indice 2 : malformed) : sans effet.
        model.current_environment = Some("local".into());
        let error_point = environment_panel_point(&model, 2);
        click(&mut model, error_point);
        assert_eq!(model.current_environment.as_deref(), Some("local"));
    }

    #[test]
    fn click_in_the_popup_starts_editing_the_clicked_variable() {
        let mut model = environment_model();
        model.focus = Focus::EnvironmentPicker;
        model.environment_selected = 1; // "local"
        update(&mut model, Message::StartEdit);
        let session = model.environment_editing.clone().expect("popup ouvert");
        assert_eq!(session.cursor, 0);

        // "debug" est la seconde variable (indice 1), sur la ligne 2 du
        // popup (0 : en-tête, 1 : host, 2 : debug) — désactivée.
        let point = environment_popup_point(&model, &session, 2);
        click(&mut model, point);
        let session = model.environment_editing.as_ref().expect("popup ouvert");
        assert_eq!(session.state, EnvironmentEditState::Select, "désactivée");

        // "host" (indice 0), sur la ligne 1 du popup : activée.
        let point = environment_popup_point(&model, &session.clone(), 1);
        click(&mut model, point);
        let session = model.environment_editing.as_ref().expect("popup ouvert");
        assert_eq!(session.cursor, 0);
        assert!(matches!(session.state, EnvironmentEditState::Input(_)));
    }

    #[test]
    fn click_in_the_popup_while_editing_another_variable_commits_first() {
        let mut model = environment_model();
        model.focus = Focus::EnvironmentPicker;
        model.environment_selected = 1; // "local"
        update(&mut model, Message::StartEdit);
        update(&mut model, Message::StartEdit); // démarre la saisie sur "host"
        for c in "!!!".chars() {
            update(&mut model, Message::InputKey(InputKey::Char(c)));
        }

        let session = model.environment_editing.clone().expect("popup ouvert");
        // "debug" est désactivée : cliquer dessus ne démarre pas de
        // nouvelle saisie, mais valide déjà la saisie en cours sur "host".
        let point = environment_popup_point(&model, &session, 2);
        click(&mut model, point);
        let session = model.environment_editing.as_ref().expect("popup ouvert");
        assert_eq!(session.variables[0].value, "localhost:3000!!!");
        assert_eq!(session.state, EnvironmentEditState::Select);
    }

    #[test]
    fn click_outside_the_popup_has_no_effect() {
        let mut model = environment_model();
        model.focus = Focus::EnvironmentPicker;
        model.environment_selected = 1;
        update(&mut model, Message::StartEdit);

        // Un clic sur l'arbre, dessiné dessous mais capturé par le popup
        // modal, ne change rien (`add-environment-panel-and-edit-popup`,
        // design D6).
        let point = tree_point(&model, "grp");
        click(&mut model, point);
        assert!(model.environment_editing.is_some());
        assert_eq!(model.focus, Focus::EnvironmentPicker);
    }
}
