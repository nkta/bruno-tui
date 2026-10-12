//! Utilitaires partagés pour les tests de `app::update`.

#![allow(dead_code)]

use crate::app::message::{InputKey, MaskedChar, Message};
use crate::app::model::{EditSession, EditState, EditableField, InputTarget, Model, field_value};
use crate::app::test_support::{loaded_model, select};
use crate::app::text_input::TextInput;
use crate::app::update::{Command, update};
use crate::app::view::detail::{plain_lines, response_plain_lines};
use crate::runner;
use crate::runner::SecretString;
use crate::secrets::{Resolved, SecretMapping, SecretSource};
use crate::writer::EntrySection;

pub(crate) fn sample_request_result(filename: &str) -> runner::report::RequestResult {
    use runner::report::{RequestFile, RequestInfo, ResponseInfo, ResponseStatus, ResultStatus};
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
            data: None,
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

pub(crate) fn sample_report(filenames: &[&str]) -> runner::Report {
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

pub(crate) fn plain_line_index_containing(model: &Model, needle: &str) -> u16 {
    plain_lines(model)
        .iter()
        .position(|line| line.contains(needle))
        .expect("ligne attendue") as u16
}

pub(crate) fn response_line_index_containing(model: &Model, needle: &str) -> u16 {
    response_plain_lines(model)
        .iter()
        .position(|line| line.contains(needle))
        .expect("ligne attendue") as u16
}

/// Modèle avec une session ouverte sur `path`, focus Détail.
pub(crate) fn edit_session(path: &str, size: (u16, u16)) -> Model {
    let mut model = loaded_model(size);
    select(&mut model, path);
    update(&mut model, Message::NextFocus);
    update(&mut model, Message::Enter);
    assert!(model.editing.is_some(), "session ouverte sur {path}");
    model
}

pub(crate) fn move_to_field(model: &mut Model, field: EditableField) {
    let index = model
        .editing
        .as_ref()
        .and_then(|s| s.fields.iter().position(|f| *f == field))
        .expect("champ présent");
    while model.editing.as_ref().map(|s| s.cursor) != Some(index) {
        update(model, Message::Down);
    }
}

pub(crate) fn type_input(model: &mut Model, text: &str) {
    for c in text.chars() {
        update(model, Message::InputKey(InputKey::Char(c)));
    }
}

pub(crate) fn press(model: &mut Model, key: InputKey, times: usize) {
    for _ in 0..times {
        update(model, Message::InputKey(key));
    }
}

pub(crate) fn input_of(model: &Model) -> &TextInput {
    match &model.editing.as_ref().expect("session").state {
        EditState::Input(input) => input,
        EditState::FieldSelect | EditState::MethodPicker { .. } => panic!("saisie attendue"),
    }
}

pub(crate) fn shown_value(model: &Model, field: EditableField) -> String {
    field_value(model.editing.as_ref().expect("session"), &field).to_owned()
}

pub(crate) fn session(model: &Model) -> &EditSession {
    model.editing.as_ref().expect("session")
}

pub(crate) fn mapping(name: &str, key: Option<&str>) -> SecretMapping {
    SecretMapping {
        name: name.to_owned(),
        key: key.map(str::to_owned),
    }
}

pub(crate) fn resolved_dotenv(name: &str, key: &str, value: &str) -> Resolved {
    Resolved {
        name: name.to_owned(),
        source: SecretSource::DotEnv {
            key: key.to_owned(),
        },
        value: Some(SecretString::new(value)),
    }
}

pub(crate) fn type_text(model: &mut Model, text: &str) {
    for c in text.chars() {
        update(model, Message::SecretInput(MaskedChar(c)));
    }
}

pub(crate) fn env_vars_of(command: Command) -> Vec<(String, String)> {
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
pub(crate) fn local_model() -> Model {
    let mut model = loaded_model((100, 30));
    model.current_environment = Some("local".into());
    model
}

/// Déplace le curseur de champ sur `field`, vers le haut ou le bas.
pub(crate) fn go_to(model: &mut Model, field: EditableField) {
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

pub(crate) fn target_of(model: &Model) -> &InputTarget {
    &session(model).target
}

/// Ajoute `key: value` à `section` depuis sa ligne d'ajout.
pub(crate) fn add_via_row(model: &mut Model, section: EntrySection, key: &str, value: &str) {
    go_to(model, EditableField::AddRow(section));
    update(model, Message::Enter);
    type_input(model, key);
    update(model, Message::ValidateInput);
    type_input(model, value);
    update(model, Message::InputKey(InputKey::Enter));
    assert_eq!(session(model).state, EditState::FieldSelect, "ajout validé");
}
