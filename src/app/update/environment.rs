use std::path::PathBuf;

use super::Command;
use crate::app::message::{InputKey, Message};
use crate::app::model::{
    EnvironmentEditSession, EnvironmentEditState, Focus, Model, SavedEnvironment, StatusMessage,
    environment_name_at,
};
use crate::collection::Collection;
use crate::writer::{FileStamp, WriteError};

/// Touche d'édition de tampon pendant une saisie du popup d'environnement
/// (valeur, clé ou valeur d'un ajout en cours) — `Entrée` est traitée à
/// part, selon l'état courant (`add-environment-entry-management`).
pub(crate) fn environment_input_key(model: &mut Model, key: InputKey) {
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
pub(crate) fn environment_commit_or_advance(model: &mut Model) {
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
pub(crate) fn commit_environment_add_key(model: &mut Model, key: String) {
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

/// Déclenche la sauvegarde des modifications de l'environnement si la session est modifiée.
pub(crate) fn save_environment(model: &mut Model) -> Command {
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
pub(crate) fn environment_saved(
    model: &mut Model,
    path: PathBuf,
    result: Result<SavedEnvironment, WriteError>,
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
            model.last_saved_stamps.insert(path.clone(), saved.stamp);
            Some((saved.ast, session.variables.clone()))
        }
        Err(error) => {
            let Some(session) = &model.environment_editing else {
                return;
            };
            if session.path != path {
                return;
            }
            model.last_status = Some(StatusMessage::SaveError(error.to_string()));
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

/// Indice à sélectionner à l'ouverture du panneau, pour retrouver
/// l'environnement déjà courant (0 si `current` est `None` ou ne
/// correspond à aucune entrée valide).
pub(crate) fn environment_index_for(collection: &Collection, current: Option<&str>) -> usize {
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
pub(crate) fn navigate_environment_picker(model: &mut Model, message: Message) {
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
pub(crate) fn open_environment_editing(model: &mut Model, index: usize) {
    if index == 0 {
        return;
    }
    let Some(collection) = model.loaded() else {
        return;
    };
    let Some(Ok(env)) = collection.environments.get(index - 1) else {
        return;
    };
    if !collection.format.is_editable() {
        model.last_status = Some(StatusMessage::ReadOnlyCollection);
        return;
    }
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
pub(crate) fn select_environment_picker(model: &mut Model, index: usize) {
    let Some(collection) = model.loaded() else {
        return;
    };
    let Some(name_opt) = environment_name_at(collection, index) else {
        return;
    };
    let name = name_opt.map(str::to_owned);
    model.current_environment = name;
}

/// `E` : ouvre le panneau Environnement sur l'environnement actif, ou le
/// referme s'il est ouvert.
pub(crate) fn toggle_environment_picker(model: &mut Model) {
    match model.focus {
        Focus::EnvironmentPicker => model.focus = Focus::Tree,
        _ => {
            if let Some(collection) = model.loaded() {
                let index = environment_index_for(collection, model.current_environment.as_deref());
                model.environment_selected = index;
                model.focus = Focus::EnvironmentPicker;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::super::*;
    use crate::app::message::InputKey;
    use crate::app::model::{
        EnvironmentEditState, Focus, HistoryEntry, HistoryOutcome, StatusMessage,
    };
    use crate::app::test_support::{loaded_model, select};

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
    fn space_toggles_variable_in_environment_popup() {
        let mut model = loaded_model((100, 30));
        model.focus = Focus::EnvironmentPicker;
        model.environment_selected = 1; // "local"
        update(&mut model, Message::StartEdit);

        // Variable 0 ('host') est activée initialement
        assert!(model.environment_editing.as_ref().unwrap().variables[0].enabled);

        // Espace bascule l'état activé -> désactivé
        update(&mut model, Message::ToggleField);
        let session = model.environment_editing.as_ref().unwrap();
        assert!(!session.variables[0].enabled);
        assert!(session.dirty);
        assert_eq!(session.pending.len(), 1);
        assert_eq!(
            session.pending[0],
            crate::writer::EnvironmentVarEdit::Enabled {
                index: 0,
                enabled: false,
            }
        );

        // Second appui sur Espace bascule désactivé -> activé
        update(&mut model, Message::ToggleField);
        let session = model.environment_editing.as_ref().unwrap();
        assert!(session.variables[0].enabled);
        assert_eq!(session.pending.len(), 2);
        assert_eq!(
            session.pending[1],
            crate::writer::EnvironmentVarEdit::Enabled {
                index: 0,
                enabled: true,
            }
        );
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

        let saved_root = model.loaded().expect("chargée").root.clone();
        update(
            &mut model,
            Message::EnvironmentSaved {
                root: saved_root,
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

        let saved_root = model.loaded().expect("chargée").root.clone();
        update(
            &mut model,
            Message::EnvironmentSaved {
                root: saved_root,
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

        let saved_root = model.loaded().expect("chargée").root.clone();
        update(
            &mut model,
            Message::EnvironmentSaved {
                root: saved_root,
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
    fn collection_reload_keeps_current_environment_and_picker_focus() {
        let mut model = loaded_model((100, 30));
        model.current_environment = Some("local".into());
        model.focus = Focus::EnvironmentPicker;

        use crate::collection::CollectionLoader;
        let reloaded = crate::collection::BruLoader.load(&model.source.clone());
        update(&mut model, Message::CollectionLoaded(reloaded));

        assert_eq!(model.current_environment, Some("local".into()));
        assert_eq!(model.focus, Focus::EnvironmentPicker);

        // Si l'environnement n'existe plus dans la collection rechargée, il est réinitialisé
        model.current_environment = Some("supprime".into());
        let reloaded = crate::collection::BruLoader.load(&model.source.clone());
        update(&mut model, Message::CollectionLoaded(reloaded));
        assert_eq!(model.current_environment, None);
        assert_eq!(model.focus, Focus::EnvironmentPicker);
    }
}
