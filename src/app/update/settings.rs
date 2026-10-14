//! Popup « Réglages » de la collection (`docs/reglages-collection.md`) :
//! ouverture avec `B`, touches, écriture de `bruno.json` au `Ctrl+S`.
//!
//! La lecture de `bruno.json` à l'ouverture et l'écriture (par la boucle,
//! `Command::SaveSettings`) ne touchent qu'à `<racine>/bruno.json`.
//! Aucun message de statut ne cite une valeur de secret.

use std::path::PathBuf;

use super::Command;
use crate::app::message::Message;
use crate::app::model::{Model, SavedSettings, StatusMessage};
use crate::app::settings::{Action, SettingsSession};
use crate::app::workspace::run_in_progress;
use crate::writer::{WriteError, bruno_config};

fn refuse(model: &mut Model, reason: impl Into<String>) {
    model.last_status = Some(StatusMessage::FileOpRefused(reason.into()));
}

/// `B` dans l'arbre : ouvre le popup « Réglages » de la collection active.
pub(crate) fn open_settings(model: &mut Model) -> Command {
    use crate::app::model::Focus;
    if model.focus != Focus::Tree
        || model.settings.is_some()
        || model.editing.is_some()
        || model.environment_editing.is_some()
        || model.file_popup.is_some()
    {
        return Command::None;
    }
    let Some(collection) = model.loaded() else {
        return Command::None;
    };
    if !collection.format.is_editable() {
        model.last_status = Some(StatusMessage::ReadOnlyCollection);
        return Command::None;
    }
    let root = collection.root.clone();
    let name = collection.name.clone();
    let environments: Vec<String> = collection
        .environments
        .iter()
        .flatten()
        .map(|env| env.name.clone())
        .collect();
    if run_in_progress(model) {
        refuse(model, "Exécution en cours : réglages refusés");
        return Command::None;
    }
    let (text, stamp) = match bruno_config::read_for_edit(&root) {
        Ok(read) => read,
        Err(error) => {
            refuse(model, format!("Réglages impossibles : {error}"));
            return Command::None;
        }
    };
    match SettingsSession::open(root, name, stamp, &text, environments) {
        Ok(session) => {
            if session.legacy_proxy {
                model.last_status = Some(StatusMessage::FileOpDone(
                    "Proxy à l'ancien format : affiché converti, écrit seulement si tu le modifies"
                        .to_owned(),
                ));
            }
            model.settings = Some(session);
        }
        Err(error) => refuse(
            model,
            format!("Réglages impossibles : bruno.json : {error}"),
        ),
    }
    Command::None
}

/// Messages du popup ouvert ; `None` laisse passer le message au
/// traitement ordinaire (redimensionnement, sortie forcée…).
pub(crate) fn settings_message(model: &mut Model, message: &Message) -> Option<Command> {
    let session = model.settings.as_mut()?;
    let action = match message {
        Message::Resize { .. } | Message::ForceQuit | Message::TerminalClosed(_) => return None,
        _ if session.saving => Action::None,
        Message::SettingsKey(key) => session.key(*key),
        Message::SaveEdit => session.save_key(),
        Message::ToggleRevealSecret => {
            session.toggle_reveal();
            Action::None
        }
        _ => Action::None,
    };
    Some(match action {
        Action::None => Command::None,
        Action::Close => {
            model.settings = None;
            Command::None
        }
        Action::Save => save(model),
    })
}

fn save(model: &mut Model) -> Command {
    let Some(session) = model.settings.as_mut() else {
        return Command::None;
    };
    if !session.has_unsaved() {
        model.settings = None;
        return Command::None;
    }
    session.saving = true;
    Command::SaveSettings {
        root: session.root.clone(),
        stamp: session.stamp,
        changes: session.changes(),
        added_paths: session.added_paths.clone(),
    }
}

/// Exécute `Command::SaveSettings` : écrit les clés modifiées de
/// `<root>/bruno.json`, puis relève les chemins de certificats introuvables
/// (simple avertissement : `bru` l'affichera à l'exécution, le contenu du
/// fichier n'est jamais lu).
pub fn execute_save(
    root: &std::path::Path,
    stamp: &crate::writer::FileStamp,
    changes: &[(&'static str, bruno_config::Json)],
    added_paths: &[String],
) -> Result<SavedSettings, WriteError> {
    let stamp = bruno_config::write_bruno_config(root, stamp, changes)?;
    let warnings = added_paths
        .iter()
        .filter(|path| !root.join(path).exists())
        .map(|path| format!("fichier introuvable : {path}"))
        .collect();
    Ok(SavedSettings { stamp, warnings })
}

/// Applique le résultat d'une écriture de `bruno.json`.
pub(crate) fn settings_saved(
    model: &mut Model,
    root: PathBuf,
    result: Result<SavedSettings, WriteError>,
) -> Command {
    let Some(session) = model.settings.as_mut() else {
        return Command::None;
    };
    if session.root != root {
        return Command::None;
    }
    match result {
        Ok(saved) => {
            let mut text = format!(
                "Réglages enregistrés ({})",
                session.changed_labels().join(", ")
            );
            if session.legacy_proxy && session.proxy_dirty() {
                text.push_str(" — Proxy converti au nouveau format de Bruno");
            }
            for warning in &saved.warnings {
                text.push_str(" — Avertissement : ");
                text.push_str(warning);
            }
            model.settings = None;
            model
                .last_saved_stamps
                .insert(PathBuf::from(bruno_config::CONFIG_FILE), saved.stamp);
            model.last_status = Some(StatusMessage::FileOpDone(text));
            Command::ReloadCollection
        }
        Err(error) => {
            session.saving = false;
            let reason = match error {
                WriteError::Stale { .. } => "bruno.json a changé sur le disque".to_owned(),
                other => other.to_string(),
            };
            refuse(model, reason);
            Command::None
        }
    }
}
