//! Transitions du modèle.
//!
//! `update` est la seule fonction qui modifie le modèle. Elle ne fait
//! aucune I/O : le chargement et le rendu sont pilotés par la boucle.

use super::filter::{FilterState, evaluate};
use super::global_search::GlobalSearchPopup;
use super::message::{InputKey, Message, MouseKind};
use super::model::{
    CampaignComparison, CampaignFailure, CampaignFilter, CampaignItem, CampaignItemStatus,
    CampaignSummary, CampaignView, CollectionState, EditSession, EditState, EditableField, EnvRun,
    EnvRunOutcome, EnvSeries, EnvironmentEditSession, Exit, Focus, HistoryEntry, HistoryOutcome,
    InputTarget, Model, ResponseTab, RunnerOptionsField, RunnerOptionsPopup, SecretError,
    SecretInput, StatusMessage, field_enabled, field_value, field_value_committed, secret_env_vars,
    secret_env_vars_for_env, secret_lookups, secret_rows, secret_rows_for_env,
    secrets_panel_environment, section_entries, selected_response_body, visible_rows,
};
use super::text_input::TextInput;
use super::tree_filter::{TreeFilterState, filtered_rows, find_node_by_path, matches_tree_filter};
use super::view::detail::detail_text_fields_boxes;
use super::view::hit::{Hit, hit_test};
use super::view::{help_max_scroll, help_popup_area, inner, layout_for};
use crate::collection::TreeNode;
use crate::runner::report::ResponseStatus;
use crate::runner::{RunRequest, SecretString};
use crate::secrets::{SecretLookup, is_valid_name};
use crate::writer::{EntrySection, FieldEdit, FileStamp};
use ratatui::layout::{Position, Rect};
use ratatui::text::Line;

pub mod environment;
pub mod find;
pub mod navigation;
pub mod output;
pub mod pointer;
pub mod scroll;
#[cfg(test)]
pub(super) mod test_support;

pub(crate) use self::pointer::*;

pub(crate) use self::environment::*;
use self::find::{confirm_search, repeat_search, start_search};
pub(crate) use self::navigation::*;
use self::scroll::{
    detail_max_scroll, next_response_tab, previous_response_tab, response_max_scroll,
    scroll_detail, scroll_response, select_request_tab,
};
pub(crate) use self::scroll::{response_selection_range, selection_range};

use self::output::{
    apply_clipboard_result, copy_curl, copy_curl_full, start_export_response_body, toggle_visual,
    yank,
};
pub use self::output::{resolve_editor, resolve_editor_with};

/// Effet demandé par `update`, à exécuter par la boucle `run`, qui seule
/// détient le `BruRunner` et tourne dans un contexte tokio.
#[derive(Debug)]
pub enum Command {
    None,
    /// Démarrer cette exécution ; la boucle appelle `BruRunner::start` puis
    /// renvoie le résultat à `update` via `Message::RunStarted`.
    StartRun {
        request: Box<RunRequest>,
        target: std::path::PathBuf,
        is_campaign: bool,
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
    /// Ouvrir un bloc de texte dans un éditeur externe ; la boucle l'exécute
    /// en suspendant ratatui et renvoie le contenu relu à `update` via
    /// `Message::TextEditorClosed`.
    EditTextInEditor {
        program: String,
        text: String,
        extension: String,
        field: EditableField,
    },
    /// Recharger la collection depuis `model.source` ; la boucle l'exécute
    /// dans `spawn_blocking` via `CollectionLoader::load` et renvoie le
    /// résultat à `update` via `Message::CollectionLoaded`.
    ReloadCollection,
    /// Exporter le corps brut de la réponse dans un fichier ; la boucle
    /// l'exécute dans `spawn_blocking` via `std::fs::write` et renvoie
    /// le résultat à `update` via `Message::ResponseBodyExported`.
    ExportResponseBody {
        path: std::path::PathBuf,
        bytes: Vec<u8>,
    },
    /// Regroupe plusieurs commandes à exécuter séquentiellement par la boucle.
    Batch(Vec<Command>),
    /// Charger l'historique persistant depuis le disque.
    LoadHistory {
        root: std::path::PathBuf,
    },
    /// Sauvegarder l'historique persistant sur le disque.
    SaveHistory {
        root: std::path::PathBuf,
        history: Vec<super::model::HistoryEntry>,
    },
}

/// Applique un message au modèle.
pub fn update(model: &mut Model, message: Message) -> Command {
    let had_session = model.editing.is_some() || model.environment_editing.is_some();
    let mut command = apply(model, message);
    // Pendant une session d'édition, l'onglet affiché du détail suit le
    // champ sous le curseur : il devient l'onglet choisi, pour que le
    // détail ne saute pas d'onglet à la fin de la session.
    if model.editing.is_some() {
        model.request_tab = model.shown_request_tab();
    }
    // Si une session d'édition vient de se fermer et qu'un rechargement était en attente :
    if had_session
        && model.editing.is_none()
        && model.environment_editing.is_none()
        && model.pending_reload
    {
        model.pending_reload = false;
        if matches!(command, Command::None) {
            command = Command::ReloadCollection;
        }
    }
    command
}

fn apply(model: &mut Model, message: Message) -> Command {
    // Un statut (copie, campagne terminée…) masque le rappel des touches :
    // il ne dure que jusqu'à l'action suivante de l'utilisateur. Le
    // traitement du message peut ensuite en poser un nouveau.
    if is_user_action(&message) {
        model.last_status = None;
    }
    if model.confirm.is_some() {
        return match message {
            Message::ConfirmYes | Message::Yank | Message::Right | Message::Enter => {
                let confirm = model.confirm.take().unwrap();
                match confirm {
                    super::model::PendingConfirm::DiscardEdit => {
                        model.editing = None;
                        Command::None
                    }
                    super::model::PendingConfirm::QuitWithUnsavedEdit => {
                        model.exit = Some(Exit::Normal);
                        Command::None
                    }
                    super::model::PendingConfirm::CopyUnmaskedCurl => copy_curl(model, false),
                    super::model::PendingConfirm::OverwriteExport { path, bytes } => {
                        Command::ExportResponseBody { path, bytes }
                    }
                }
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
                model.confirm = None;
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

    // Seules les entrées de l'utilisateur sont interceptées : les résultats
    // asynchrones (exécution, chargement, copie, sauvegarde) doivent être
    // traités même popup ouvert, sinon une exécution resterait active.
    if model.help.is_some() && !is_background_result(&message) {
        return match message {
            Message::ForceQuit => {
                model.help = None;
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
                let max = help_max_scroll(model);
                if let Some(help) = &mut model.help {
                    help.scroll = help.scroll.min(max);
                }
                Command::None
            }
            Message::ToggleHelp | Message::FocusTree | Message::Quit => {
                model.help = None;
                Command::None
            }
            Message::Up => {
                if let Some(help) = &mut model.help {
                    help.scroll = help.scroll.saturating_sub(1);
                }
                Command::None
            }
            Message::Down => {
                let max = help_max_scroll(model);
                if let Some(help) = &mut model.help {
                    help.scroll = help.scroll.saturating_add(1).min(max);
                }
                Command::None
            }
            Message::Home => {
                if let Some(help) = &mut model.help {
                    help.scroll = 0;
                }
                Command::None
            }
            Message::End => {
                let max = help_max_scroll(model);
                if let Some(help) = &mut model.help {
                    help.scroll = max;
                }
                Command::None
            }
            Message::Mouse(input) => {
                if input.kind == MouseKind::Press {
                    let popup_area = help_popup_area(Rect::new(0, 0, model.size.0, model.size.1));
                    let position = Position::new(input.column, input.row);
                    if !popup_area.contains(position) {
                        model.help = None;
                    }
                }
                Command::None
            }
            _ => Command::None,
        };
    }

    if model.runner_options_popup.is_some() && !is_background_result(&message) {
        return apply_runner_options_popup(model, message);
    }

    if model.global_search.is_some() && !is_background_result(&message) {
        return apply_global_search(model, message);
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
            let mut commands = Vec::new();
            if let Ok(collection) = &result {
                let lookups = secret_lookups(&model.secrets.mappings, collection);
                if !lookups.is_empty() {
                    commands.push(Command::ResolveSecrets {
                        root: collection.root.clone(),
                        lookups,
                    });
                }
                if model.history_enabled {
                    commands.push(Command::LoadHistory {
                        root: collection.root.clone(),
                    });
                }
            }
            let command = match commands.len() {
                0 => Command::None,
                1 => commands.pop().unwrap(),
                _ => Command::Batch(commands),
            };
            let is_reloading = matches!(model.collection, CollectionState::Loaded(_));
            // Échec d'un rechargement automatique (fichier momentanément
            // invalide pendant l'enregistrement d'un éditeur, par exemple) :
            // l'ancienne collection reste affichée, seul le statut signale l'échec.
            if is_reloading && let Err(error) = &result {
                model.last_status =
                    Some(super::model::StatusMessage::ReloadFailed(error.to_string()));
                return Command::None;
            }
            let prev_selected_path = model.selected_node().map(|n| n.path().to_path_buf());

            match result {
                Ok(collection) => {
                    // Conserver l'environnement actif s'il existe toujours
                    if let Some(ref current_env) = model.current_environment {
                        let still_exists = collection
                            .environments
                            .iter()
                            .any(|e| e.as_ref().is_ok_and(|env| &env.name == current_env));
                        if !still_exists {
                            model.current_environment = None;
                        }
                    }

                    model.collection = CollectionState::Loaded(collection);
                    reset_secrets_for_collection(model);
                    // Conserver les dossiers dépliés (`expanded` n'est pas vidé) ;
                    // un filtre actif est levé en restaurant l'état d'avant filtre.
                    if let Some(filter) = model.tree_filter.take() {
                        model.tree.expanded = filter.saved_expanded;
                    }
                    refresh_rows(model);

                    // Conserver la sélection par chemin
                    if let Some(prev_path) = prev_selected_path {
                        if let Some(index) = model.tree.rows.iter().position(|row| {
                            model
                                .node_at(&row.address)
                                .is_some_and(|n| n.path() == prev_path)
                        }) {
                            model.tree.selected = index;
                        } else {
                            model.tree.selected = model
                                .tree
                                .selected
                                .min(model.tree.rows.len().saturating_sub(1));
                        }
                    } else {
                        model.tree.selected = 0;
                    }
                    scroll_tree_into_view(model);
                    if is_reloading {
                        model.detail_scroll = model.detail_scroll.min(detail_max_scroll(model));
                        model.last_status = Some(super::model::StatusMessage::CollectionReloaded);
                    } else {
                        model.detail_scroll = 0;
                    }
                }
                Err(error) => {
                    model.collection = CollectionState::Failed(error);
                    reset_secrets_for_collection(model);
                    model.tree.expanded.clear();
                    model.tree_filter = None;
                    refresh_rows(model);
                    model.tree.selected = 0;
                    model.tree.offset = 0;
                    model.detail_scroll = 0;
                    model.current_environment = None;
                    if matches!(model.focus, Focus::EnvironmentPicker | Focus::Secrets) {
                        model.focus = Focus::Tree;
                    }
                }
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
        Message::ToggleHelp => {
            model.help = Some(super::model::HelpPopup::default());
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
        Message::Rename => match model.focus {
            Focus::Tree => {
                model.selected_collection = !model.selected_collection;
                model.detail_scroll = 0;
                Command::None
            }
            Focus::Detail => {
                start_rename(model);
                Command::None
            }
            Focus::Response => copy_curl(model, true),
            _ => Command::None,
        },

        Message::CopyCurlFull => {
            if model.focus == Focus::Response {
                copy_curl_full(model)
            } else {
                Command::None
            }
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
            if model.zoom {
                scroll_tree_into_view(model);
                model.detail_scroll = model.detail_scroll.min(detail_max_scroll(model));
                model.response_scroll = model.response_scroll.min(response_max_scroll(model));
            }
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
                    model.confirm = Some(crate::app::model::PendingConfirm::DiscardEdit);
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
                    if model.focus == Focus::Tree && has_active_tree_filter(model) {
                        clear_tree_filter(model);
                        scroll_tree_into_view(model);
                    } else {
                        model.zoom = false;
                        model.focus = Focus::Tree;
                        scroll_tree_into_view(model);
                        model.detail_scroll = model.detail_scroll.min(detail_max_scroll(model));
                        model.response_scroll =
                            model.response_scroll.min(response_max_scroll(model));
                    }
                }
            }
            Command::None
        }
        Message::ToggleZoom => {
            if model.zoom {
                model.zoom = false;
                scroll_tree_into_view(model);
                model.detail_scroll = model.detail_scroll.min(detail_max_scroll(model));
                model.response_scroll = model.response_scroll.min(response_max_scroll(model));
            } else if matches!(model.focus, Focus::Tree | Focus::Detail | Focus::Response) {
                model.zoom = true;
                scroll_tree_into_view(model);
                model.detail_scroll = model.detail_scroll.min(detail_max_scroll(model));
                model.response_scroll = model.response_scroll.min(response_max_scroll(model));
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
            toggle_environment_picker(model);
            Command::None
        }
        Message::ToggleRunnerOptions => {
            if model.loaded().is_some() {
                model.runner_options_popup = match model.runner_options_popup.take() {
                    Some(_) => None,
                    None => Some(RunnerOptionsPopup::from_config(
                        &model.runner_options,
                        model.loaded(),
                    )),
                };
            }
            Command::None
        }
        Message::ToggleGlobalSearch => {
            if model.loaded().is_some() {
                model.global_search = match model.global_search.take() {
                    Some(_) => None,
                    None => Some(GlobalSearchPopup::new()),
                };
            }
            Command::None
        }
        Message::RunSelected if model.focus == Focus::Secrets => launch_pending_run(model),
        Message::RunSelected => run_selected(model),
        // `R` n'agit que depuis le panneau Campagne : ailleurs (dont une
        // session d'édition), une majuscule tapée par erreur ne relance rien.
        Message::RunFailedRequests if model.focus == Focus::Campaign => run_failed_requests(model),
        Message::RunFailedRequests => Command::None,
        Message::ToggleCampaignView => {
            toggle_campaign_view(model);
            Command::None
        }
        Message::CycleCampaignFilter => {
            cycle_campaign_filter(model);
            Command::None
        }
        Message::CancelRun => {
            cancel_run(model);
            Command::None
        }
        Message::RunStarted {
            id,
            target,
            recursive,
            is_campaign,
            handle,
        } => {
            model.run.active = Some(super::model::ActiveRun {
                id,
                target,
                recursive,
                is_campaign,
                handle: Some(handle),
            });
            Command::None
        }
        Message::RunFinished(event) => run_finished(model, event),
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
        Message::OpenTreeFilter => {
            if model.focus == Focus::Campaign {
                cycle_campaign_filter(model);
            } else if model.focus == Focus::Tree && model.loaded().is_some() {
                // Filtrer peut déplacer la sélection : refusé tant qu'une
                // édition non enregistrée la verrouille.
                if selection_locked(model) {
                    model.last_status = Some(StatusMessage::EditLocked);
                } else {
                    open_tree_filter(model);
                }
            }
            Command::None
        }
        Message::TreeFilterInput(c) => {
            if let Some(filter) = &mut model.tree_filter
                && filter.editing
            {
                filter.draft.push(c);
                refresh_rows(model);
                scroll_tree_into_view(model);
            }
            Command::None
        }
        Message::TreeFilterBackspace => {
            if let Some(filter) = &mut model.tree_filter
                && filter.editing
            {
                filter.draft.pop();
                if filter.draft.is_empty() {
                    model.tree.expanded = filter.saved_expanded.clone();
                }
                refresh_rows(model);
                scroll_tree_into_view(model);
            }
            Command::None
        }
        Message::ConfirmTreeFilter => {
            confirm_tree_filter(model);
            Command::None
        }
        Message::CancelTreeFilter => {
            cancel_tree_filter(model);
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
                enter_in_detail(model)
            } else if model.focus == Focus::EnvironmentPicker {
                navigate_environment_picker(model, Message::Enter);
                Command::None
            } else if model.focus == Focus::Campaign
                && model.campaign_view == CampaignView::Comparison
                && model.comparison.is_some()
            {
                // Dans le tableau, `→` change de colonne : `Entrée` ouvre la cellule.
                navigate_comparison(model, Message::Enter);
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
        Message::TextEditorClosed { field, result } => {
            match result {
                Ok(new_text) => {
                    let current = model
                        .editing
                        .as_ref()
                        .map(|s| field_value_committed(s, &field))
                        .unwrap_or("");
                    if current != new_text {
                        let edit = match field {
                            EditableField::ScriptPreRequest => {
                                FieldEdit::ScriptPreRequest(new_text)
                            }
                            EditableField::ScriptPostResponse => {
                                FieldEdit::ScriptPostResponse(new_text)
                            }
                            EditableField::Tests => FieldEdit::Tests(new_text),
                            EditableField::Docs => FieldEdit::Docs(new_text),
                            _ => return Command::None,
                        };
                        try_commit(model, edit);
                    }
                }
                Err(error) => {
                    model.last_status = Some(StatusMessage::EditorError(error));
                }
            }
            Command::None
        }
        Message::SaveEdit => {
            if read_only_collection(model)
                && matches!(model.focus, Focus::Detail | Focus::EnvironmentPicker)
            {
                model.last_status = Some(StatusMessage::ReadOnlyCollection);
                return Command::None;
            }
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
                program: resolve_editor(model.editor.as_deref()),
                text,
            },
            None => {
                model.last_status = Some(StatusMessage::NoResponseBody);
                Command::None
            }
        },
        Message::ExportResponseBody => {
            if model.focus == Focus::Response {
                start_export_response_body(model);
            }
            Command::None
        }
        Message::ExportPathInput(c) => {
            if let Some(state) = &mut model.export_input {
                state.input.insert(c);
            }
            Command::None
        }
        Message::ExportPathBackspace => {
            if let Some(state) = &mut model.export_input {
                state.input.backspace();
            }
            Command::None
        }
        Message::ExportPathDelete => {
            if let Some(state) = &mut model.export_input {
                state.input.delete();
            }
            Command::None
        }
        Message::ExportPathLeft => {
            if let Some(state) = &mut model.export_input {
                state.input.left();
            }
            Command::None
        }
        Message::ExportPathRight => {
            if let Some(state) = &mut model.export_input {
                state.input.right();
            }
            Command::None
        }
        Message::ExportPathHome => {
            if let Some(state) = &mut model.export_input {
                state.input.home();
            }
            Command::None
        }
        Message::ExportPathEnd => {
            if let Some(state) = &mut model.export_input {
                state.input.end();
            }
            Command::None
        }
        Message::CancelExportPath => {
            model.export_input = None;
            Command::None
        }
        Message::ConfirmExportPath => {
            if let Some(state) = model.export_input.take() {
                let trimmed = state.input.text().trim();
                if trimmed.is_empty() {
                    Command::None
                } else {
                    let path = std::path::PathBuf::from(trimmed);
                    if path.exists() {
                        model.confirm = Some(super::model::PendingConfirm::OverwriteExport {
                            path,
                            bytes: state.bytes,
                        });
                        Command::None
                    } else {
                        Command::ExportResponseBody {
                            path,
                            bytes: state.bytes,
                        }
                    }
                }
            } else {
                Command::None
            }
        }
        Message::ResponseBodyExported { path, result } => {
            match result {
                Ok(()) => {
                    model.last_status = Some(StatusMessage::ResponseExported(path));
                }
                Err(reason) => {
                    model.last_status = Some(StatusMessage::ExportError(reason));
                }
            }
            Command::None
        }
        Message::HistoryLoaded {
            root,
            entries,
            notice,
        } => {
            let is_matching_col = model.loaded().is_some_and(|col| col.root == root);
            if is_matching_col {
                model.history = std::collections::VecDeque::from(entries);
                model.history_selected = 0;
                if let Some(msg) = notice
                    && let Some(col) = model.loaded_mut()
                {
                    col.notices.push(crate::collection::LoadNotice {
                        path: std::path::PathBuf::from("history.json"),
                        message: msg,
                    });
                }
            }
            Command::None
        }
        Message::Mouse(input) => {
            // Le bouton « Lancer » déclenche une exécution, seule action
            // souris qui produit une commande.
            if input.kind == MouseKind::Press
                && mouse_accepted(model)
                && hit_test(model, input.column, input.row) == Some(Hit::RunButton)
            {
                model.mouse.drag = None;
                return run_selected(model);
            }
            mouse(model, input)
        }
        Message::ToggleMouseCapture => Command::SetMouseCapture(!model.mouse.capture),
        Message::WatchReload(paths) => handle_watch_reload(model, paths),
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
                    // Hors session : ←/→ changent d'onglet de requête ou dossier/collection.
                    None => match navigation {
                        Message::Left => {
                            if model.is_folder_or_collection_selected() {
                                select_request_tab(
                                    model,
                                    crate::app::model::previous_folder_tab(
                                        model.shown_request_tab(),
                                    ),
                                );
                            } else {
                                select_request_tab(model, model.request_tab.previous());
                            }
                        }
                        Message::Right => {
                            if model.is_folder_or_collection_selected() {
                                select_request_tab(
                                    model,
                                    crate::app::model::next_folder_tab(model.shown_request_tab()),
                                );
                            } else {
                                select_request_tab(model, model.request_tab.next());
                            }
                        }
                        other => scroll_detail(model, other),
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
        Focus::Tree | Focus::Detail | Focus::Response => {
            if model.selected_collection {
                Some((std::path::PathBuf::new(), true))
            } else {
                match model.selected_node() {
                    // `bru run` 4.2.1 n'exécute ni gRPC ni WebSocket : rien
                    // n'est lancé (voir `tests/fixtures/reports/grpc-ws.json`).
                    Some(TreeNode::Request(request))
                        if let Some(kind) = request.view.non_http_label() =>
                    {
                        model.last_status = Some(StatusMessage::NotExecutable(kind));
                        return Command::None;
                    }
                    Some(TreeNode::Request(request)) => Some((request.path.clone(), false)),
                    Some(TreeNode::Folder(folder)) => Some((folder.path.clone(), true)),
                    Some(TreeNode::Error(_)) | None => None,
                }
            }
        }

        Focus::Campaign => {
            let target = model
                .campaign
                .as_ref()
                .map(|camp| camp.target.clone())
                .or_else(|| match model.selected_node() {
                    Some(TreeNode::Folder(folder)) => Some(folder.path.clone()),
                    _ => Some(std::path::PathBuf::new()),
                })
                .unwrap_or_default();
            Some((target, true))
        }
        Focus::Diagnostics | Focus::EnvironmentPicker | Focus::Secrets => None,
    };
    let Some((target, recursive)) = target else {
        return Command::None;
    };
    if model.loaded().is_none() {
        return Command::None;
    }
    begin_run(model, target, recursive)
}

/// Déclenche une campagne récursive depuis le popup d'options.
fn run_campaign_from_popup(model: &mut Model) -> Command {
    if model.run.active.is_some() || model.loaded().is_none() {
        return Command::None;
    }
    let target = match model.focus {
        Focus::Campaign => model.campaign.as_ref().map(|camp| camp.target.clone()),
        _ => match model.selected_node() {
            Some(TreeNode::Folder(folder)) => Some(folder.path.clone()),
            Some(TreeNode::Request(request)) => request
                .path
                .parent()
                .map(|p| p.to_path_buf())
                .or_else(|| Some(std::path::PathBuf::new())),
            _ => Some(std::path::PathBuf::new()),
        },
    }
    .unwrap_or_default();
    begin_run(model, target, true)
}

/// Traite les messages lorsque le popup d'options de campagne est ouvert.
fn apply_runner_options_popup(model: &mut Model, message: Message) -> Command {
    match message {
        Message::ForceQuit => {
            model.runner_options_popup = None;
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
        Message::ToggleRunnerOptions | Message::Quit => {
            if let Some(popup) = &mut model.runner_options_popup
                && let Ok(config) = popup.to_config()
            {
                model.runner_options = config;
            }
            model.runner_options_popup = None;
            Command::None
        }
        Message::FocusTree | Message::CancelInput => {
            if let Some(popup) = &mut model.runner_options_popup {
                if popup.editing_text {
                    popup.editing_text = false;
                    popup.error = None;
                } else {
                    if let Ok(config) = popup.to_config() {
                        model.runner_options = config;
                    }
                    model.runner_options_popup = None;
                }
            }
            Command::None
        }
        Message::ValidateInput | Message::NextFocus => {
            if let Some(popup) = &mut model.runner_options_popup {
                if let Err(err) = popup.to_config() {
                    popup.error = Some(err.to_string());
                } else {
                    popup.error = None;
                    popup.editing_text = false;
                    popup.next_field();
                }
            }
            Command::None
        }
        Message::InputKey(key) => {
            if let Some(popup) = &mut model.runner_options_popup
                && popup.editing_text
            {
                match key {
                    InputKey::Char(c) => {
                        if let Some(input) = popup.active_text_input_mut() {
                            input.insert(c);
                        }
                    }
                    InputKey::Backspace => {
                        if let Some(input) = popup.active_text_input_mut() {
                            input.backspace();
                        }
                    }
                    InputKey::Delete => {
                        if let Some(input) = popup.active_text_input_mut() {
                            input.delete();
                        }
                    }
                    InputKey::Left => {
                        if let Some(input) = popup.active_text_input_mut() {
                            input.left();
                        }
                    }
                    InputKey::Right => {
                        if let Some(input) = popup.active_text_input_mut() {
                            input.right();
                        }
                    }
                    InputKey::Home => {
                        if let Some(input) = popup.active_text_input_mut() {
                            input.home();
                        }
                    }
                    InputKey::End => {
                        if let Some(input) = popup.active_text_input_mut() {
                            input.end();
                        }
                    }
                    InputKey::Up => {
                        popup.editing_text = false;
                        popup.prev_field();
                    }
                    InputKey::Down => {
                        popup.editing_text = false;
                        popup.next_field();
                    }
                    InputKey::Enter => {
                        if let Err(err) = popup.to_config() {
                            popup.error = Some(err.to_string());
                        } else {
                            popup.error = None;
                            popup.editing_text = false;
                        }
                    }
                }
            }
            Command::None
        }
        Message::Up => {
            if let Some(popup) = &mut model.runner_options_popup {
                popup.prev_field();
            }
            Command::None
        }
        Message::Down => {
            if let Some(popup) = &mut model.runner_options_popup {
                popup.next_field();
            }
            Command::None
        }
        Message::ToggleField => {
            if let Some(popup) = &mut model.runner_options_popup
                && popup.current_field().is_toggle()
            {
                popup.toggle_current();
            }
            Command::None
        }
        Message::Enter => {
            let Some(popup) = &mut model.runner_options_popup else {
                return Command::None;
            };
            match popup.current_field() {
                RunnerOptionsField::Environment(_) => {
                    popup.toggle_current();
                    Command::None
                }
                RunnerOptionsField::Tags
                | RunnerOptionsField::ExcludeTags
                | RunnerOptionsField::Delay
                | RunnerOptionsField::Cacert
                | RunnerOptionsField::ClientCertConfig
                | RunnerOptionsField::CsvFilePath
                | RunnerOptionsField::JsonFilePath
                | RunnerOptionsField::IterationCount
                | RunnerOptionsField::EnvFile
                | RunnerOptionsField::GlobalEnv
                | RunnerOptionsField::WorkspacePath => {
                    if popup.editing_text {
                        if let Err(err) = popup.to_config() {
                            popup.error = Some(err.to_string());
                        } else {
                            popup.error = None;
                            popup.editing_text = false;
                        }
                    } else {
                        popup.editing_text = true;
                    }
                    Command::None
                }
                RunnerOptionsField::Bail
                | RunnerOptionsField::TestsOnly
                | RunnerOptionsField::Insecure
                | RunnerOptionsField::IgnoreTruststore
                | RunnerOptionsField::NoProxy
                | RunnerOptionsField::DisableCookies
                | RunnerOptionsField::Parallel
                | RunnerOptionsField::Sandbox
                | RunnerOptionsField::Verbose
                | RunnerOptionsField::ReporterJunit
                | RunnerOptionsField::ReporterHtml
                | RunnerOptionsField::SkipHeaders => {
                    popup.toggle_current();
                    Command::None
                }
                RunnerOptionsField::SaveButton => match popup.to_config() {
                    Ok(config) => {
                        model.runner_options = config;
                        model.runner_options_popup = None;
                        model.last_status = Some(StatusMessage::RunnerOptionsSaved);
                        Command::None
                    }
                    Err(err) => {
                        popup.error = Some(err.to_string());
                        Command::None
                    }
                },
                RunnerOptionsField::RunButton => match popup.to_config() {
                    Ok(config) => {
                        model.runner_options = config;
                        model.runner_options_popup = None;
                        run_campaign_from_popup(model)
                    }
                    Err(err) => {
                        popup.error = Some(err.to_string());
                        Command::None
                    }
                },
            }
        }
        Message::RunSelected => {
            let Some(popup) = &mut model.runner_options_popup else {
                return Command::None;
            };
            match popup.to_config() {
                Ok(config) => {
                    model.runner_options = config;
                    model.runner_options_popup = None;
                    run_campaign_from_popup(model)
                }
                Err(err) => {
                    popup.error = Some(err.to_string());
                    Command::None
                }
            }
        }
        Message::Mouse(input) => {
            if input.kind == MouseKind::Press {
                let popup_area = super::view::runner_options_popup_area(Rect::new(
                    0,
                    0,
                    model.size.0,
                    model.size.1,
                ));
                let position = Position::new(input.column, input.row);
                if !popup_area.contains(position) {
                    if let Some(popup) = &mut model.runner_options_popup
                        && let Ok(config) = popup.to_config()
                    {
                        model.runner_options = config;
                    }
                    model.runner_options_popup = None;
                }
            }
            Command::None
        }
        _ => Command::None,
    }
}

/// Traite les messages lorsque le popup de recherche globale est ouvert.
fn apply_global_search(model: &mut Model, message: Message) -> Command {
    let candidates = model
        .loaded()
        .map(|c| crate::app::global_search::collect_candidates(&c.tree))
        .unwrap_or_default();
    let query = model
        .global_search
        .as_ref()
        .map(|p| p.input.text().to_owned())
        .unwrap_or_default();
    let results = crate::app::global_search::search(&candidates, &query);
    let visible_height = super::view::global_search::global_search_visible_height(model);

    match message {
        Message::ForceQuit => {
            model.global_search = None;
            if model.editing.as_ref().is_some_and(EditSession::has_unsaved) {
                model.confirm = Some(super::model::PendingConfirm::QuitWithUnsavedEdit);
            } else {
                model.exit = Some(Exit::Normal);
            }
            Command::None
        }
        Message::ToggleGlobalSearch | Message::CancelGlobalSearch => {
            model.global_search = None;
            Command::None
        }
        Message::GlobalSearchInput(c) => {
            if let Some(popup) = &mut model.global_search {
                popup.input.insert(c);
                popup.reset_selection();
            }
            Command::None
        }
        Message::GlobalSearchBackspace => {
            if let Some(popup) = &mut model.global_search {
                popup.input.backspace();
                popup.reset_selection();
            }
            Command::None
        }
        Message::GlobalSearchDelete => {
            if let Some(popup) = &mut model.global_search {
                popup.input.delete();
                popup.reset_selection();
            }
            Command::None
        }
        Message::GlobalSearchLeft => {
            if let Some(popup) = &mut model.global_search {
                popup.input.left();
            }
            Command::None
        }
        Message::GlobalSearchRight => {
            if let Some(popup) = &mut model.global_search {
                popup.input.right();
            }
            Command::None
        }
        Message::GlobalSearchHome => {
            if let Some(popup) = &mut model.global_search {
                popup.input.home();
            }
            Command::None
        }
        Message::GlobalSearchEnd => {
            if let Some(popup) = &mut model.global_search {
                popup.input.end();
            }
            Command::None
        }
        Message::GlobalSearchUp => {
            if let Some(popup) = &mut model.global_search {
                popup.selected = popup.selected.saturating_sub(1);
                popup.ensure_visible(visible_height);
            }
            Command::None
        }
        Message::GlobalSearchDown => {
            if let Some(popup) = &mut model.global_search {
                if !results.is_empty() {
                    popup.selected = (popup.selected + 1).min(results.len() - 1);
                }
                popup.ensure_visible(visible_height);
            }
            Command::None
        }
        Message::GlobalSearchPageUp => {
            if let Some(popup) = &mut model.global_search {
                popup.selected = popup.selected.saturating_sub(visible_height);
                popup.ensure_visible(visible_height);
            }
            Command::None
        }
        Message::GlobalSearchPageDown => {
            if let Some(popup) = &mut model.global_search {
                if !results.is_empty() {
                    popup.selected = (popup.selected + visible_height).min(results.len() - 1);
                }
                popup.ensure_visible(visible_height);
            }
            Command::None
        }
        Message::ConfirmGlobalSearch => {
            let selected_path = model.global_search.as_ref().and_then(|popup| {
                results
                    .get(popup.selected)
                    .map(|r| r.candidate.path.clone())
            });
            model.global_search = None;
            if let Some(target) = selected_path {
                select_request_and_focus_detail(model, &target);
            }
            Command::None
        }
        Message::Mouse(input) => {
            if input.kind == MouseKind::Press {
                let popup_area = super::view::global_search_popup_area(Rect::new(
                    0,
                    0,
                    model.size.0,
                    model.size.1,
                ));
                let position = Position::new(input.column, input.row);
                if !popup_area.contains(position) {
                    model.global_search = None;
                } else {
                    let list_y = popup_area.y + 3;
                    if input.row >= list_y {
                        let row_offset = (input.row - list_y) as usize;
                        if let Some(popup) = &model.global_search {
                            let clicked_idx = popup.scroll + row_offset;
                            if let Some(res) = results.get(clicked_idx) {
                                let target = res.candidate.path.clone();
                                model.global_search = None;
                                select_request_and_focus_detail(model, &target);
                            }
                        }
                    }
                }
            }
            Command::None
        }
        Message::Resize { width, height } => {
            model.size = (width, height);
            scroll_tree_into_view(model);
            model.detail_scroll = model.detail_scroll.min(detail_max_scroll(model));
            Command::None
        }
        _ => Command::None,
    }
}

/// Lance une exécution : propose d'abord les variables secrètes manquantes,
/// puis, pour une campagne avec des environnements cochés, une série d'un
/// `bru run` par environnement ; sinon une exécution unique sur
/// l'environnement courant.
fn begin_run(model: &mut Model, target: std::path::PathBuf, recursive: bool) -> Command {
    // Ni seule ni en série : `bru run` 4.2.1 n'exécute pas gRPC/WebSocket.
    if let Some(kind) = non_executable_kind(model, &target) {
        model.last_status = Some(StatusMessage::NotExecutable(kind));
        return Command::None;
    }
    let environments = if recursive {
        campaign_environments(model)
    } else {
        Vec::new()
    };
    if environments.is_empty() {
        if should_propose_secrets(model, model.current_environment.as_deref()) {
            propose_secrets(model, target, recursive, None, Vec::new());
            return Command::None;
        }
        return start_run(model, target, recursive);
    }
    if let Some(env) = next_environment_to_propose(model, &environments) {
        propose_secrets(model, target, true, Some(env), environments);
        return Command::None;
    }
    start_series(model, target, environments)
}

/// Libellé du type si `path` désigne une requête gRPC ou WebSocket.
fn non_executable_kind(model: &Model, path: &std::path::Path) -> Option<&'static str> {
    let collection = model.loaded()?;
    match find_node_by_path(&collection.tree, path)? {
        TreeNode::Request(request) => request.view.non_http_label(),
        _ => None,
    }
}

/// Environnements cochés dans les options de campagne et encore déclarés
/// par la collection, dans l'ordre de la collection.
fn campaign_environments(model: &Model) -> Vec<String> {
    let Some(collection) = model.loaded() else {
        return Vec::new();
    };
    collection
        .environments
        .iter()
        .flatten()
        .map(|env| &env.name)
        .filter(|name| model.runner_options.environments.contains(name))
        .cloned()
        .collect()
}

/// Démarre une série : la première exécution part tout de suite, les
/// suivantes à la réception de l'issue de la précédente (`run_finished`).
fn start_series(
    model: &mut Model,
    target: std::path::PathBuf,
    environments: Vec<String>,
) -> Command {
    model.comparison = None;
    model.series = Some(EnvSeries::new(target, environments));
    start_series_step(model)
}

/// Lance l'exécution de l'environnement courant de la série.
fn start_series_step(model: &mut Model) -> Command {
    let Some(series) = &model.series else {
        return Command::None;
    };
    let target = series.target.clone();
    let env = series.current_environment().map(str::to_owned);
    let command = start_run_in(model, target, true, env);
    if matches!(command, Command::None) {
        model.series = None;
    }
    command
}

/// Exécution sur l'environnement courant.
fn start_run(model: &mut Model, target: std::path::PathBuf, recursive: bool) -> Command {
    let env = model.current_environment.clone();
    start_run_in(model, target, recursive, env)
}

/// Construit l'exécution sur la cible et l'environnement donnés, avec les
/// options de campagne et les variables secrètes en vigueur à cet instant.
fn start_run_in(
    model: &mut Model,
    target: std::path::PathBuf,
    recursive: bool,
    env: Option<String>,
) -> Command {
    let Some(root) = model.loaded().map(|collection| collection.root.clone()) else {
        return Command::None;
    };
    model.run.last_failure = None;
    // `--env-file` remplace l'environnement nommé pour une campagne.
    let env = if recursive && model.runner_options.env_file.is_some() {
        None
    } else {
        env
    };
    let (options, reports) = if recursive {
        model
            .runner_options
            .build_runner_options(&model.reports_dir)
    } else {
        (crate::runner::RunnerOptions::default(), Vec::new())
    };
    // Une série cumule les rapports exportés de chacune de ses exécutions.
    if model
        .series
        .as_ref()
        .is_some_and(|series| series.current > 0)
    {
        model.last_reports.extend(reports);
    } else {
        model.last_reports = reports;
    }
    let targets = if target.as_os_str().is_empty() {
        Vec::new()
    } else {
        vec![target.clone()]
    };
    Command::StartRun {
        request: Box::new(RunRequest {
            collection_root: root,
            targets,
            recursive,
            env_vars: secret_env_vars_for_env(model, env.as_deref()),
            env,
            options,
        }),
        target,
        is_campaign: recursive,
    }
}

/// Lance une exécution ciblée uniquement sur les requêtes en échec de la dernière campagne,
/// avec les options de campagne courantes et sans récursion.
fn start_run_failures(
    model: &mut Model,
    target: std::path::PathBuf,
    failure_paths: Vec<std::path::PathBuf>,
) -> Command {
    let Some(root) = model.loaded().map(|collection| collection.root.clone()) else {
        return Command::None;
    };
    model.run.last_failure = None;
    let (options, reports) = model
        .runner_options
        .build_runner_options(&model.reports_dir);
    model.last_reports = reports;
    Command::StartRun {
        request: Box::new(RunRequest {
            collection_root: root,
            targets: failure_paths,
            recursive: false,
            // `--env-file` remplace l'environnement nommé.
            env: model
                .current_environment
                .clone()
                .filter(|_| model.runner_options.env_file.is_none()),
            env_vars: secret_env_vars(model),
            options,
        }),
        target,
        is_campaign: true,
    }
}

/// Action « relancer seulement les échecs » : lance un unique `bru run` avec les chemins
/// des requêtes en échec en arguments positionnels et les options de campagne courantes.
fn run_failed_requests(model: &mut Model) -> Command {
    if model.run.active.is_some() || model.loaded().is_none() {
        return Command::None;
    }
    let Some(campaign) = &model.campaign else {
        model.last_status = Some(StatusMessage::NoFailedRequests);
        return Command::None;
    };
    let mut failure_paths: Vec<std::path::PathBuf> = campaign
        .items
        .iter()
        .filter(|item| item.status == CampaignItemStatus::Failed)
        .map(|item| item.path.clone())
        .collect();
    if failure_paths.is_empty() && !campaign.failures.is_empty() {
        failure_paths = campaign.failures.iter().map(|f| f.path.clone()).collect();
    }
    let target = campaign.target.clone();
    // Les échecs gRPC/WebSocket ne sont pas relancés : `bru run` ne les exécute pas.
    let mut skipped_kind = None;
    failure_paths.retain(|path| match non_executable_kind(model, path) {
        Some(kind) => {
            skipped_kind = Some(kind);
            false
        }
        None => true,
    });
    if failure_paths.is_empty() {
        model.last_status = Some(match skipped_kind {
            Some(kind) => StatusMessage::NotExecutable(kind),
            None => StatusMessage::NoFailedRequests,
        });
        return Command::None;
    }
    if should_propose_secrets(model, model.current_environment.as_deref()) {
        propose_secrets(model, target, true, None, Vec::new());
        return Command::None;
    }
    start_run_failures(model, target, failure_paths)
}

/// Fait tourner le filtre de statut de la campagne (Échecs -> Réussis -> Ignorés -> Tous -> Échecs)
/// et repositionne le curseur en tête de la liste. Dans le tableau
/// comparatif, fait tourner son propre filtre, qui ajoute « Divergences ».
fn cycle_campaign_filter(model: &mut Model) {
    if let Some(comparison) = comparison_shown_mut(model) {
        comparison.filter = comparison.filter.next();
        model.campaign_selected = 0;
        return;
    }
    if let Some(campaign) = &mut model.campaign {
        campaign.filter = campaign.filter.next();
        model.campaign_selected = 0;
    }
}

/// Vrai si l'environnement donné, non encore acquitté, déclare un
/// `vars:secret` resté sans valeur.
fn should_propose_secrets(model: &Model, env: Option<&str>) -> bool {
    let Some(env) = env else {
        return false;
    };
    !model.secrets.acknowledged.contains(env)
        && secret_rows_for_env(model, Some(env))
            .iter()
            .any(|row| row.declared && row.value.is_none())
}

/// Premier environnement de la série à qui proposer des secrets.
fn next_environment_to_propose(model: &Model, environments: &[String]) -> Option<String> {
    environments
        .iter()
        .find(|env| should_propose_secrets(model, Some(env)))
        .cloned()
}

/// Retient l'exécution et ouvre le panneau des secrets sur la première
/// variable déclarée non fournie de `prompt_env` (l'environnement courant
/// à défaut). `pending_envs` porte les environnements d'une série.
fn propose_secrets(
    model: &mut Model,
    target: std::path::PathBuf,
    recursive: bool,
    prompt_env: Option<String>,
    pending_envs: Vec<String>,
) {
    let secrets = &mut model.secrets;
    secrets.pending_run = Some((target, recursive));
    secrets.prompt_env = prompt_env;
    secrets.pending_envs = pending_envs;
    secrets.input = None;
    secrets.error = None;
    model.secrets.selected = secret_rows(model)
        .iter()
        .position(|row| row.declared && row.value.is_none())
        .unwrap_or(0);
    model.focus = Focus::Secrets;
}

/// `r` dans le panneau des variables secrètes : acquitte l'environnement
/// proposé, puis passe à l'environnement suivant de la série qui manque de
/// secrets, ou lance l'exécution en attente avec les valeurs alors résolues.
fn launch_pending_run(model: &mut Model) -> Command {
    if model.run.active.is_some() {
        return Command::None;
    }
    let Some((target, recursive)) = model.secrets.pending_run.take() else {
        return Command::None;
    };
    acknowledge_environment(model);
    model.secrets.input = None;
    model.secrets.prompt_env = None;
    let environments = std::mem::take(&mut model.secrets.pending_envs);
    if environments.is_empty() {
        model.focus = Focus::Tree;
        return start_run(model, target, recursive);
    }
    if let Some(env) = next_environment_to_propose(model, &environments) {
        propose_secrets(model, target, recursive, Some(env), environments);
        return Command::None;
    }
    model.focus = Focus::Tree;
    start_series(model, target, environments)
}

/// Acquitte l'environnement montré par le panneau des secrets.
fn acknowledge_environment(model: &mut Model) {
    if let Some(env) = secrets_panel_environment(model).map(str::to_owned) {
        model.secrets.acknowledged.insert(env);
    }
}

/// `Échap` ou `S` : ferme le panneau ; une exécution en attente (ou toute
/// la série) est abandonnée et l'environnement proposé acquitté.
fn close_secrets(model: &mut Model) {
    if model.secrets.pending_run.take().is_some() {
        acknowledge_environment(model);
    }
    model.secrets.prompt_env = None;
    model.secrets.pending_envs.clear();
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
    secrets.prompt_env = None;
    secrets.pending_envs.clear();
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
        // Une série n'enchaîne pas les environnements suivants.
        if let Some(series) = &mut model.series {
            series.cancelled = true;
        }
    }
}

/// Applique l'issue d'une exécution, en ignorant un identifiant obsolète.
/// Touche ou clic de l'utilisateur, par opposition aux résultats de
/// tâches de fond et aux événements du terminal (redimensionnement,
/// glisser ou relâchement de la souris).
fn is_user_action(message: &Message) -> bool {
    match message {
        Message::Resize { .. } | Message::TerminalClosed(_) => false,
        Message::Mouse(input) => input.kind == MouseKind::Press,
        other => !is_background_result(other),
    }
}

/// Message issu d'une tâche de fond plutôt que d'une action de
/// l'utilisateur : il est toujours traité, quel que soit l'overlay ouvert.
fn is_background_result(message: &Message) -> bool {
    matches!(
        message,
        Message::CollectionLoaded(_)
            | Message::RunStarted { .. }
            | Message::RunFinished(_)
            | Message::ClipboardResult { .. }
            | Message::EditSaved { .. }
            | Message::EnvironmentSaved { .. }
            | Message::MouseCaptureChanged { .. }
            | Message::SecretsResolved { .. }
            | Message::WatchReload(_)
            | Message::ResponseBodyExported { .. }
            | Message::HistoryLoaded { .. }
    )
}

/// Applique l'issue d'une exécution ; dans une série, renvoie le lancement
/// de l'environnement suivant.
fn run_finished(model: &mut Model, event: crate::runner::RunEvent) -> Command {
    let matches_active = model
        .run
        .active
        .as_ref()
        .is_some_and(|active| active.id == event.id);
    if !matches_active {
        return Command::None;
    }
    let Some(active) = model.run.active.take() else {
        return Command::None;
    };
    // Environnement de la série dont c'est l'issue, et l'issue à conserver
    // pour le tableau comparatif (`None` pour une annulation).
    let series_env = model
        .series
        .as_ref()
        .filter(|_| active.recursive)
        .and_then(|series| series.current_environment().map(str::to_owned));
    let series_run = series_env.as_ref().map(|env| {
        let outcome = match &event.outcome {
            crate::runner::RunOutcome::Completed {
                report,
                exit_code,
                output,
            } => Some(EnvRunOutcome::Completed {
                report: report.clone(),
                exit_code: *exit_code,
                console: std::sync::Arc::from(output.as_str()),
            }),
            crate::runner::RunOutcome::Failed(error) => {
                Some(EnvRunOutcome::Failed(error.to_string()))
            }
            crate::runner::RunOutcome::Cancelled => None,
        };
        outcome.map(|outcome| EnvRun {
            environment: env.clone(),
            outcome,
        })
    });

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

    if (active.recursive || active.is_campaign)
        && let crate::runner::RunOutcome::Completed { ref report, .. } = event.outcome
    {
        let iterations = report.iterations();
        let total_iterations = iterations.len();
        let results_with_iteration: Vec<_> = iterations
            .iter()
            .flat_map(|it| it.results.iter().map(move |r| (it.iteration_index, r)))
            .collect();
        let total = results_with_iteration.len();
        let failed = results_with_iteration
            .iter()
            .filter(|(_, r)| r.is_failure())
            .count();
        let skipped = results_with_iteration
            .iter()
            .filter(|(_, r)| {
                (r.skipped || r.status == crate::runner::report::ResultStatus::Skipped)
                    && !r.is_failure()
            })
            .count();
        let passed = total.saturating_sub(failed + skipped);
        let duration_secs: f64 = results_with_iteration
            .iter()
            .map(|(_, r)| r.run_duration)
            .sum();

        let items: Vec<CampaignItem> = results_with_iteration
            .iter()
            .map(|(iter_idx, r)| {
                let status = if r.is_failure() {
                    CampaignItemStatus::Failed
                } else if r.skipped || r.status == crate::runner::report::ResultStatus::Skipped {
                    CampaignItemStatus::Skipped
                } else {
                    CampaignItemStatus::Passed
                };
                let reason = match status {
                    CampaignItemStatus::Failed => Some(extract_failure_reason(r)),
                    CampaignItemStatus::Skipped => r
                        .response
                        .status_text
                        .clone()
                        .or_else(|| Some("ignoré".to_owned())),
                    CampaignItemStatus::Passed => None,
                };
                let http_code = match r.response.status {
                    ResponseStatus::Http(code) => Some(code),
                    _ => None,
                };
                CampaignItem {
                    path: std::path::PathBuf::from(&r.test.filename),
                    name: r.name.clone(),
                    status,
                    http_code,
                    duration_secs: r.run_duration,
                    reason,
                    iteration_index: *iter_idx,
                }
            })
            .collect();

        let failures = items
            .iter()
            .filter(|i| i.status == CampaignItemStatus::Failed)
            .map(|i| CampaignFailure {
                path: i.path.clone(),
                name: i.name.clone(),
                http_code: i.http_code,
                reason: i.reason.clone().unwrap_or_else(|| "échec".to_owned()),
            })
            .collect();

        // Une campagne hors série remplace la comparaison précédente.
        if series_env.is_none() {
            model.comparison = None;
        }
        model.campaign = Some(CampaignSummary {
            target: active.target.clone(),
            total,
            passed,
            failed,
            skipped,
            duration_secs,
            items,
            filter: CampaignFilter::default(),
            failures,
            total_iterations,
        });
        model.campaign_selected = 0;

        let failures_text = if failed <= 1 {
            format!("{failed} échec")
        } else {
            format!("{failed} échecs")
        };
        let reports_text = if !model.last_reports.is_empty() {
            let paths: Vec<_> = model
                .last_reports
                .iter()
                .map(|p| p.display().to_string())
                .collect();
            format!(" · Rapports : {}", paths.join(", "))
        } else {
            String::new()
        };
        let status_text = format!(
            "Campagne {} : {passed}/{total} réussis · {failures_text}{reports_text} — C pour le détail",
            active.target.display()
        );
        model.last_status = Some(StatusMessage::CampaignFinished(status_text));
    }

    match event.outcome {
        crate::runner::RunOutcome::Completed {
            report,
            exit_code,
            output,
        } => {
            // Console de toute l'exécution : récursive, ou relance de
            // plusieurs requêtes en échec (`R`).
            let is_global_console = active.recursive || active.is_campaign;
            let output: std::sync::Arc<str> = std::sync::Arc::from(output);
            let total_iterations = report.iterations().len() as u32;
            for iteration in report.0 {
                let current_iteration = iteration.iteration_index + 1;
                for result in iteration.results {
                    let key = std::path::PathBuf::from(&result.test.filename);
                    model.run.outcomes.insert(
                        key,
                        super::model::RequestOutcome::with_console(
                            result,
                            exit_code,
                            std::sync::Arc::clone(&output),
                            is_global_console,
                        )
                        .with_iteration(current_iteration, total_iterations)
                        .with_environment(series_env.clone()),
                    );
                }
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
    // Série : l'exécution suivante prime. L'historique complet est
    // enregistré par la dernière issue de la série.
    if let Some(run) = series_run {
        let next = advance_series(model, run);
        if !matches!(next, Command::None) {
            return next;
        }
    }
    if model.history_enabled
        && let Some(col) = model.loaded()
    {
        Command::SaveHistory {
            root: col.root.clone(),
            history: model.history.iter().cloned().collect(),
        }
    } else {
        Command::None
    }
}

/// Range l'issue d'un environnement de la série, puis lance le suivant,
/// sauf annulation (`run` vaut `None`, ou annulation demandée) ou fin de
/// série.
fn advance_series(model: &mut Model, run: Option<EnvRun>) -> Command {
    let Some(series) = &mut model.series else {
        return Command::None;
    };
    let stop = run.is_none() || series.cancelled;
    if let Some(run) = run {
        series.runs.push(run);
    }
    if !stop && series.current + 1 < series.environments.len() {
        series.current += 1;
        return start_series_step(model);
    }
    finish_series(model);
    Command::None
}

/// Clôt la série : construit le tableau comparatif à partir d'au moins deux
/// environnements exécutés et résume les verdicts dans la barre d'état.
fn finish_series(model: &mut Model) {
    let Some(series) = model.series.take() else {
        return;
    };
    let planned = series.environments.len();
    let done = series.runs.len();
    let mut parts: Vec<String> = series
        .runs
        .iter()
        .map(|run| match &run.outcome {
            EnvRunOutcome::Completed { report, .. } => {
                let results: Vec<_> = report
                    .iterations()
                    .iter()
                    .flat_map(|iteration| &iteration.results)
                    .collect();
                let failed = results.iter().filter(|r| r.is_failure()).count();
                format!(
                    "{} : {} échec(s) sur {}",
                    run.environment,
                    failed,
                    results.len()
                )
            }
            EnvRunOutcome::Failed(_) => format!("{} : erreur", run.environment),
        })
        .collect();
    if done < planned {
        parts.push(format!("interrompue après {done}/{planned}"));
    }
    let hint = if done >= 2 {
        " — C puis t pour comparer"
    } else {
        ""
    };
    let target = if series.target.as_os_str().is_empty() {
        "collection entière".to_owned()
    } else {
        series.target.display().to_string()
    };
    let text = format!(
        "Campagne multi-environnements {target} : {}{hint}",
        parts.join(" · ")
    );
    if done >= 2 {
        model.comparison = Some(CampaignComparison::build(series.target, series.runs));
        model.campaign_view = CampaignView::Comparison;
        model.campaign_selected = 0;
        model.comparison_column = 0;
    }
    model.last_status = Some(StatusMessage::CampaignFinished(text));
}

/// Détermine la première raison d'échec d'une requête exécutée.
fn extract_failure_reason(result: &crate::runner::report::RequestResult) -> String {
    if let Some(error) = &result.error {
        return error.clone();
    }
    for assertion in &result.assertion_results {
        if assertion.status.is_failure() {
            // `rhsExpr` contient déjà l'opérateur (`eq 404`) : le répéter
            // donnait « res.status eq eq 404 ».
            let expr = format!("{} {}", assertion.lhs_expr, assertion.rhs_expr);
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
        let found_same = previous_path.as_ref().and_then(|path| {
            model
                .tree
                .rows
                .iter()
                .position(|r| model.node_at(&r.address).is_some_and(|n| n.path() == path))
        });

        if let Some(pos) = found_same {
            model.tree.selected = pos;
        } else {
            let first_match = model.tree.rows.iter().position(|r| {
                model
                    .node_at(&r.address)
                    .is_some_and(|n| matches_tree_filter(n, p))
            });
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

fn navigate_tree(model: &mut Model, message: Message) {
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
fn select_row(model: &mut Model, index: usize) {
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
pub(super) fn clear_detail_view_state(model: &mut Model) {
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
fn read_only_collection(model: &Model) -> bool {
    model
        .loaded()
        .is_some_and(|collection| !collection.format.is_editable())
}

/// Ouvre une session d'édition sur la requête, le dossier ou la collection sélectionné,
/// dans l'état Sélection de champ.
fn start_edit(model: &mut Model) {
    if model.focus != Focus::Detail || model.editing.is_some() {
        return;
    }
    if read_only_collection(model) {
        model.last_status = Some(StatusMessage::ReadOnlyCollection);
        return;
    }
    let Some(collection) = model.loaded() else {
        return;
    };
    let root = &collection.root;
    let (path, view) = if model.selected_collection {
        let path = std::path::PathBuf::from("collection.bru");
        let Some(Ok(meta)) = collection.settings.as_ref() else {
            return;
        };
        (path, meta.to_request_view())
    } else {
        match model.selected_node() {
            Some(TreeNode::Request(request)) => {
                if let Some(kind) = request.view.non_http_label() {
                    model.last_status = Some(StatusMessage::ReadOnlyRequestType(kind));
                    return;
                }
                (request.path.clone(), request.view.clone())
            }
            Some(TreeNode::Folder(folder)) => {
                let path = folder.path.join("folder.bru");
                let Some(Ok(meta)) = folder.meta.as_ref() else {
                    return;
                };
                (path, meta.to_request_view())
            }
            _ => return,
        }
    };
    let full_path = root.join(&path);
    let Ok(stamp) = FileStamp::capture(&full_path) else {
        return;
    };
    model.editing = Some(EditSession::new(path, stamp, view));
    model.last_status = None;
    scroll_edit_into_view(model);
}

/// `Entrée` dans le détail : ouvre la session, ou commence la saisie du
/// champ sous le curseur (`improve-direct-editing`, D3), ou ouvre l'éditeur
/// externe pour un bloc de texte.
fn enter_in_detail(model: &mut Model) -> Command {
    match &model.editing {
        None => {
            start_edit(model);
            Command::None
        }
        Some(session) if session.state == EditState::FieldSelect => begin_input(model),
        Some(session) if matches!(session.state, EditState::MethodPicker { .. }) => {
            confirm_method_picker(model);
            Command::None
        }
        Some(_) => Command::None,
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
        EditableField::VarsPreRequestValue(index) => {
            FieldEdit::VarsPreRequestEnabled { index, enabled }
        }
        EditableField::VarsPostResponseValue(index) => {
            FieldEdit::VarsPostResponseEnabled { index, enabled }
        }
        EditableField::AssertValue(index) => FieldEdit::AssertEnabled { index, enabled },
        EditableField::SettingsValue(_)
        | EditableField::Method
        | EditableField::Url
        | EditableField::AddRow(_)
        | EditableField::BodyText
        | EditableField::ScriptPreRequest
        | EditableField::ScriptPostResponse
        | EditableField::Tests
        | EditableField::Docs => return,
    };
    try_commit(model, edit);
}

/// `Entrée` en sélection de champ : saisie de la valeur du champ sous le
/// curseur, ajout d'une entrée sur une ligne « + Ajouter », ou ouverture
/// de l'éditeur externe pour un bloc de texte.
fn begin_input(model: &mut Model) -> Command {
    let Some(session) = &model.editing else {
        return Command::None;
    };
    if session.state != EditState::FieldSelect {
        return Command::None;
    }
    let Some(field) = session.current_field() else {
        return Command::None;
    };
    match field {
        EditableField::AddRow(section) => {
            start_add(model, section);
            Command::None
        }
        EditableField::Method => {
            open_method_picker(model);
            Command::None
        }
        EditableField::ScriptPreRequest
        | EditableField::ScriptPostResponse
        | EditableField::Tests
        | EditableField::Docs => {
            let extension = match field {
                EditableField::Docs => "md",
                _ => "js",
            };
            let text = field_value_committed(session, &field).to_string();
            let program = resolve_editor(model.editor.as_deref());
            Command::EditTextInEditor {
                program,
                text,
                extension: extension.to_string(),
                field,
            }
        }
        _ => {
            let input = TextInput::new(
                field_value(session, &field),
                field == EditableField::BodyText,
            );
            open_input(model, input, InputTarget::Field);
            Command::None
        }
    }
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
    if section == EntrySection::Settings {
        return;
    }
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
    if section == EntrySection::Settings {
        return;
    }
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
    if section == EntrySection::Settings {
        return;
    }
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
            EntrySection::VarsPreRequest => {
                EditableField::VarsPreRequestValue(count.saturating_sub(1))
            }
            EntrySection::VarsPostResponse => {
                EditableField::VarsPostResponseValue(count.saturating_sub(1))
            }
            EntrySection::Assert => EditableField::AssertValue(count.saturating_sub(1)),
            EntrySection::Settings => EditableField::SettingsValue(count.saturating_sub(1)),
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
        EditableField::VarsPreRequestValue(index) => {
            FieldEdit::VarsPreRequestValue { index, value }
        }
        EditableField::VarsPostResponseValue(index) => {
            FieldEdit::VarsPostResponseValue { index, value }
        }
        EditableField::AssertValue(index) => FieldEdit::AssertValue { index, value },
        EditableField::SettingsValue(index) => FieldEdit::SettingsValue { index, value },
        EditableField::BodyText => FieldEdit::BodyText(value),
        EditableField::ScriptPreRequest => FieldEdit::ScriptPreRequest(value),
        EditableField::ScriptPostResponse => FieldEdit::ScriptPostResponse(value),
        EditableField::Tests => FieldEdit::Tests(value),
        EditableField::Docs => FieldEdit::Docs(value),
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

/// Journal de la session augmenté de `edit`, avec une seule coalescence :
/// deux modifications de valeur consécutives du même champ.
fn with_edit(pending: &[FieldEdit], edit: FieldEdit) -> Vec<FieldEdit> {
    let mut edits = pending.to_vec();
    let same_value_target = match (edits.last(), &edit) {
        (Some(FieldEdit::Url(_)), FieldEdit::Url(_))
        | (Some(FieldEdit::BodyText(_)), FieldEdit::BodyText(_))
        | (Some(FieldEdit::ScriptPreRequest(_)), FieldEdit::ScriptPreRequest(_))
        | (Some(FieldEdit::ScriptPostResponse(_)), FieldEdit::ScriptPostResponse(_))
        | (Some(FieldEdit::Tests(_)), FieldEdit::Tests(_))
        | (Some(FieldEdit::Docs(_)), FieldEdit::Docs(_)) => true,
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

/// Retourne l'AST du fichier actuellement édité (`request`, `folder.bru` ou `collection.bru`),
/// si la sélection et le chemin correspondent à la session.
fn current_editing_ast<'a>(
    model: &'a Model,
    session_path: &std::path::Path,
) -> Option<&'a crate::collection::BruFile> {
    if model.selected_collection {
        if session_path == std::path::Path::new("collection.bru") {
            return model
                .loaded()?
                .settings
                .as_ref()?
                .as_ref()
                .ok()?
                .ast
                .as_ref();
        }
    } else {
        match model.selected_node() {
            Some(TreeNode::Request(request)) if request.path == session_path => {
                return request.ast.as_ref();
            }
            Some(TreeNode::Folder(folder)) if folder.path.join("folder.bru") == session_path => {
                return folder.meta.as_ref()?.as_ref().ok()?.ast.as_ref();
            }
            _ => {}
        }
    }
    None
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
    let ast =
        current_editing_ast(model, &session.path).ok_or(crate::writer::EditError::Unreadable)?;
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
    let Some(areas) = layout_for(model.size, model.zoomed_panel()) else {
        return;
    };
    let area = inner(areas.detail);
    let (height, width) = (area.height.max(1), area.width);
    let Some(session) = &model.editing else {
        return;
    };
    let Some(field) = session.current_field() else {
        return;
    };
    let (_, field_lines, _) = detail_text_fields_boxes(model);
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
pub(super) fn selection_locked(model: &Model) -> bool {
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
    let Some(ast) = current_editing_ast(model, &session.path) else {
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
            model.last_saved_stamps.insert(path.clone(), saved.stamp);
            model.replace_file_meta_or_request_node(&path, saved.ast, saved.view);
        }
        Err(error) => {
            model.last_status = Some(super::model::StatusMessage::SaveError(error.to_string()));
        }
    }
}

/// Traite une notification de modification de fichiers surveillés sur disque.
fn handle_watch_reload(model: &mut Model, paths: Vec<std::path::PathBuf>) -> Command {
    use super::watch::is_watched_path;

    let relevant_paths: Vec<std::path::PathBuf> =
        paths.into_iter().filter(|p| is_watched_path(p)).collect();
    if relevant_paths.is_empty() {
        return Command::None;
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
            .get(path)
            .is_some_and(|saved_stamp| {
                FileStamp::capture(&root.join(path)).is_ok_and(|current| current == *saved_stamp)
            })
    });

    if all_self_saved {
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
        model.last_status = Some(super::model::StatusMessage::FileModifiedOnDisk);
        Command::None
    } else {
        model.editing = None;
        model.environment_editing = None;
        model.pending_reload = false;
        Command::ReloadCollection
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
    layout_for(model.size, model.zoomed_panel())
        .map_or(1, |areas| usize::from(inner(areas.tree).height).max(1))
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

// --- Recherche -----------------------------------------------------------

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
pub(super) fn reset_filter_if_selection_changed(
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
pub(crate) fn is_tree_filtered(model: &Model) -> bool {
    current_tree_filter_pattern(model).is_some()
}

/// Motif validé rappelé dans le titre du panneau Collection.
pub(crate) fn active_tree_filter_pattern(model: &Model) -> Option<&str> {
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
fn open_tree_filter(model: &mut Model) {
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
fn confirm_tree_filter(model: &mut Model) {
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
fn cancel_tree_filter(model: &mut Model) {
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

    use super::test_support::*;
    use super::*;
    use crate::app::model::{
        ActiveRun, HistoryEntry, HistoryOutcome, PendingConfirm, RequestTab, RunFailure, SavedEdit,
        StatusMessage,
    };
    use crate::app::test_support::{loaded_model, runner_probe_model, select, selected_name};
    use crate::app::view::detail::request_text_and_fields;
    use crate::collection::{LoadError, RequestView};
    use crate::runner;

    /// Le popup `O` reste utilisable sur un terminal de 80×24 : le champ
    /// sous le curseur est toujours visible, du premier au dernier.
    #[test]
    fn runner_options_popup_scrolls_on_80x24() {
        let mut model = loaded_model((80, 24));
        update(&mut model, Message::ToggleRunnerOptions);
        let screen = crate::app::test_support::render(&model, 80, 24);
        assert!(
            screen.iter().any(|l| l.contains("▶ Tags inclus")),
            "{screen:#?}"
        );
        // Jusqu'au bout de la liste des champs : la sélection défile.
        for _ in 0..40 {
            update(&mut model, Message::Down);
        }
        let screen = crate::app::test_support::render(&model, 80, 24);
        assert!(
            screen.iter().any(|l| l.contains("▶")),
            "champ sélectionné hors écran : {screen:#?}"
        );
        assert!(
            !screen.iter().any(|l| l.contains("▶ Tags inclus")),
            "{screen:#?}"
        );
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
            Command::StartRun {
                request, target, ..
            } => {
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
            Command::StartRun {
                request, target, ..
            } => {
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
            is_campaign: false,
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
            Command::StartRun {
                request, target, ..
            } => {
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
            is_campaign: false,
            handle: None,
        });
        let command = update(&mut model, Message::RunSelected);
        assert!(matches!(command, Command::None));
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
            is_campaign: false,
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
                    output: String::new(),
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
            is_campaign: true,
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
                    output: String::new(),
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
    fn run_finished_emits_save_history_when_enabled() {
        let mut model = loaded_model((100, 30));
        model.history_enabled = true;
        model.run.active = Some(ActiveRun {
            id: runner::RunId(7),
            target: "grp".into(),
            recursive: true,
            is_campaign: true,
            handle: None,
        });
        let report = sample_report(&["grp/x.bru"]);
        let command = update(
            &mut model,
            Message::RunFinished(runner::RunEvent {
                id: runner::RunId(7),
                outcome: runner::RunOutcome::Completed {
                    report,
                    exit_code: Some(0),
                    output: String::new(),
                },
            }),
        );
        assert!(matches!(command, Command::SaveHistory { .. }));
    }

    #[test]
    fn run_finished_failed_sets_last_failure_and_clears_active() {
        let mut model = loaded_model((100, 30));
        model.run.active = Some(ActiveRun {
            id: runner::RunId(3),
            target: "simple-get.bru".into(),
            recursive: false,
            is_campaign: false,
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
            is_campaign: false,
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
            options: crate::runner::RunnerOptions::default(),
        });
        model.run.active = Some(ActiveRun {
            id: handle.id(),
            target: std::path::PathBuf::from("ok.bru"),
            recursive: false,
            is_campaign: false,
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
            options: crate::runner::RunnerOptions::default(),
        });
        model.run.active = Some(ActiveRun {
            id: handle.id(),
            target: std::path::PathBuf::from("green.bru"),
            recursive: false,
            is_campaign: false,
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
            options: crate::runner::RunnerOptions::default(),
        });
        let id = handle.id();
        model.run.active = Some(ActiveRun {
            id,
            target: std::path::PathBuf::from("sleeping.bru"),
            recursive: false,
            is_campaign: false,
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
            options: crate::runner::RunnerOptions::default(),
        });
        model.run.active = Some(ActiveRun {
            id: handle.id(),
            target: std::path::PathBuf::from("failed.bru"),
            recursive: false,
            is_campaign: false,
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
                is_campaign: false,
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

    #[test]
    fn script_tab_search_selection_and_yank() {
        let mut model = loaded_model((100, 30));
        select(&mut model, "scripted.bru");
        model.request_tab = RequestTab::Script;
        update(&mut model, Message::NextFocus); // Focus sur Détail

        // 1. Recherche dans l'onglet Script
        update(&mut model, Message::StartSearch);
        for c in "started".chars() {
            update(&mut model, Message::SearchInput(c));
        }
        update(&mut model, Message::ConfirmSearch);
        assert!(model.detail_match.is_some());
        let (line, range) = model.detail_match.clone().expect("match");
        let lines = crate::app::view::detail::plain_lines(&model);
        assert_eq!(&lines[line as usize][range], "started");

        // 2. Sélection visuelle et copie
        update(&mut model, Message::ToggleVisual);
        let Command::CopyToClipboard { text, token } = update(&mut model, Message::Yank) else {
            panic!("copie attendue");
        };
        assert!(text.contains("started"));
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
    fn tests_tab_search_selection_and_yank() {
        let mut model = loaded_model((100, 30));
        select(&mut model, "scripted.bru");
        model.request_tab = RequestTab::Tests;
        update(&mut model, Message::NextFocus); // Focus sur Détail

        // 1. Recherche dans l'onglet Tests
        update(&mut model, Message::StartSearch);
        for c in "getStatus".chars() {
            update(&mut model, Message::SearchInput(c));
        }
        update(&mut model, Message::ConfirmSearch);
        assert!(model.detail_match.is_some());
        let (line, range) = model.detail_match.clone().expect("match");
        let lines = crate::app::view::detail::plain_lines(&model);
        assert_eq!(&lines[line as usize][range], "getStatus");

        // 2. Sélection visuelle et copie
        update(&mut model, Message::ToggleVisual);
        let Command::CopyToClipboard { text, token } = update(&mut model, Message::Yank) else {
            panic!("copie attendue");
        };
        assert!(text.contains("expect(res.getStatus())"));
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

        // 2. Sur un dossier sans folder.bru avec Focus::Detail : sans effet
        select(&mut model, "misc");
        update(&mut model, Message::NextFocus);
        assert_eq!(model.focus, Focus::Detail);
        update(&mut model, Message::StartEdit);
        assert!(model.editing.is_none());

        // Sur un dossier avec folder.bru valide : ouverture
        select(&mut model, "grp");
        update(&mut model, Message::StartEdit);
        assert!(model.editing.is_some());
        assert_eq!(
            model.editing.as_ref().unwrap().path,
            std::path::PathBuf::from("grp/folder.bru")
        );
        model.editing = None;

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
    fn console_tab_supports_search_visual_selection_and_yank() {
        use crate::app::test_support::runner_probe_model;

        let mut model = runner_probe_model();
        select(&mut model, "green.bru");
        if let Some(outcome) = model.run.outcomes.get_mut(Path::new("green.bru")) {
            outcome.console =
                std::sync::Arc::from("première ligne\nlog_console_cible\ntroisième ligne\n");
        }

        update(&mut model, Message::NextFocus); // Détail
        update(&mut model, Message::NextFocus); // Réponse
        model.response_tab = ResponseTab::Console;

        // 1. Recherche '/'
        update(&mut model, Message::StartSearch);
        for c in "log_console_cible".chars() {
            update(&mut model, Message::SearchInput(c));
        }
        update(&mut model, Message::ConfirmSearch);
        assert!(model.response_match.is_some());
        let (matched_line, _) = model.response_match.as_ref().unwrap();
        assert_eq!(*matched_line, 1);

        // 2. Sélection visuelle 'v' et copie 'y'
        update(&mut model, Message::ToggleVisual);
        assert!(model.response_selection.is_some());
        let Command::CopyToClipboard { text, .. } = update(&mut model, Message::Yank) else {
            panic!("copie attendue");
        };
        assert!(text.contains("log_console_cible"));
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

        // Sans effet sur un dossier sans folder.bru ni sur un nœud en erreur.
        let mut model = loaded_model((100, 30));
        select(&mut model, "misc");
        update(&mut model, Message::NextFocus);
        update(&mut model, Message::Enter);
        assert!(model.editing.is_none());
        select(&mut model, "broken.bru");
        update(&mut model, Message::Enter);
        assert!(model.editing.is_none());

        // Ouvre la session sur un dossier doté de folder.bru.
        select(&mut model, "grp");
        update(&mut model, Message::Enter);
        assert!(model.editing.is_some());
        assert_eq!(
            model.editing.as_ref().unwrap().path,
            std::path::PathBuf::from("grp/folder.bru")
        );
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
        let height = layout_for(model.size, model.zoomed_panel())
            .map(|a| inner(a.detail).height)
            .expect("layout");
        assert_eq!(model.detail_scroll, 0);
        move_to_field(&mut model, EditableField::BodyText);
        let Some(TreeNode::Request(request)) = model.selected_node() else {
            panic!("requête attendue");
        };
        let (_, lines, _) =
            request_text_and_fields(request, model.editing.as_ref(), model.shown_request_tab());
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
        let width = layout_for(model.size, model.zoomed_panel())
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
            is_campaign: false,
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
        assert_eq!(model.focus, Focus::Secrets);
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
    fn settings_refuses_structural_edits_and_accepts_value_edits() {
        let mut model = edit_session("simple-get.bru", (100, 30));
        let source = "meta {\n  name: ping\n  type: http\n  seq: 7\n}\n\nget {\n  url: https://{{host}}/ping\n  body: none\n  auth: none\n}\n\nsettings {\n  encodeUrl: true\n}\n";
        let ast = crate::collection::BruFile::parse(source.to_owned()).expect("AST");
        let view = RequestView::from_ast(&ast).expect("vue");
        if let CollectionState::Loaded(col) = &mut model.collection {
            for node in &mut col.tree {
                if let TreeNode::Request(req) = node
                    && req.path == std::path::Path::new("simple-get.bru")
                {
                    req.ast = Some(ast.clone());
                    req.view = view.clone();
                }
            }
        }
        let session_mut = model.editing.as_mut().expect("session");
        session_mut.fields.push(EditableField::SettingsValue(0));
        session_mut
            .preview
            .extra
            .settings
            .push(crate::collection::KeyValue {
                key: "encodeUrl".into(),
                value: "true".into(),
                enabled: true,
            });
        go_to(&mut model, EditableField::SettingsValue(0));

        // Toggle (Espace) est sans effet
        update(&mut model, Message::ToggleField);
        assert!(!session(&model).dirty);

        // Delete ('d') est sans effet
        update(&mut model, Message::Delete);
        assert!(!session(&model).dirty);

        // Rename ('c') est sans effet
        update(&mut model, Message::Rename);
        assert_eq!(session(&model).state, EditState::FieldSelect);

        // Add ('a') est sans effet
        update(&mut model, Message::Add);
        assert_eq!(session(&model).state, EditState::FieldSelect);

        // Mais Value edit (Enter) est autorisé !
        update(&mut model, Message::Enter);
        assert!(matches!(session(&model).state, EditState::Input(_)));
        press(&mut model, InputKey::Backspace, 4);
        type_input(&mut model, "false");
        update(&mut model, Message::ValidateInput);
        assert_eq!(session(&model).state, EditState::FieldSelect);
        assert!(session(&model).dirty);
        assert_eq!(session(&model).preview.extra.settings[0].value, "false");
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
            is_campaign: true,
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
            rhs_expr: "eq 200".into(),
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
                    output: String::new(),
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
            is_campaign: false,
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
                    output: String::new(),
                },
            }),
        );
        assert_eq!(
            model.campaign.as_ref().unwrap().target,
            PathBuf::from("grp")
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
    fn campaign_filter_cycling_and_navigation_across_statuses() {
        let mut model = loaded_model((100, 30));
        model.campaign = Some(CampaignSummary {
            target: PathBuf::from("grp"),
            total: 3,
            passed: 1,
            failed: 1,
            skipped: 1,
            duration_secs: 0.5,
            items: vec![
                CampaignItem {
                    path: PathBuf::from("grp/sub/deep.bru"),
                    name: "deep".into(),
                    status: CampaignItemStatus::Passed,
                    http_code: Some(200),
                    duration_secs: 0.1,
                    reason: None,
                    iteration_index: 0,
                },
                CampaignItem {
                    path: PathBuf::from("grp/x.bru"),
                    name: "x".into(),
                    status: CampaignItemStatus::Failed,
                    http_code: Some(500),
                    duration_secs: 0.2,
                    reason: Some("server error".into()),
                    iteration_index: 0,
                },
                CampaignItem {
                    path: PathBuf::from("grp/skip.bru"),
                    name: "skip".into(),
                    status: CampaignItemStatus::Skipped,
                    http_code: None,
                    duration_secs: 0.0,
                    reason: None,
                    iteration_index: 0,
                },
            ],
            filter: CampaignFilter::Failed,
            failures: vec![CampaignFailure {
                path: PathBuf::from("grp/x.bru"),
                name: "x".into(),
                http_code: Some(500),
                reason: "server error".into(),
            }],
            total_iterations: 1,
        });
        model.focus = Focus::Campaign;
        assert_eq!(model.campaign_selected, 0);

        // 1. Filtre par défaut : Failed (1 seul item : grp/x.bru)
        let campaign = model.campaign.as_ref().unwrap();
        assert_eq!(campaign.filter, CampaignFilter::Failed);
        assert_eq!(campaign.filtered_items().len(), 1);
        assert_eq!(
            campaign.filtered_items()[0].path,
            PathBuf::from("grp/x.bru")
        );

        // 2. Cycle vers Passed via Message::CycleCampaignFilter
        update(&mut model, Message::CycleCampaignFilter);
        let campaign = model.campaign.as_ref().unwrap();
        assert_eq!(campaign.filter, CampaignFilter::Passed);
        assert_eq!(model.campaign_selected, 0);
        assert_eq!(campaign.filtered_items().len(), 1);
        assert_eq!(
            campaign.filtered_items()[0].path,
            PathBuf::from("grp/sub/deep.bru")
        );

        // Sélection croisée depuis le filtre Passed
        update(&mut model, Message::Enter);
        assert_eq!(model.focus, Focus::Detail);
        assert_eq!(
            model.selected_node().unwrap().path(),
            Path::new("grp/sub/deep.bru")
        );

        // Retour au panneau Campagne et cycle vers Skipped via touche 'f' (OpenTreeFilter quand focus Campagne)
        model.focus = Focus::Campaign;
        update(&mut model, Message::OpenTreeFilter);
        let campaign = model.campaign.as_ref().unwrap();
        assert_eq!(campaign.filter, CampaignFilter::Skipped);
        assert_eq!(model.campaign_selected, 0);
        assert_eq!(campaign.filtered_items().len(), 1);
        assert_eq!(
            campaign.filtered_items()[0].path,
            PathBuf::from("grp/skip.bru")
        );

        // Cycle vers All
        update(&mut model, Message::CycleCampaignFilter);
        let campaign = model.campaign.as_ref().unwrap();
        assert_eq!(campaign.filter, CampaignFilter::All);
        assert_eq!(model.campaign_selected, 0);
        assert_eq!(campaign.filtered_items().len(), 3);

        // Navigation dans Tous
        update(&mut model, Message::Down);
        assert_eq!(model.campaign_selected, 1);
        update(&mut model, Message::Down);
        assert_eq!(model.campaign_selected, 2);
        update(&mut model, Message::Down);
        assert_eq!(model.campaign_selected, 2, "borné à la fin de la liste");

        // Cycle revient à Failed, la sélection est réinitialisée à 0
        update(&mut model, Message::CycleCampaignFilter);
        let campaign = model.campaign.as_ref().unwrap();
        assert_eq!(campaign.filter, CampaignFilter::Failed);
        assert_eq!(model.campaign_selected, 0);
    }

    #[test]
    fn campaign_run_failed_requests_starts_run_with_failure_paths() {
        let mut model = runner_probe_model();
        model.runner_options = crate::runner::RunnerOptionsConfig {
            tags: vec!["smoke".into()],
            exclude_tags: Vec::new(),
            delay_ms: Some(25),
            bail: false,
            tests_only: false,
            export_junit: false,
            export_html: false,
            skip_headers: true,
            ..crate::runner::RunnerOptionsConfig::default()
        };
        model.campaign = Some(CampaignSummary {
            target: PathBuf::from("grp"),
            total: 3,
            passed: 1,
            failed: 2,
            skipped: 0,
            duration_secs: 0.5,
            items: vec![
                CampaignItem {
                    path: PathBuf::from("ok.bru"),
                    name: "ok".into(),
                    status: CampaignItemStatus::Passed,
                    http_code: Some(200),
                    duration_secs: 0.1,
                    reason: None,
                    iteration_index: 0,
                },
                CampaignItem {
                    path: PathBuf::from("folder/down.bru"),
                    name: "down".into(),
                    status: CampaignItemStatus::Failed,
                    http_code: None,
                    duration_secs: 0.2,
                    reason: Some("connect error".into()),
                    iteration_index: 0,
                },
                CampaignItem {
                    path: PathBuf::from("json.bru"),
                    name: "json".into(),
                    status: CampaignItemStatus::Failed,
                    http_code: Some(500),
                    duration_secs: 0.2,
                    reason: Some("assertion failed".into()),
                    iteration_index: 0,
                },
            ],
            filter: CampaignFilter::Failed,
            failures: vec![
                CampaignFailure {
                    path: PathBuf::from("folder/down.bru"),
                    name: "down".into(),
                    http_code: None,
                    reason: "connect error".into(),
                },
                CampaignFailure {
                    path: PathBuf::from("json.bru"),
                    name: "json".into(),
                    http_code: Some(500),
                    reason: "assertion failed".into(),
                },
            ],
            total_iterations: 1,
        });

        model.focus = Focus::Campaign;
        let cmd = update(&mut model, Message::RunFailedRequests);

        match cmd {
            Command::StartRun {
                request,
                is_campaign,
                ..
            } => {
                assert!(
                    is_campaign,
                    "doit être marqué comme campagne pour mettre à jour le bilan"
                );
                assert!(!request.recursive, "ne doit pas être récursif");
                assert_eq!(
                    request.targets,
                    vec![PathBuf::from("folder/down.bru"), PathBuf::from("json.bru")]
                );
                assert_eq!(request.options.tags, vec!["smoke"]);
                assert_eq!(request.options.delay_ms, Some(25));

                let args = request.bru_args();
                assert_eq!(args[0], "run");
                assert_eq!(args[1], "folder/down.bru");
                assert_eq!(args[2], "json.bru");
                assert!(!args.iter().any(|a| a == "-r"), "aucun drapeau -r");
                assert!(args.iter().any(|a| a == "--delay"));
            }
            other => panic!("attendu StartRun, reçu {other:?}"),
        }
    }

    #[test]
    fn campaign_run_failed_requests_empty_or_no_failures() {
        let mut model = runner_probe_model();

        // 0. Hors du panneau Campagne : `R` est ignoré.
        let cmd = update(&mut model, Message::RunFailedRequests);
        assert!(matches!(cmd, Command::None));
        assert_eq!(model.last_status, None);
        model.focus = Focus::Campaign;

        // 1. Sans campagne active
        let cmd = update(&mut model, Message::RunFailedRequests);
        assert!(matches!(cmd, Command::None));
        assert!(matches!(
            model.last_status,
            Some(StatusMessage::NoFailedRequests)
        ));

        // 2. Avec campagne mais aucun échec
        model.campaign = Some(CampaignSummary {
            target: PathBuf::from("grp"),
            total: 2,
            passed: 2,
            failed: 0,
            skipped: 0,
            duration_secs: 0.2,
            items: vec![CampaignItem {
                path: PathBuf::from("ok.bru"),
                name: "ok".into(),
                status: CampaignItemStatus::Passed,
                http_code: Some(200),
                duration_secs: 0.1,
                reason: None,
                iteration_index: 0,
            }],
            filter: CampaignFilter::Failed,
            failures: Vec::new(),
            total_iterations: 1,
        });
        model.last_status = None;
        let cmd = update(&mut model, Message::RunFailedRequests);
        assert!(matches!(cmd, Command::None));
        assert!(matches!(
            model.last_status,
            Some(StatusMessage::NoFailedRequests)
        ));
    }

    #[test]
    fn campaign_run_failed_requests_when_already_running() {
        let mut model = runner_probe_model();
        model.campaign = Some(CampaignSummary {
            target: PathBuf::from("grp"),
            total: 1,
            passed: 0,
            failed: 1,
            skipped: 0,
            duration_secs: 0.1,
            items: vec![CampaignItem {
                path: PathBuf::from("down.bru"),
                name: "down".into(),
                status: CampaignItemStatus::Failed,
                http_code: None,
                duration_secs: 0.1,
                reason: Some("error".into()),
                iteration_index: 0,
            }],
            filter: CampaignFilter::Failed,
            failures: vec![CampaignFailure {
                path: PathBuf::from("down.bru"),
                name: "down".into(),
                http_code: None,
                reason: "error".into(),
            }],
            total_iterations: 1,
        });
        model.run.active = Some(ActiveRun {
            id: runner::RunId(1),
            target: PathBuf::from("grp"),
            recursive: true,
            is_campaign: true,
            handle: None,
        });

        let cmd = update(&mut model, Message::RunFailedRequests);
        assert!(matches!(cmd, Command::None));
        assert!(model.run.active.is_some());
    }

    // --- Rechargement automatique de collection (Lot F) -----------------

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

/// Campagnes multi-environnements : une exécution par environnement coché,
/// enchaînées l'une après l'autre (`feat/multi-env`).
#[cfg(test)]
mod series_tests {
    use std::path::PathBuf;

    use super::*;
    use crate::app::model::ActiveRun;
    use crate::app::test_support::{loaded_model, render, select};
    use crate::runner::{Report, RunEvent, RunId, RunOutcome};

    fn report(json: &str) -> Report {
        serde_json::from_str(json).expect("rapport de fixture")
    }

    fn up_report() -> Report {
        report(include_str!(
            "../../../tests/fixtures/reports/multi-env-up.json"
        ))
    }

    fn down_report() -> Report {
        report(include_str!(
            "../../../tests/fixtures/reports/multi-env-down.json"
        ))
    }

    fn args(request: &RunRequest) -> Vec<String> {
        request
            .bru_args()
            .into_iter()
            .map(|arg| arg.into_string().expect("argument UTF-8"))
            .collect()
    }

    /// `parser-cases` : `local` déclare le secret `token`, `staging` aucun.
    fn series_model(environments: &[&str]) -> Model {
        let mut model = loaded_model((100, 30));
        model.runner_options.environments = environments.iter().map(|e| e.to_string()).collect();
        select(&mut model, "grp");
        model
    }

    fn started(command: Command) -> RunRequest {
        match command {
            Command::StartRun { request, .. } => *request,
            other => panic!("StartRun attendu, obtenu {other:?}"),
        }
    }

    /// Simule le démarrage effectif par la boucle (sans poignée).
    fn mark_active(model: &mut Model, id: u64) {
        model.run.active = Some(ActiveRun {
            id: RunId(id),
            target: PathBuf::from("grp"),
            recursive: true,
            is_campaign: true,
            handle: None,
        });
    }

    fn finish(model: &mut Model, id: u64, outcome: RunOutcome) -> Command {
        update(
            model,
            Message::RunFinished(RunEvent {
                id: RunId(id),
                outcome,
            }),
        )
    }

    fn completed(report: Report) -> RunOutcome {
        RunOutcome::Completed {
            report,
            exit_code: Some(1),
            output: "console".into(),
        }
    }

    #[test]
    fn popup_keeps_the_collection_order_of_checked_environments() {
        let mut model = loaded_model((100, 30));
        update(&mut model, Message::ToggleRunnerOptions);
        let popup = model.runner_options_popup.as_mut().expect("popup");
        assert_eq!(popup.available_environments, ["local", "staging"]);
        // Cocher `staging` puis `local` : la campagne suit l'ordre de la collection.
        popup.cursor = RunnerOptionsField::ENVIRONMENTS_AT + 1;
        assert_eq!(popup.current_field(), RunnerOptionsField::Environment(1));
        update(&mut model, Message::ToggleField);
        update(&mut model, Message::Up);
        update(&mut model, Message::Enter);
        let popup = model.runner_options_popup.as_ref().expect("popup");
        assert_eq!(popup.selected_environments, ["staging", "local"]);
        assert_eq!(
            popup.to_config().expect("config").environments,
            ["local", "staging"]
        );
        // Un second appui décoche.
        update(&mut model, Message::ToggleField);
        let popup = model.runner_options_popup.as_ref().expect("popup");
        assert_eq!(popup.to_config().expect("config").environments, ["staging"]);
    }

    #[test]
    fn series_runs_each_environment_in_turn_with_the_same_options() {
        let mut model = series_model(&["local", "staging"]);
        model.secrets.acknowledged.insert("local".into());
        model.runner_options.tags = vec!["smoke".into()];
        model.current_environment = Some("staging".into());

        let first = started(update(&mut model, Message::RunSelected));
        assert_eq!(
            args(&first),
            [
                "run",
                "grp",
                "-r",
                "--env",
                "local",
                "--tags",
                "smoke",
                "--reporter-json",
                "/dev/fd/3"
            ]
        );
        mark_active(&mut model, 1);
        let screen = render(&model, 100, 30);
        assert!(
            screen.iter().any(|l| l.contains("env 1/2 : local")),
            "{screen:#?}"
        );

        // Fin du premier : le second part aussitôt, mêmes options.
        let second = started(finish(&mut model, 1, completed(up_report())));
        assert_eq!(
            args(&second),
            [
                "run",
                "grp",
                "-r",
                "--env",
                "staging",
                "--tags",
                "smoke",
                "--reporter-json",
                "/dev/fd/3"
            ]
        );
        assert!(model.comparison.is_none(), "pas de tableau avant la fin");
        mark_active(&mut model, 2);
        let screen = render(&model, 100, 30);
        assert!(
            screen.iter().any(|l| l.contains("env 2/2 : staging")),
            "{screen:#?}"
        );

        // Fin du dernier : plus d'exécution, tableau comparatif prêt.
        assert!(matches!(
            finish(&mut model, 2, completed(down_report())),
            Command::None
        ));
        assert!(model.series.is_none());
        assert!(model.run.active.is_none());
        let comparison = model.comparison.as_ref().expect("comparaison");
        assert_eq!(comparison.environments, ["local", "staging"]);
        assert_eq!(model.campaign_view, CampaignView::Comparison);
        let Some(StatusMessage::CampaignFinished(text)) = &model.last_status else {
            panic!("statut de fin attendu : {:?}", model.last_status);
        };
        assert!(text.contains("local : 0 échec(s) sur 3"), "{text}");
        assert!(text.contains("staging : 1 échec(s) sur 3"), "{text}");
        // Le panneau Réponse indique l'environnement du résultat conservé.
        let outcome = &model.run.outcomes[&PathBuf::from("data.bru")];
        assert_eq!(outcome.environment.as_deref(), Some("staging"));
    }

    #[test]
    fn a_failed_environment_does_not_stop_the_series() {
        let mut model = series_model(&["local", "staging"]);
        model.secrets.acknowledged.insert("local".into());
        started(update(&mut model, Message::RunSelected));
        mark_active(&mut model, 1);
        let next = finish(
            &mut model,
            1,
            RunOutcome::Failed(crate::runner::RunError::BruNotFound),
        );
        assert_eq!(started(next).env.as_deref(), Some("staging"));
        mark_active(&mut model, 2);
        finish(&mut model, 2, completed(down_report()));
        let comparison = model.comparison.as_ref().expect("comparaison");
        assert!(comparison.errors[0].is_some());
        assert!(comparison.errors[1].is_none());
    }

    #[test]
    fn a_cancelled_run_ends_the_series_without_chaining() {
        let mut model = series_model(&["local", "staging"]);
        model.secrets.acknowledged.insert("local".into());
        started(update(&mut model, Message::RunSelected));
        mark_active(&mut model, 1);
        assert!(matches!(
            finish(&mut model, 1, RunOutcome::Cancelled),
            Command::None
        ));
        assert!(model.series.is_none());
        assert!(model.comparison.is_none());
        let Some(StatusMessage::CampaignFinished(text)) = &model.last_status else {
            panic!("statut de fin attendu : {:?}", model.last_status);
        };
        assert!(text.contains("interrompue après 0/2"), "{text}");
    }

    /// `Ctrl+X` pendant une exécution : même si `bru` livre malgré tout un
    /// rapport (fin simultanée), la série n'enchaîne pas.
    #[tokio::test]
    async fn cancel_request_stops_the_chain_even_if_the_run_completes() {
        let mut model = series_model(&["local", "staging"]);
        model.secrets.acknowledged.insert("local".into());
        let request = started(update(&mut model, Message::RunSelected));
        let (tx, _rx) = tokio::sync::mpsc::channel(4);
        let runner = crate::runner::BruRunner::with_program("bru-introuvable", tx);
        let handle = runner.start(request);
        let id = handle.id();
        update(
            &mut model,
            Message::RunStarted {
                id,
                target: PathBuf::from("grp"),
                recursive: true,
                is_campaign: true,
                handle,
            },
        );
        update(&mut model, Message::CancelRun);
        assert!(model.series.as_ref().is_some_and(|s| s.cancelled));
        let command = update(
            &mut model,
            Message::RunFinished(RunEvent {
                id,
                outcome: completed(up_report()),
            }),
        );
        assert!(matches!(command, Command::None));
        assert!(model.series.is_none());
        // Un seul environnement exécuté : pas de tableau comparatif.
        assert!(model.comparison.is_none());
    }

    #[test]
    fn secrets_are_proposed_per_environment_before_the_series() {
        let mut model = series_model(&["local", "staging"]);
        assert!(matches!(
            update(&mut model, Message::RunSelected),
            Command::None
        ));
        assert_eq!(model.focus, Focus::Secrets);
        assert_eq!(model.secrets.prompt_env.as_deref(), Some("local"));
        assert_eq!(model.secrets.pending_envs, ["local", "staging"]);
        let rows = secret_rows(&model);
        assert!(rows.iter().any(|row| row.name == "token" && row.declared));
        let screen = render(&model, 100, 30);
        assert!(
            screen
                .iter()
                .any(|l| l.contains("Environnement : local (1/2)")),
            "{screen:#?}"
        );

        // Saisie de `token`, puis `r` : `staging` ne manque de rien, la série démarre.
        model
            .secrets
            .typed
            .push(("token".into(), SecretString::new("valeur")));
        let first = started(update(&mut model, Message::RunSelected));
        assert_eq!(first.env.as_deref(), Some("local"));
        assert!(args(&first).contains(&"token=valeur".to_owned()));
        assert_eq!(model.focus, Focus::Tree);
        assert!(model.secrets.prompt_env.is_none());
        assert!(model.secrets.acknowledged.contains("local"));

        // `staging` ne déclare pas `token` : la valeur ne lui est pas transmise.
        mark_active(&mut model, 1);
        let second = started(finish(&mut model, 1, completed(up_report())));
        assert_eq!(second.env.as_deref(), Some("staging"));
        assert!(!args(&second).iter().any(|a| a.starts_with("token=")));
    }

    #[test]
    fn escape_in_the_secrets_panel_abandons_the_whole_series() {
        let mut model = series_model(&["local", "staging"]);
        update(&mut model, Message::RunSelected);
        assert_eq!(model.focus, Focus::Secrets);
        update(&mut model, Message::FocusTree);
        assert!(model.secrets.pending_run.is_none());
        assert!(model.secrets.prompt_env.is_none());
        assert!(model.secrets.pending_envs.is_empty());
        assert!(model.series.is_none());
        assert!(model.secrets.acknowledged.contains("local"));
    }

    #[test]
    fn a_single_checked_environment_replaces_the_current_one() {
        let mut model = series_model(&["staging"]);
        model.current_environment = Some("local".into());
        model.secrets.acknowledged.insert("local".into());
        let request = started(update(&mut model, Message::RunSelected));
        assert_eq!(request.env.as_deref(), Some("staging"));
        mark_active(&mut model, 1);
        assert!(matches!(
            finish(&mut model, 1, completed(up_report())),
            Command::None
        ));
        assert!(model.comparison.is_none());
        assert!(model.series.is_none());
    }

    #[test]
    fn a_single_request_ignores_the_checked_environments() {
        let mut model = series_model(&["local", "staging"]);
        model.current_environment = Some("staging".into());
        select(&mut model, "simple-get.bru");
        let request = started(update(&mut model, Message::RunSelected));
        assert_eq!(request.env.as_deref(), Some("staging"));
        assert!(model.series.is_none());
    }

    /// `multi-env-probe` chargée, après une série sur ses deux
    /// environnements, dans l'ordre de la collection : `down` puis `up`.
    fn compared_model(size: (u16, u16)) -> Model {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/collections/multi-env-probe");
        let mut model = Model::new(root.clone(), size);
        update(
            &mut model,
            Message::CollectionLoaded(crate::collection::CollectionLoader::load(
                &crate::collection::BruLoader,
                &root,
            )),
        );
        model.runner_options.environments = vec!["up".into(), "down".into()];
        model.focus = Focus::Campaign;
        assert_eq!(
            started(update(&mut model, Message::RunSelected))
                .env
                .as_deref(),
            Some("down")
        );
        mark_active(&mut model, 1);
        started(finish(&mut model, 1, completed(down_report())));
        mark_active(&mut model, 2);
        finish(&mut model, 2, completed(up_report()));
        model.focus = Focus::Campaign;
        model
    }

    #[test]
    fn comparison_view_toggles_only_with_a_comparison() {
        let mut model = loaded_model((100, 30));
        model.focus = Focus::Campaign;
        let message = crate::app::message::to_message(
            crate::app::event::AppEvent::Terminal(ratatui::crossterm::event::Event::Key(
                ratatui::crossterm::event::KeyEvent::from(
                    ratatui::crossterm::event::KeyCode::Char('t'),
                ),
            )),
            None,
        )
        .expect("t");
        update(&mut model, message);
        assert_eq!(model.campaign_view, CampaignView::List);

        let mut model = compared_model((100, 30));
        // La série ouvre directement la vue Comparaison.
        assert_eq!(model.campaign_view, CampaignView::Comparison);
        update(&mut model, Message::ToggleCampaignView);
        assert_eq!(model.campaign_view, CampaignView::List);
        // Hors du panneau Campagne, `t` ne fait rien.
        model.focus = Focus::Tree;
        update(&mut model, Message::ToggleCampaignView);
        assert_eq!(model.campaign_view, CampaignView::List);
        model.focus = Focus::Campaign;
        update(&mut model, Message::ToggleCampaignView);
        assert_eq!(model.campaign_view, CampaignView::Comparison);

        // Une campagne hors série efface la comparaison.
        model.runner_options.environments.clear();
        started(update(&mut model, Message::RunSelected));
        mark_active(&mut model, 3);
        finish(&mut model, 3, completed(up_report()));
        assert!(model.comparison.is_none());
        let screen = render(&model, 100, 30);
        assert!(
            !screen.iter().any(|l| l.contains("comparaison")),
            "{screen:#?}"
        );
    }

    #[test]
    fn f_adds_a_divergences_filter_in_the_comparison() {
        let mut model = compared_model((100, 30));
        update(&mut model, Message::Down);
        assert_eq!(model.campaign_selected, 1);
        update(&mut model, Message::OpenTreeFilter);
        let comparison = model.comparison.as_ref().expect("comparaison");
        assert_eq!(
            comparison.filter,
            crate::app::model::ComparisonFilter::Divergent
        );
        assert_eq!(model.campaign_selected, 0);
        assert_eq!(comparison.filtered_rows().len(), 1);
        // Le filtre de la liste n'a pas bougé.
        assert_eq!(
            model.campaign.as_ref().expect("campagne").filter,
            CampaignFilter::Failed
        );
    }

    #[test]
    fn enter_on_a_cell_shows_that_environment_in_the_response_panel() {
        let mut model = compared_model((100, 30));
        // Ligne `data.bru`, colonne `down` : `→` puis `←` restent bornés.
        update(&mut model, Message::Left);
        assert_eq!(model.comparison_column, 0);
        update(&mut model, Message::Right);
        update(&mut model, Message::Right);
        assert_eq!(model.comparison_column, 1);
        update(&mut model, Message::Left);
        assert_eq!(model.comparison_column, 0);

        update(&mut model, Message::Enter);
        assert_eq!(model.focus, Focus::Response);
        assert_eq!(
            model.selected_node().map(|node| node.path().to_path_buf()),
            Some(PathBuf::from("data.bru"))
        );
        let outcome = &model.run.outcomes[&PathBuf::from("data.bru")];
        assert_eq!(outcome.environment.as_deref(), Some("down"));
        assert!(outcome.result.is_failure());
        let screen = render(&model, 100, 30);
        assert!(screen.iter().any(|l| l.contains("[down]")), "{screen:#?}");

        // Retour au tableau, colonne `up` : le résultat bascule.
        model.focus = Focus::Campaign;
        update(&mut model, Message::Right);
        update(&mut model, Message::Enter);
        let outcome = &model.run.outcomes[&PathBuf::from("data.bru")];
        assert_eq!(outcome.environment.as_deref(), Some("up"));
        assert!(!outcome.result.is_failure());
        let screen = render(&model, 100, 30);
        assert!(screen.iter().any(|l| l.contains("[up]")), "{screen:#?}");
        // Lisible aussi en 80 colonnes.
        let screen = render(&model, 80, 24);
        assert!(screen.iter().any(|l| l.contains("[up]")), "{screen:#?}");
    }

    #[test]
    fn enter_on_an_absent_cell_reports_it() {
        let mut model = compared_model((100, 30));
        let comparison = model.comparison.as_mut().expect("comparaison");
        comparison.outcomes[1].remove(std::path::Path::new("data.bru"));
        model.comparison_column = 1;
        update(&mut model, Message::Enter);
        assert_eq!(model.focus, Focus::Campaign);
        assert_eq!(
            model.last_status,
            Some(StatusMessage::NoEnvironmentResult("up".into()))
        );
    }

    #[test]
    fn comparison_table_renders_in_80_columns() {
        let model = compared_model((80, 24));
        let screen = render(&model, 80, 24);
        let text = screen.join("\n");
        assert!(text.contains("Campagne — comparaison"), "{text}");
        assert!(text.contains("1 divergence"), "{text}");
        let header = screen
            .iter()
            .find(|l| l.contains("Requête"))
            .expect("en-tête du tableau");
        assert!(header.contains("up") && header.contains("down"), "{header}");
        let data = screen
            .iter()
            .find(|l| l.contains("data.bru"))
            .expect("ligne data.bru");
        assert!(
            data.contains('≠') && data.contains('✓') && data.contains('✗'),
            "{data}"
        );
        let fixed = screen
            .iter()
            .find(|l| l.contains("static.bru"))
            .expect("ligne static.bru");
        assert!(!fixed.contains('≠'), "{fixed}");
        let skip = screen
            .iter()
            .find(|l| l.contains("skip.bru"))
            .expect("skip");
        assert!(skip.contains('–'), "{skip}");
        assert!(screen.iter().all(|l| l.chars().count() <= 80));
    }

    /// Plus d'environnements que de place : noms tronqués, défilement
    /// horizontal signalé par `‹` / `›`.
    #[test]
    fn many_environments_scroll_horizontally() {
        let mut model = compared_model((80, 24));
        let base = model.comparison.take().expect("comparaison");
        let runs = (0..9)
            .map(|i| EnvRun {
                environment: format!("environnement-{i}"),
                outcome: EnvRunOutcome::Completed {
                    report: if i % 2 == 0 {
                        up_report()
                    } else {
                        down_report()
                    },
                    exit_code: Some(0),
                    console: std::sync::Arc::from(""),
                },
            })
            .collect();
        model.comparison = Some(CampaignComparison::build(base.target, runs));
        let screen = render(&model, 80, 24);
        let header = screen
            .iter()
            .find(|l| l.contains("Requête"))
            .expect("en-tête")
            .clone();
        assert!(header.contains("environ…"), "{header}");
        assert!(header.contains('›') && !header.contains('‹'), "{header}");

        for _ in 0..8 {
            update(&mut model, Message::Right);
        }
        assert_eq!(model.comparison_column, 8);
        let screen = render(&model, 80, 24);
        let header = screen
            .iter()
            .find(|l| l.contains("Requête"))
            .expect("en-tête")
            .clone();
        assert!(header.contains('‹') && !header.contains('›'), "{header}");
        assert!(screen.iter().all(|l| l.chars().count() <= 80));
    }

    #[test]
    fn env_file_is_exclusive_with_checked_environments() {
        let mut model = loaded_model((100, 30));
        update(&mut model, Message::ToggleRunnerOptions);
        let popup = model.runner_options_popup.as_mut().expect("popup");
        popup.selected_environments = vec!["local".into()];
        popup.env_file_input = TextInput::new(
            "tests/fixtures/collections/multi-env-probe/environments/up.bru",
            false,
        );
        assert_eq!(
            popup.to_config(),
            Err(crate::runner::RunnerOptionsError::EnvFileWithEnvironments)
        );
        popup.selected_environments.clear();
        let config = popup.to_config().expect("config");
        assert!(config.env_file.as_ref().is_some_and(|p| p.is_absolute()));
    }

    #[test]
    fn environment_option_fields_are_reachable_and_editable() {
        let mut model = loaded_model((80, 24));
        update(&mut model, Message::ToggleRunnerOptions);
        let popup = model.runner_options_popup.as_mut().expect("popup");
        popup.cursor = RunnerOptionsField::ENVIRONMENTS_AT + popup.available_environments.len() + 1;
        assert_eq!(popup.current_field(), RunnerOptionsField::GlobalEnv);
        update(&mut model, Message::Enter);
        for c in "prod".chars() {
            update(&mut model, Message::InputKey(InputKey::Char(c)));
        }
        update(&mut model, Message::InputKey(InputKey::Enter));
        let screen = render(&model, 80, 24);
        assert!(
            screen
                .iter()
                .any(|l| l.contains("▶ Env. global") && l.contains("[ prod ]")),
            "{screen:#?}"
        );
        update(&mut model, Message::Down);
        assert_eq!(
            model
                .runner_options_popup
                .as_ref()
                .expect("popup")
                .current_field(),
            RunnerOptionsField::WorkspacePath
        );
        update(&mut model, Message::Down);
        assert_eq!(
            model
                .runner_options_popup
                .as_ref()
                .expect("popup")
                .current_field(),
            RunnerOptionsField::SaveButton
        );
        update(&mut model, Message::Enter);
        assert_eq!(model.runner_options.global_env.as_deref(), Some("prod"));
    }

    #[test]
    fn env_file_replaces_the_named_environment_for_campaigns_only() {
        let mut model = series_model(&[]);
        model.current_environment = Some("staging".into());
        model.runner_options.env_file = Some(PathBuf::from("/abs/ci.bru"));
        model.runner_options.global_env = Some("production".into());
        let campaign = args(&started(update(&mut model, Message::RunSelected)));
        assert!(!campaign.contains(&"--env".to_owned()), "{campaign:?}");
        assert!(
            campaign
                .windows(2)
                .any(|w| w == ["--env-file", "/abs/ci.bru"])
        );
        assert!(
            campaign
                .windows(2)
                .any(|w| w == ["--global-env", "production"])
        );

        // Une requête seule garde l'environnement actif, sans options de campagne.
        let mut model = series_model(&[]);
        model.current_environment = Some("staging".into());
        model.runner_options.env_file = Some(PathBuf::from("/abs/ci.bru"));
        select(&mut model, "simple-get.bru");
        let single = args(&started(update(&mut model, Message::RunSelected)));
        assert!(single.windows(2).any(|w| w == ["--env", "staging"]));
        assert!(!single.contains(&"--env-file".to_owned()));
    }

    /// `grpc-ws-probe` chargée, avec deux environnements ajoutés en mémoire
    /// (la fixture n'en déclare pas) et cochés pour la campagne.
    fn grpc_ws_series_model() -> Model {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/collections/grpc-ws-probe");
        let mut model = Model::new(root.clone(), (100, 30));
        update(
            &mut model,
            Message::CollectionLoaded(crate::collection::CollectionLoader::load(
                &crate::collection::BruLoader,
                &root,
            )),
        );
        if let CollectionState::Loaded(collection) = &mut model.collection {
            for name in ["dev", "staging"] {
                collection
                    .environments
                    .push(Ok(crate::collection::Environment {
                        name: name.into(),
                        path: PathBuf::from(format!("environments/{name}.bru")),
                        variables: Vec::new(),
                        secret_names: Vec::new(),
                        extends: None,
                        color: None,
                        ast: None,
                    }));
            }
        }
        model.runner_options.environments = vec!["dev".into(), "staging".into()];
        model
    }

    /// Une requête gRPC ou WebSocket, non lancée seule, ne l'est pas non
    /// plus en série, même rejouée depuis l'historique en mode campagne.
    #[test]
    fn grpc_and_websocket_requests_never_start_a_series() {
        for (path, kind) in [("say-hello.bru", "gRPC"), ("echo.bru", "WebSocket")] {
            let mut model = grpc_ws_series_model();
            model.focus = Focus::History;
            model.history.push_back(crate::app::model::HistoryEntry {
                started_at: std::time::SystemTime::now(),
                target: PathBuf::from(path),
                recursive: true,
                outcome: crate::app::model::HistoryOutcome::Cancelled,
            });
            model.history_selected = 0;
            assert!(matches!(
                update(&mut model, Message::RunSelected),
                Command::None
            ));
            assert!(model.series.is_none(), "{path}");
            assert_eq!(model.last_status, Some(StatusMessage::NotExecutable(kind)));

            // Sélectionnée dans l'arbre : même refus, aucune série.
            let mut model = grpc_ws_series_model();
            select(&mut model, path);
            assert!(matches!(
                update(&mut model, Message::RunSelected),
                Command::None
            ));
            assert!(model.series.is_none(), "{path}");
        }

        // La collection entière reste une campagne normale, en série.
        let mut model = grpc_ws_series_model();
        model.focus = Focus::Campaign;
        let request = started(update(&mut model, Message::RunSelected));
        assert_eq!(request.env.as_deref(), Some("dev"));
        assert!(model.series.is_some());
    }

    #[test]
    fn failed_relaunch_skips_grpc_and_websocket_requests() {
        let mut model = grpc_ws_series_model();
        model.runner_options.environments.clear();
        model.focus = Focus::Campaign;
        started(update(&mut model, Message::RunSelected));
        mark_active(&mut model, 1);
        let report = report(include_str!("../../../tests/fixtures/reports/grpc-ws.json"));
        finish(&mut model, 1, completed(report));
        model.focus = Focus::Campaign;
        // Seuls les échecs gRPC/WS : rien n'est relancé.
        assert!(matches!(
            update(&mut model, Message::RunFailedRequests),
            Command::None
        ));
        assert!(matches!(
            model.last_status,
            Some(StatusMessage::NotExecutable(_))
        ));
    }
}
