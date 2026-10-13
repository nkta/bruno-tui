//! Plusieurs collections ouvertes ensemble (`docs/workspace.md`).
//!
//! Le `Model` garde dans ses champs l'état de la collection **active**,
//! celle de la ligne sélectionnée. Les autres collections attendent dans
//! des emplacements ([`CollectionSlot`]) échangés par `std::mem::swap`
//! quand la sélection change de collection : le reste de l'application ne
//! voit jamais qu'une collection.

use std::path::PathBuf;

use crate::collection::workspace::{self, Workspace, WorkspaceError};

/// Ce que l'interface ouvre au lancement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Launch {
    /// Une collection, ou un fichier ou dossier qu'elle contient.
    Single(PathBuf),
    /// Plusieurs collections : `workspace.yml` ou plusieurs chemins.
    Workspace(Workspace),
}

impl From<PathBuf> for Launch {
    fn from(path: PathBuf) -> Self {
        Self::Single(path)
    }
}

/// Interprète les chemins de la ligne de commande : un dossier contenant
/// `workspace.yml` ouvre le workspace, plusieurs chemins forment un
/// workspace sans nom, sinon une collection comme avant.
pub fn resolve_launch(paths: &[PathBuf]) -> Result<Launch, WorkspaceError> {
    match paths {
        [] => Ok(Launch::Single(PathBuf::from("."))),
        [path] if workspace::is_workspace_dir(path) => {
            workspace::read_workspace(path).map(Launch::Workspace)
        }
        [path] => Ok(Launch::Single(path.clone())),
        several => Ok(Launch::Workspace(workspace::from_paths(several))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn fixtures() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
    }

    #[test]
    fn one_collection_is_unchanged() {
        let path = fixtures().join("collections/runner-probe");
        assert_eq!(
            resolve_launch(std::slice::from_ref(&path)).expect("valide"),
            Launch::Single(path)
        );
        assert_eq!(
            resolve_launch(&[]).expect("valide"),
            Launch::Single(".".into())
        );
    }

    #[test]
    fn workspace_directory_and_several_paths() {
        let dir = fixtures().join("workspaces/demo");
        let Launch::Workspace(ws) = resolve_launch(&[dir]).expect("valide") else {
            panic!("workspace attendu");
        };
        assert_eq!(ws.name.as_deref(), Some("Demo"));
        assert_eq!(ws.members.len(), 4);

        let a = fixtures().join("collections/runner-probe");
        let b = fixtures().join("collections/multi-env-probe");
        let Launch::Workspace(ws) = resolve_launch(&[a.clone(), b]).expect("valide") else {
            panic!("workspace attendu");
        };
        assert_eq!(ws.name, None);
        assert_eq!(ws.path, None);
        assert_eq!(ws.members[0].root, a);
    }

    #[test]
    fn invalid_workspace_file_is_an_error() {
        let dir = std::env::temp_dir().join(format!("bruno-tui-ws-bad-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dossier");
        std::fs::write(dir.join("workspace.yml"), "collections: [\n  : x").expect("écriture");
        let error = resolve_launch(std::slice::from_ref(&dir)).expect_err("YAML invalide");
        assert!(matches!(error, WorkspaceError::InvalidYaml { .. }));
        std::fs::remove_dir_all(&dir).expect("nettoyage");
    }
}
