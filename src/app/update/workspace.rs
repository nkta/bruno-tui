//! Messages propres au mode workspace : chargement, surveillance,
//! secrets et historique de chaque collection.
//!
//! Un résultat qui concerne la collection active suit le chemin ordinaire.
//! Celui d'une collection rangée est appliqué à son emplacement, échangé
//! le temps de l'appliquer, sans toucher à l'affichage ni à la sélection.

use crate::app::message::Message;
use crate::app::model::{CollectionState, Model, StatusMessage};
use crate::app::workspace::{active_member, member_of_root, swap_active};
use crate::collection::{Collection, LoadError};

use super::tree::{external_change, handle_watch_reload, row_of_key, selected_key};
use super::{
    Command, apply, load_commands, refresh_rows, reset_secrets_for_collection,
    scroll_tree_into_view,
};

/// Traite les messages du mode workspace ; rend le message intact s'il
/// relève du traitement ordinaire (ou hors workspace). Le message est
/// rendu en boîte : certains portent une collection entière.
pub(crate) fn route(model: &mut Model, message: Message) -> Result<Command, Box<Message>> {
    let Some(active) = active_member(model) else {
        return Err(Box::new(message));
    };
    match message {
        Message::MemberLoaded { member, result } if member == active => {
            Ok(keep_selection(model, |model| {
                apply(model, Message::CollectionLoaded(result))
            }))
        }
        Message::MemberLoaded { member, result } => Ok(keep_selection(model, |model| {
            in_member(model, member, |model| load_inactive(model, result))
        })),
        Message::MemberWatchReload { member, paths } if member == active => {
            Ok(handle_watch_reload(model, paths))
        }
        Message::MemberWatchReload { member, paths } => {
            let changed = in_member(model, member, |model| external_change(model, &paths));
            Ok(if changed {
                Command::ReloadMember(member)
            } else {
                Command::None
            })
        }
        Message::SecretsResolved { root, resolved } => match member_of_root(model, &root) {
            Some(member) if member != active => {
                in_member(model, member, |model| model.secrets.resolved = resolved);
                Ok(Command::None)
            }
            _ => Err(Box::new(Message::SecretsResolved { root, resolved })),
        },
        Message::HistoryLoaded {
            root,
            entries,
            notice,
        } => match member_of_root(model, &root) {
            Some(member) if member != active => {
                in_member(model, member, |model| {
                    apply(
                        model,
                        Message::HistoryLoaded {
                            root,
                            entries,
                            notice,
                        },
                    )
                });
                Ok(Command::None)
            }
            _ => Err(Box::new(Message::HistoryLoaded {
                root,
                entries,
                notice,
            })),
        },
        other => Err(Box::new(other)),
    }
}

/// Applique `f` avec la collection `member` placée dans les champs du
/// `Model`, puis la range de nouveau. `f` ne doit toucher qu'aux champs
/// échangés (voir `swap_slot`) et au statut.
fn in_member<T>(model: &mut Model, member: usize, f: impl FnOnce(&mut Model) -> T) -> T {
    let Some(active) = active_member(model) else {
        return f(model);
    };
    swap_active(model, member);
    let result = f(model);
    swap_active(model, active);
    result
}

/// Recalcule l'arbre après `f` en gardant la ligne sélectionnée.
fn keep_selection(model: &mut Model, f: impl FnOnce(&mut Model) -> Command) -> Command {
    let key = selected_key(model);
    let command = f(model);
    refresh_rows(model);
    if let Some(index) = key.as_ref().and_then(|key| row_of_key(model, key)) {
        model.tree.selected = index;
    }
    scroll_tree_into_view(model);
    command
}

/// Chargement d'une collection rangée : même règles que pour la
/// collection active (environnement conservé s'il existe toujours, échec
/// d'un rechargement signalé sans effacer la collection), sans effet sur
/// l'affichage.
fn load_inactive(model: &mut Model, result: Result<Collection, LoadError>) -> Command {
    let is_reloading = matches!(model.collection, CollectionState::Loaded(_));
    match result {
        Err(error) if is_reloading => {
            model.last_status = Some(StatusMessage::ReloadFailed(error.to_string()));
            Command::None
        }
        Err(error) => {
            model.collection = CollectionState::Failed(error);
            reset_secrets_for_collection(model);
            model.tree.expanded.clear();
            model.current_environment = None;
            Command::None
        }
        Ok(collection) => {
            let command = load_commands(model, &collection);
            if let Some(current) = &model.current_environment
                && !collection
                    .environments
                    .iter()
                    .any(|env| env.as_ref().is_ok_and(|env| &env.name == current))
            {
                model.current_environment = None;
            }
            model.collection = CollectionState::Loaded(collection);
            reset_secrets_for_collection(model);
            if is_reloading {
                model.last_status = Some(StatusMessage::CollectionReloaded);
            }
            command
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::*;
    use crate::app::test_support::workspace_model;
    use crate::app::update::update;
    use crate::app::workspace::{activate, member_collection, member_name};
    use crate::collection::{BruLoader, CollectionLoader};

    fn fixture(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/collections")
            .join(name)
    }

    fn slot_env(model: &Model, member: usize) -> Option<String> {
        model
            .workspace
            .as_ref()
            .and_then(|state| state.slots[member].current_environment.clone())
    }

    #[test]
    fn reload_of_an_inactive_member_targets_its_slot() {
        let mut model = workspace_model();
        update(&mut model, Message::Right);
        update(&mut model, Message::Down);
        let selected = model.tree.selected;
        activate(&mut model, 1);
        model.current_environment = Some("up".into());
        activate(&mut model, 0);
        let active_name = model.loaded().map(|c| c.name.clone());

        let command = update(
            &mut model,
            Message::MemberLoaded {
                member: 1,
                result: BruLoader.load(&fixture("multi-env-probe")),
            },
        );
        assert!(matches!(command, Command::None), "{command:?}");
        assert_eq!(model.loaded().map(|c| c.name.clone()), active_name);
        assert_eq!(model.tree.selected, selected, "sélection inchangée");
        assert_eq!(slot_env(&model, 1).as_deref(), Some("up"));
        assert!(matches!(
            model.last_status,
            Some(StatusMessage::CollectionReloaded)
        ));

        // Échec d'un rechargement : la collection reste affichée.
        update(
            &mut model,
            Message::MemberLoaded {
                member: 1,
                result: Err(LoadError::NotACollectionRoot { path: "/x".into() }),
            },
        );
        assert!(matches!(
            member_collection(&model, 1),
            Some(CollectionState::Loaded(_))
        ));
        assert!(matches!(
            model.last_status,
            Some(StatusMessage::ReloadFailed(_))
        ));
    }

    #[test]
    fn first_load_of_each_member() {
        let a = fixture("runner-probe");
        let b = fixture("multi-env-probe");
        let mut model = Model::new(a.clone(), (100, 30));
        crate::app::workspace::install(
            &mut model,
            crate::collection::workspace::from_paths(&[a.clone(), b.clone(), "/absent".into()]),
        );
        model.history_enabled = true;
        let command = update(
            &mut model,
            Message::MemberLoaded {
                member: 1,
                result: BruLoader.load(&b),
            },
        );
        assert!(
            matches!(command, Command::LoadHistory { ref root } if root.ends_with("multi-env-probe")),
            "{command:?}"
        );
        assert!(matches!(model.collection, CollectionState::Loading));
        update(
            &mut model,
            Message::MemberLoaded {
                member: 2,
                result: Err(LoadError::NotACollectionRoot {
                    path: "/absent".into(),
                }),
            },
        );
        assert!(matches!(
            member_collection(&model, 2),
            Some(CollectionState::Failed(_))
        ));
        update(
            &mut model,
            Message::MemberLoaded {
                member: 0,
                result: BruLoader.load(&a),
            },
        );
        assert_eq!(model.tree.rows.len(), 3);
        assert_eq!(model.tree.selected, 0);
        assert_eq!(member_name(&model, 0), "runner-probe");
    }

    #[test]
    fn watch_reload_of_an_inactive_member_reloads_only_it() {
        let mut model = workspace_model();
        let command = update(
            &mut model,
            Message::MemberWatchReload {
                member: 1,
                paths: vec!["data.bru".into()],
            },
        );
        assert!(matches!(command, Command::ReloadMember(1)), "{command:?}");
        let command = update(
            &mut model,
            Message::MemberWatchReload {
                member: 1,
                paths: vec!["README.md".into()],
            },
        );
        assert!(matches!(command, Command::None));
        let command = update(
            &mut model,
            Message::MemberWatchReload {
                member: 0,
                paths: vec!["ok.bru".into()],
            },
        );
        assert!(matches!(command, Command::ReloadCollection), "{command:?}");
    }

    #[test]
    fn secrets_and_history_reach_the_right_member() {
        use crate::app::model::{HistoryEntry, HistoryOutcome};
        let mut model = workspace_model();
        let root_b = match member_collection(&model, 1) {
            Some(CollectionState::Loaded(c)) => c.root.clone(),
            _ => panic!("chargée"),
        };
        let entry = HistoryEntry {
            started_at: std::time::SystemTime::UNIX_EPOCH,
            target: "data.bru".into(),
            recursive: false,
            outcome: HistoryOutcome::Cancelled,
        };
        update(
            &mut model,
            Message::HistoryLoaded {
                root: root_b.clone(),
                entries: vec![entry],
                notice: None,
            },
        );
        assert!(model.history.is_empty(), "collection active intacte");
        activate(&mut model, 1);
        assert_eq!(model.history.len(), 1);
    }
}
