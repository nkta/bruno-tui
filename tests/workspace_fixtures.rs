//! Lecture de `tests/fixtures/workspaces/demo/workspace.yml` (format de
//! Bruno 4.2.1) et chargement de ses collections.

use std::path::{Path, PathBuf};

use bruno_tui::collection::workspace::{self, Member};
use bruno_tui::collection::{AutoLoader, LoadError};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

/// Copie temporaire du workspace de démonstration, chemin absolu réécrit
/// vers le vrai dossier des fixtures.
fn demo_copy(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("bruno-tui-ws-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("dossier temporaire");
    let text = std::fs::read_to_string(fixtures().join("workspaces/demo/workspace.yml"))
        .expect("fixture lisible");
    let text = text
        .replace("/BRUNO_TUI_FIXTURES", fixtures().to_str().expect("UTF-8"))
        .replace(
            "../../collections",
            fixtures().join("collections").to_str().expect("UTF-8"),
        );
    std::fs::write(dir.join("workspace.yml"), text).expect("copie");
    dir
}

#[test]
fn demo_fixture_is_read_as_is() {
    let dir = fixtures().join("workspaces/demo");
    assert!(workspace::is_workspace_dir(&dir));
    assert!(!workspace::is_workspace_dir(
        &fixtures().join("collections/runner-probe")
    ));
    let ws = workspace::read_workspace(&dir).expect("workspace.yml valide");
    assert_eq!(ws.name.as_deref(), Some("Demo"));
    assert_eq!(ws.path.as_deref(), Some(dir.as_path()));
    let names: Vec<_> = ws.members.iter().map(|m| m.name.clone()).collect();
    assert_eq!(
        names,
        [
            Some("Runner probe".to_owned()),
            Some("Multi-env".to_owned()),
            Some("Secrets".to_owned()),
            Some("Absente".to_owned()),
        ]
    );
    assert_eq!(
        ws.members[0].root,
        dir.join("../../collections/runner-probe")
    );
    assert_eq!(
        ws.members[2].root,
        PathBuf::from("/BRUNO_TUI_FIXTURES/collections/secret-probe")
    );

    // Tel quel : les deux chemins relatifs se chargent, l'absolu fictif et
    // le chemin absent sont en erreur.
    let loaded: Vec<bool> = ws
        .members
        .iter()
        .map(|m| workspace::load_member(&AutoLoader, &m.root).is_ok())
        .collect();
    assert_eq!(loaded, [true, true, false, false]);
}

#[test]
fn absolute_path_is_loaded_from_a_rewritten_copy() {
    let dir = demo_copy("abs");
    let ws = workspace::read_workspace(&dir).expect("copie valide");
    let Member { root, .. } = &ws.members[2];
    assert!(root.is_absolute());
    let collection = workspace::load_member(&AutoLoader, root).expect("secret-probe");
    assert_eq!(
        collection.root,
        std::fs::canonicalize(fixtures().join("collections/secret-probe")).expect("canonique")
    );
    let absent =
        workspace::load_member(&AutoLoader, &ws.members[3].root).expect_err("chemin invalide");
    assert!(matches!(absent, LoadError::NotACollectionRoot { .. }));
    std::fs::remove_dir_all(&dir).expect("nettoyage");
}

#[test]
fn a_folder_inside_a_collection_is_not_a_member() {
    let inner = fixtures().join("collections/runner-probe/folder");
    let error = workspace::load_member(&AutoLoader, &inner).expect_err("pas une racine");
    assert!(error.to_string().contains("n'est pas une collection"));
}
