//! Lecture seule d'un workspace Bruno : plusieurs collections ouvertes
//! ensemble (`docs/workspace.md`).
//!
//! Un workspace est un dossier qui contient `workspace.yml`. Seuls
//! `info.name` et `collections[].name` / `path` sont lus ; les autres clés
//! (`opencollection`, `info.type`, `remote`, `specs`, `docs`...) sont
//! ignorées sans erreur. Le fichier n'est jamais écrit.
//!
//! Un workspace peut aussi être formé sans fichier par plusieurs chemins
//! passés en ligne de commande (`bruno-tui A B C`) : il n'a alors ni nom
//! ni dossier.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use thiserror::Error;

use super::config::CONFIG_FILE;
use super::error::LoadError;
use super::loader::CollectionLoader;
use super::opencollection::COLLECTION_FILE as OPENCOLLECTION_FILE;
use super::tree::Collection;

/// Nom du fichier qui marque un workspace.
pub const WORKSPACE_FILE: &str = "workspace.yml";

/// Workspace : liste ordonnée de collections.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Workspace {
    /// `info.name` ; `None` pour un workspace sans fichier ou sans nom.
    pub name: Option<String>,
    /// Dossier absolu contenant `workspace.yml` ; `None` sans fichier.
    pub path: Option<PathBuf>,
    pub members: Vec<Member>,
}

/// Collection déclarée dans un workspace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Member {
    /// `collections[].name`, s'il est renseigné.
    pub name: Option<String>,
    /// Racine attendue de la collection : `path` résolu par rapport au
    /// dossier du workspace s'il est relatif. Elle n'est pas vérifiée ici :
    /// voir [`load_member`].
    pub root: PathBuf,
}

/// Erreur de lecture de `workspace.yml`.
#[derive(Debug, Error)]
pub enum WorkspaceError {
    #[error("lecture impossible de `{path}` : {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    /// Seule la position est conservée : le message YAML brut peut citer
    /// des valeurs du fichier. Ligne et colonne valent 0 si inconnues.
    #[error("workspace.yml invalide dans `{path}` (ligne {line}, colonne {column})")]
    InvalidYaml {
        path: PathBuf,
        line: usize,
        column: usize,
    },
}

/// Partie lue de `workspace.yml`.
#[derive(Debug, Default, Deserialize)]
struct RawWorkspace {
    #[serde(default)]
    info: Option<RawInfo>,
    #[serde(default)]
    collections: Option<Vec<RawMember>>,
}

#[derive(Debug, Default, Deserialize)]
struct RawInfo {
    #[serde(default)]
    name: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct RawMember {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    path: Option<String>,
}

/// Vrai si `dir` est un dossier de workspace (contient `workspace.yml`).
pub fn is_workspace_dir(dir: &Path) -> bool {
    dir.join(WORKSPACE_FILE).is_file()
}

/// Lit `workspace.yml` dans le dossier `dir`.
pub fn read_workspace(dir: &Path) -> Result<Workspace, WorkspaceError> {
    let absolute_dir = absolute(dir);
    let file = absolute_dir.join(WORKSPACE_FILE);
    let text = fs::read_to_string(&file).map_err(|source| WorkspaceError::Io {
        path: file.clone(),
        source,
    })?;
    parse_workspace(&text, &absolute_dir).map_err(|(line, column)| WorkspaceError::InvalidYaml {
        path: file,
        line,
        column,
    })
}

/// Analyse le texte de `workspace.yml` ; `dir` est le dossier absolu du
/// workspace. En erreur, seule la position (ligne, colonne) est rendue.
fn parse_workspace(text: &str, dir: &Path) -> Result<Workspace, (usize, usize)> {
    let raw: Option<RawWorkspace> = serde_norway::from_str(text).map_err(|error| {
        error
            .location()
            .map_or((0, 0), |location| (location.line(), location.column()))
    })?;
    let raw = raw.unwrap_or_default();
    let name = raw
        .info
        .and_then(|info| info.name)
        .filter(|name| !name.trim().is_empty());
    let members = raw
        .collections
        .unwrap_or_default()
        .into_iter()
        .map(|member| Member {
            name: member.name.filter(|name| !name.trim().is_empty()),
            // Chemin manquant : le dossier du workspace, qui n'est pas une
            // collection, donne une ligne d'erreur comme un chemin invalide.
            root: dir.join(member.path.unwrap_or_default()),
        })
        .collect();
    Ok(Workspace {
        name,
        path: Some(dir.to_path_buf()),
        members,
    })
}

/// Workspace sans fichier formé par plusieurs chemins de la ligne de
/// commande ; chaque chemin doit être la racine d'une collection.
pub fn from_paths(paths: &[PathBuf]) -> Workspace {
    Workspace {
        name: None,
        path: None,
        members: paths
            .iter()
            .map(|path| Member {
                name: None,
                root: absolute(path),
            })
            .collect(),
    }
}

/// Charge la collection d'un membre. Le chemin doit désigner la racine
/// d'une collection : le loader ne remonte pas vers un dossier parent.
pub fn load_member(loader: &dyn CollectionLoader, root: &Path) -> Result<Collection, LoadError> {
    if !root.join(CONFIG_FILE).is_file() && !root.join(OPENCOLLECTION_FILE).is_file() {
        return Err(LoadError::NotACollectionRoot {
            path: root.to_path_buf(),
        });
    }
    loader.load(root)
}

/// Rend un chemin absolu sans résoudre les liens ; inchangé en cas
/// d'échec (répertoire courant illisible).
fn absolute(path: &Path) -> PathBuf {
    std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_name_and_members_only() {
        let text = "\
opencollection: 1.0.0
info:
  name: Test
  type: workspace
collections:
  - name: Clients
    path: ../clients
  - name: Factures
    path: /chemin/absolu/factures
    remote: https://exemple/depot.git
specs: []
docs: ''
";
        let workspace = parse_workspace(text, Path::new("/ws/demo")).expect("valide");
        assert_eq!(workspace.name.as_deref(), Some("Test"));
        assert_eq!(workspace.path.as_deref(), Some(Path::new("/ws/demo")));
        assert_eq!(
            workspace.members,
            [
                Member {
                    name: Some("Clients".into()),
                    root: PathBuf::from("/ws/demo/../clients"),
                },
                Member {
                    name: Some("Factures".into()),
                    root: PathBuf::from("/chemin/absolu/factures"),
                },
            ]
        );
    }

    #[test]
    fn missing_keys_are_tolerated() {
        let workspace = parse_workspace("", Path::new("/ws")).expect("vide");
        assert_eq!(workspace.name, None);
        assert!(workspace.members.is_empty());

        let workspace = parse_workspace(
            "info:\n  name: ''\ncollections:\n  - path: a\n  - name: Sans chemin\n",
            Path::new("/ws"),
        )
        .expect("valide");
        assert_eq!(workspace.name, None);
        assert_eq!(workspace.members[0].name, None);
        assert_eq!(workspace.members[0].root, PathBuf::from("/ws/a"));
        assert_eq!(workspace.members[1].root, PathBuf::from("/ws/"));
    }

    #[test]
    fn invalid_yaml_keeps_only_the_position() {
        let error = parse_workspace("collections: [\n  secret: x", Path::new("/ws"))
            .expect_err("YAML invalide");
        assert!(error.0 > 0);
        let error = parse_workspace("collections: 3", Path::new("/ws")).expect_err("type");
        assert!(error.0 > 0);
    }

    #[test]
    fn unreadable_file_is_an_io_error() {
        let error = read_workspace(Path::new("/dossier/absent/xyz")).expect_err("absent");
        assert!(matches!(error, WorkspaceError::Io { .. }));
        assert!(error.to_string().contains("workspace.yml"));
    }

    #[test]
    fn paths_form_an_unnamed_workspace() {
        let workspace = from_paths(&[PathBuf::from("/a"), PathBuf::from("/b")]);
        assert_eq!(workspace.name, None);
        assert_eq!(workspace.path, None);
        assert_eq!(workspace.members.len(), 2);
        assert_eq!(workspace.members[1].root, PathBuf::from("/b"));
    }
}
