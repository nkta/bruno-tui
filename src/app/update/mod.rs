//! Transitions du modèle.
//!
//! `update` est la seule fonction qui modifie le modèle. Elle ne fait
//! aucune I/O : le chargement et le rendu sont pilotés par la boucle.
use super::global_search::GlobalSearchPopup;
use super::message::{Message, MouseKind};
use super::model::{
    CampaignView, CollectionState, EditSession, EditableField, Exit, Focus, Model,
    RunnerOptionsPopup, SecretInput, StatusMessage, field_value_committed, secret_lookups,
    selected_response_body,
};
use super::view::hit::{Hit, hit_test};
use super::view::{help_max_scroll, help_popup_area};
use crate::runner::RunRequest;
use crate::secrets::SecretLookup;
use crate::writer::FieldEdit;
use ratatui::layout::{Position, Rect};

pub mod edit;
pub mod environment;
pub mod files;
pub mod find;
pub mod navigation;
pub mod output;
pub mod pointer;
pub mod run;
pub mod scroll;
#[cfg(test)]
pub(super) mod test_support;
pub mod tree;
pub mod workspace;

pub(crate) use self::edit::*;
pub(crate) use self::pointer::*;
pub(crate) use self::run::*;
pub(crate) use self::tree::*;
pub use self::tree::{
    active_tree_filter_pattern, is_tree_filtered, refresh_rows, scroll_tree_into_view,
};

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
    /// Recharger la collection d'un membre du workspace ; la boucle renvoie
    /// le résultat via `Message::MemberLoaded`.
    ReloadMember(usize),
    /// Regroupe plusieurs commandes à exécuter séquentiellement par la boucle.
    Batch(Vec<Command>),
    /// Opération de fichier (créer, renommer, cloner, réordonner, corbeille)
    /// dans la collection `root` ; la boucle l'exécute dans `spawn_blocking`
    /// via `writer::files::execute` et renvoie `Message::FileOpDone`.
    FileOp {
        root: std::path::PathBuf,
        op: crate::writer::files::FileOp,
    },
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
    let mut command = match workspace::route(model, message) {
        Ok(command) => command,
        Err(message) => apply(model, *message),
    };
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
    sync_active_member(model);
    command
}

/// Effets qui suivent le chargement d'une collection : résolution des
/// secrets et lecture de l'historique persistant.
pub(crate) fn load_commands(model: &Model, collection: &crate::collection::Collection) -> Command {
    let mut commands = Vec::new();
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
    match commands.len() {
        0 => Command::None,
        1 => commands.pop().unwrap_or(Command::None),
        _ => Command::Batch(commands),
    }
}

/// Filet de sécurité du mode workspace : la ligne sélectionnée appartient
/// toujours à la collection active. Si elle a changé de collection hors
/// des déplacements ordinaires, la collection devient active, sauf pendant
/// une exécution, où la sélection revient sur l'en-tête de la collection.
fn sync_active_member(model: &mut Model) {
    let Some(active) = crate::app::workspace::active_member(model) else {
        return;
    };
    let Some(row) = model.selected_row().cloned() else {
        return;
    };
    if row.member == active {
        return;
    }
    if crate::app::workspace::run_in_progress(model) || tree::member_switch_locked(model) {
        if let Some(index) = model
            .tree
            .rows
            .iter()
            .position(|candidate| candidate.member == active)
        {
            model.tree.selected = index;
            model.selected_collection = true;
        }
        scroll_tree_into_view(model);
        return;
    }
    tree::switch_member(model, row);
}

pub(crate) fn apply(model: &mut Model, message: Message) -> Command {
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
                    super::model::PendingConfirm::DeleteAuthBlock { .. } => {
                        edit::force_save_edit(model)
                    }
                    super::model::PendingConfirm::TrashItem { path, .. } => {
                        files::confirm_trash(model, path)
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

    // Popup de fichier : modal, il reçoit la frappe et la validation.
    if model.file_popup.is_some()
        && !is_background_result(&message)
        && let Some(command) = files::file_popup_message(model, &message)
    {
        return command;
    }

    // Popup d'édition : modal, il garde les touches de navigation et
    // laisse passer la saisie, la sauvegarde et les résultats de fond.
    if model.editing.is_some()
        && !is_background_result(&message)
        && let Some(command) = edit_popup_message(model, &message)
    {
        return command;
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
                Ok(collection) => load_commands(model, collection),
                Err(_) => Command::None,
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
                        if let Some(index) = model.active_row_position(|n| n.path() == prev_path) {
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
                        // Le statut d'une opération de fichier reste affiché.
                        if !matches!(
                            model.last_status,
                            Some(
                                super::model::StatusMessage::FileOpDone(_)
                                    | super::model::StatusMessage::FileOpRefused(_)
                            )
                        ) {
                            model.last_status =
                                Some(super::model::StatusMessage::CollectionReloaded);
                        }
                    } else {
                        model.detail_scroll = 0;
                    }
                    if model.workspace.is_none() {
                        files::apply_pending_selection(model);
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
        Message::Add if model.focus == Focus::Tree => files::open_create(model, false),
        Message::Delete if model.focus == Focus::Tree => files::ask_trash(model),
        Message::CreateFolder if model.focus == Focus::Tree => files::open_create(model, true),
        Message::RenameItem if model.focus == Focus::Tree => files::open_rename(model),
        Message::CloneItem if model.focus == Focus::Tree => files::open_clone(model),
        Message::MoveItemUp if model.focus == Focus::Tree => files::move_item(model, true),
        Message::MoveItemDown if model.focus == Focus::Tree => files::move_item(model, false),
        Message::MoveItemUp => apply(model, Message::Up),
        Message::MoveItemDown => apply(model, Message::Down),
        Message::CreateFolder
        | Message::RenameItem
        | Message::CloneItem
        | Message::FilePopupKey(_)
        | Message::FilePopupNext
        | Message::FilePopupPrevious
        | Message::FilePopupEnter
        | Message::FilePopupCancel => Command::None,
        Message::FileOpDone { root, result } => files::file_op_done(model, &root, result),
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
                    model.response_scroll = model.response_scroll.min(response_max_scroll(model));
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
                open_tree_filter(model);
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
        Message::OpenRequestEdit => {
            if model.focus == Focus::Detail {
                start_request_edit(model);
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
        Message::ReplacePlaintextSecret => {
            edit::replace_plaintext_secret(model);
            Command::None
        }
        Message::KeepPlaintextSecret => {
            edit::keep_plaintext_secret(model);
            Command::None
        }
        Message::CancelPlaintextSecretPrompt => {
            edit::cancel_plaintext_secret_prompt(model);
            Command::None
        }
        Message::ToggleRevealSecret => {
            edit::toggle_reveal_secret(model);
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
        Message::EditSaved { root, path, result } => {
            // Écriture d'une autre collection que celle chargée : ignorée
            // (en workspace, elle est routée vers son emplacement avant).
            if model.loaded().is_some_and(|c| c.root == root) {
                edit_saved(model, path, result);
            }
            Command::None
        }
        Message::EnvironmentSaved { root, path, result } => {
            if model.loaded().is_some_and(|c| c.root == root) {
                environment_saved(model, path, result);
            }
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
                // ←/→ changent d'onglet de requête ou dossier/collection ;
                // le popup d'édition, modal, a ses propres touches.
                Focus::Detail => match navigation {
                    Message::Left => {
                        if model.is_folder_or_collection_selected() {
                            select_request_tab(
                                model,
                                crate::app::model::previous_folder_tab(model.shown_request_tab()),
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
            | Message::FileOpDone { .. }
    )
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::app::model::{RequestTab, ResponseTab};
    use crate::app::test_support::{loaded_model, select};
    use crate::collection::LoadError;
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
            scope: crate::app::model::EditScope::Request,
            fields: vec![EditableField::Url],
            cursor: 0,
            scroll: 0,
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
            unmasked: false,
            original_auth: None,
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
            scope: crate::app::model::EditScope::Request,
            fields: vec![EditableField::Url],
            cursor: 0,
            scroll: 0,
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
            unmasked: false,
            original_auth: None,
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
}
