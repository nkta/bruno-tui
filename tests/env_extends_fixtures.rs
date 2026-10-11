//! Environnements Bruno 4 (`extends`, `color`) chargés depuis
//! `tests/fixtures/collections/env-extends/`.

use std::path::PathBuf;

use bruno_tui::collection::{BruLoader, CollectionLoader, Environment, EnvironmentExtends};

fn environment(name: &str) -> Environment {
    let root =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/collections/env-extends");
    let collection = BruLoader.load(&root).expect("collection chargée");
    collection
        .environments
        .iter()
        .find_map(|env| match env {
            Ok(env) if env.name == name => Some(env.clone()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("environnement {name} absent ou invalide"))
}

#[test]
fn single_extends_and_color_are_loaded() {
    let env = environment("staging");
    assert_eq!(env.extends, Some(EnvironmentExtends::Single("base".into())));
    assert_eq!(env.color.as_deref(), Some("#ffaa00"));
    // Variables du fichier seul : aucun héritage résolu.
    let keys: Vec<_> = env.variables.iter().map(|kv| kv.key.as_str()).collect();
    assert_eq!(keys, ["app_env", "debug"]);
}

#[test]
fn list_extends_with_quoted_name_is_loaded() {
    let env = environment("quoted");
    assert_eq!(
        env.extends,
        Some(EnvironmentExtends::List(vec![
            "base".into(),
            "parent, with comma".into()
        ]))
    );
}

#[test]
fn invalid_color_is_kept_raw_without_error() {
    let env = environment("invalid-color");
    assert_eq!(env.color.as_deref(), Some("not-a-color"));
    assert_eq!(env.extends, None);
}
