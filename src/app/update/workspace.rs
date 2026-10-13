//! Messages propres au mode workspace : chargement, surveillance,
//! secrets et historique de chaque collection.
//!
//! Un résultat qui concerne la collection active suit le chemin ordinaire.
//! Celui d'une collection rangée est appliqué à son emplacement, échangé
//! le temps de l'appliquer, sans toucher à l'affichage ni à la sélection.

use std::path::Path;

use crate::app::message::Message;
use crate::app::model::{CollectionState, Model, SavedEdit, SavedEnvironment, StatusMessage};
use crate::app::workspace::{active_member, member_of_root, swap_active};
use crate::collection::{Collection, Environment, LoadError};
use crate::writer::WriteError;

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
        Message::EditSaved { root, path, result } => match member_of_root(model, &root) {
            Some(member) if member != active => {
                in_member(model, member, |model| {
                    edit_saved_in_slot(model, &path, result)
                });
                Ok(Command::None)
            }
            _ => Err(Box::new(Message::EditSaved { root, path, result })),
        },
        Message::EnvironmentSaved { root, path, result } => match member_of_root(model, &root) {
            Some(member) if member != active => {
                in_member(model, member, |model| {
                    environment_saved_in_slot(model, &path, result)
                });
                Ok(Command::None)
            }
            _ => Err(Box::new(Message::EnvironmentSaved { root, path, result })),
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

/// Enregistrement d'une requête, d'un dossier ou de `collection.bru`
/// d'une collection rangée entre-temps : son empreinte est retenue (le
/// watcher ne la prendra pas pour une modification externe) et son nœud
/// est remplacé par la version relue. La session d'édition, propre à la
/// collection active, n'est pas touchée.
fn edit_saved_in_slot(model: &mut Model, path: &Path, result: Result<SavedEdit, WriteError>) {
    match result {
        Ok(saved) => {
            model
                .last_saved_stamps
                .insert(path.to_path_buf(), saved.stamp);
            model.replace_file_meta_or_request_node(path, saved.ast, saved.view);
        }
        Err(error) => model.last_status = Some(StatusMessage::SaveError(error.to_string())),
    }
}

/// Enregistrement d'un environnement d'une collection rangée : empreinte
/// retenue et environnement reconstruit depuis le fichier relu.
fn environment_saved_in_slot(
    model: &mut Model,
    path: &Path,
    result: Result<SavedEnvironment, WriteError>,
) {
    match result {
        Ok(saved) => {
            model
                .last_saved_stamps
                .insert(path.to_path_buf(), saved.stamp);
            let Some(collection) = model.loaded_mut() else {
                return;
            };
            for env in collection.environments.iter_mut().flatten() {
                if env.path == path {
                    *env = Environment::from_ast(env.name.clone(), env.path.clone(), saved.ast);
                    break;
                }
            }
        }
        Err(error) => model.last_status = Some(StatusMessage::SaveError(error.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

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

    /// Racine canonique d'un membre chargé.
    fn member_root(model: &Model, member: usize) -> PathBuf {
        match member_collection(model, member) {
            Some(CollectionState::Loaded(c)) => c.root.clone(),
            _ => panic!("membre {member} non chargé"),
        }
    }

    fn request_url(model: &Model, member: usize, path: &str) -> String {
        let collection = match member_collection(model, member) {
            Some(CollectionState::Loaded(c)) => c,
            _ => panic!("chargé"),
        };
        match crate::app::tree_filter::find_node_by_path(&collection.tree, Path::new(path)) {
            Some(crate::collection::TreeNode::Request(request)) => request.view.url.clone(),
            other => panic!("requête attendue : {other:?}"),
        }
    }

    fn slot_stamp(model: &Model, member: usize, path: &str) -> Option<crate::writer::FileStamp> {
        model
            .workspace
            .as_ref()
            .and_then(|state| state.slots[member].last_saved_stamps.get(Path::new(path)))
            .copied()
    }

    /// Les deux collections ont un `skip.bru` : l'enregistrement de celle
    /// de `multi-env-probe`, arrivé alors que `runner-probe` est active et
    /// édite son propre `skip.bru`, ne doit viser que `multi-env-probe`.
    #[test]
    fn edit_saved_after_a_switch_goes_to_its_collection() {
        use crate::app::model::{EditScope, EditSession, RequestTab, SavedEdit};
        use crate::collection::{BruFile, RequestView};
        let mut model = workspace_model();
        let root_b = member_root(&model, 1);
        let stamp = crate::writer::FileStamp::capture(&root_b.join("skip.bru")).expect("empreinte");
        let url_a = request_url(&model, 0, "skip.bru");
        let view_a = match model.loaded().and_then(|c| {
            crate::app::tree_filter::find_node_by_path(&c.tree, Path::new("skip.bru"))
        }) {
            Some(crate::collection::TreeNode::Request(r)) => r.view.clone(),
            _ => panic!("skip.bru"),
        };
        model.editing = Some(EditSession::new(
            "skip.bru".into(),
            stamp,
            view_a,
            EditScope::Tab(RequestTab::Params),
        ));

        let text = "meta {\n  name: skip\n  seq: 3\n}\n\nget {\n  url: http://enregistre.invalid/skip\n}\n";
        let ast = BruFile::parse(text.to_owned()).expect("AST");
        let view = RequestView::from_ast(&ast).expect("vue");
        update(
            &mut model,
            Message::EditSaved {
                root: root_b.clone(),
                path: "skip.bru".into(),
                result: Ok(SavedEdit { stamp, ast, view }),
            },
        );

        // Collection rangée : nœud relu et empreinte retenue.
        assert_eq!(
            request_url(&model, 1, "skip.bru"),
            "http://enregistre.invalid/skip"
        );
        assert_eq!(slot_stamp(&model, 1, "skip.bru"), Some(stamp));
        // Collection active : session et nœud intacts.
        assert!(model.editing.is_some(), "session de la collection active");
        assert_eq!(request_url(&model, 0, "skip.bru"), url_a);
        assert!(model.last_saved_stamps.is_empty());

        // Échec : seul le statut le signale.
        update(
            &mut model,
            Message::EditSaved {
                root: root_b,
                path: "skip.bru".into(),
                result: Err(crate::writer::WriteError::Io {
                    path: "skip.bru".into(),
                    source: std::io::Error::other("disque plein"),
                }),
            },
        );
        assert!(matches!(
            model.last_status,
            Some(StatusMessage::SaveError(_))
        ));
        assert!(model.editing.is_some());
    }

    #[test]
    fn edit_saved_of_the_active_collection_still_closes_its_session() {
        use crate::app::model::{EditScope, EditSession, RequestTab, SavedEdit};
        let mut model = workspace_model();
        update(&mut model, Message::Down);
        assert_eq!(crate::app::workspace::active_member(&model), Some(1));
        let root_b = member_root(&model, 1);
        let stamp = crate::writer::FileStamp::capture(&root_b.join("data.bru")).expect("empreinte");
        let (ast, view) = match model.loaded().and_then(|c| {
            crate::app::tree_filter::find_node_by_path(&c.tree, Path::new("data.bru"))
        }) {
            Some(crate::collection::TreeNode::Request(r)) => {
                (r.ast.clone().expect("ast"), r.view.clone())
            }
            _ => panic!("data.bru"),
        };
        model.editing = Some(EditSession::new(
            "data.bru".into(),
            stamp,
            view.clone(),
            EditScope::Tab(RequestTab::Params),
        ));
        // Racine d'une autre collection : routé vers elle, la session
        // active n'est pas touchée.
        let root_a = member_root(&model, 0);
        update(
            &mut model,
            Message::EditSaved {
                root: root_a,
                path: "data.bru".into(),
                result: Ok(SavedEdit {
                    stamp,
                    ast: ast.clone(),
                    view: view.clone(),
                }),
            },
        );
        assert!(model.editing.is_some());
        update(
            &mut model,
            Message::EditSaved {
                root: root_b,
                path: "data.bru".into(),
                result: Ok(SavedEdit { stamp, ast, view }),
            },
        );
        assert!(model.editing.is_none(), "session fermée");
        assert_eq!(
            model.last_saved_stamps.get(Path::new("data.bru")),
            Some(&stamp)
        );
    }

    #[test]
    fn environment_saved_after_a_switch_goes_to_its_collection() {
        use crate::app::model::SavedEnvironment;
        let mut model = workspace_model();
        let root_b = member_root(&model, 1);
        let env_path = PathBuf::from("environments/up.bru");
        let stamp = crate::writer::FileStamp::capture(&root_b.join(&env_path)).expect("empreinte");
        let ast = crate::collection::BruFile::parse_environment(
            "vars {\n  baseUrl: http://enregistre.invalid\n}\n".to_owned(),
        )
        .expect("AST");
        update(
            &mut model,
            Message::EnvironmentSaved {
                root: root_b,
                path: env_path.clone(),
                result: Ok(SavedEnvironment { stamp, ast }),
            },
        );
        assert_eq!(slot_stamp(&model, 1, "environments/up.bru"), Some(stamp));
        assert!(
            model.last_saved_stamps.is_empty(),
            "collection active intacte"
        );
        activate(&mut model, 1);
        let up = model
            .loaded()
            .and_then(|c| c.environments.iter().flatten().find(|e| e.name == "up"))
            .expect("environnement up");
        assert_eq!(up.path, env_path);
        assert_eq!(up.variables[0].value, "http://enregistre.invalid");
        assert!(up.ast.is_some());
    }

    #[test]
    fn unsaved_environment_edit_keeps_the_collection_active() {
        use crate::app::model::EnvironmentEditSession;
        let mut model = workspace_model();
        let stamp = crate::writer::FileStamp::capture(&member_root(&model, 0).join("bruno.json"))
            .expect("empreinte");
        let mut session =
            EnvironmentEditSession::new("x".into(), "environments/x.bru".into(), stamp, Vec::new());
        session.dirty = true;
        model.environment_editing = Some(session);
        update(&mut model, Message::Down);
        assert_eq!(crate::app::workspace::active_member(&model), Some(0));
        assert!(matches!(model.last_status, Some(StatusMessage::EditLocked)));

        // Sans modification, la session est fermée au changement.
        if let Some(session) = model.environment_editing.as_mut() {
            session.dirty = false;
        }
        update(&mut model, Message::Down);
        assert_eq!(crate::app::workspace::active_member(&model), Some(1));
        assert!(model.environment_editing.is_none());
    }
}
