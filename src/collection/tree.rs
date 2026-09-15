//! Parcours du disque et construction de l'arbre ordonné d'une collection.
//!
//! Lecture seule : seules des fonctions `std::fs` de lecture sont appelées.

use std::cmp::Ordering;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use super::ast::BruFile;
use super::error::{LoadError, ParseError};
use super::view::{Environment, FileMeta, RequestView};

/// Profondeur maximale de dossiers sous la racine.
pub const MAX_DEPTH: usize = 64;

const COLLECTION_FILE: &str = "collection.bru";
const FOLDER_FILE: &str = "folder.bru";
const ENVIRONMENTS_DIR: &str = "environments";
/// Toujours ignorés, à toute profondeur.
const ALWAYS_IGNORED: [&str; 2] = ["node_modules", ".git"];

/// Collection chargée, indépendante du format source.
#[derive(Debug)]
pub struct Collection {
    /// Racine absolue (répertoire contenant `bruno.json`).
    pub root: PathBuf,
    pub name: String,
    /// `collection.bru`, s'il existe.
    pub settings: Option<Result<FileMeta, ParseError>>,
    /// Enfants de la racine, triés.
    pub tree: Vec<TreeNode>,
    /// Environnements triés par nom de fichier ; un fichier invalide donne
    /// une erreur à sa place.
    pub environments: Environments,
}

/// Environnements d'une collection ; un fichier invalide donne une erreur
/// à sa place.
pub type Environments = Vec<Result<Environment, ErrorNode>>;

#[derive(Debug)]
pub enum TreeNode {
    Request(RequestNode),
    Folder(FolderNode),
    Error(ErrorNode),
}

#[derive(Debug)]
pub struct RequestNode {
    /// Chemin relatif à la racine, utilisable comme cible de `bru run`.
    pub path: PathBuf,
    pub view: RequestView,
    pub ast: Option<BruFile>,
}

#[derive(Debug)]
pub struct FolderNode {
    /// Chemin relatif à la racine.
    pub path: PathBuf,
    /// Nom déclaré dans `folder.bru`, sinon nom du répertoire.
    pub name: String,
    pub seq: Option<u32>,
    /// `folder.bru` : absent, valide ou en erreur.
    pub meta: Option<Result<FileMeta, ParseError>>,
    pub children: Vec<TreeNode>,
}

/// Fichier ou dossier qui n'a pas pu être chargé.
#[derive(Debug)]
pub struct ErrorNode {
    /// Chemin relatif à la racine.
    pub path: PathBuf,
    pub error: ParseError,
}

impl TreeNode {
    pub fn path(&self) -> &Path {
        match self {
            Self::Request(node) => &node.path,
            Self::Folder(node) => &node.path,
            Self::Error(node) => &node.path,
        }
    }

    pub fn seq(&self) -> Option<u32> {
        match self {
            Self::Request(node) => node.view.seq,
            Self::Folder(node) => node.seq,
            Self::Error(_) => None,
        }
    }

    /// Nom affichable : nom déclaré, sinon nom de fichier sans extension.
    pub fn name(&self) -> String {
        let declared = match self {
            Self::Request(node) => node.view.name.clone(),
            Self::Folder(node) => Some(node.name.clone()),
            Self::Error(_) => None,
        };
        declared.unwrap_or_else(|| {
            self.path()
                .file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
                .unwrap_or_default()
        })
    }
}

/// Ordre des enfants : `seq` croissant, sans `seq` en dernier, égalités
/// départagées par nom de fichier.
fn compare(a: &TreeNode, b: &TreeNode) -> Ordering {
    let key = |node: &TreeNode| (node.seq().is_none(), node.seq());
    key(a)
        .cmp(&key(b))
        .then_with(|| a.path().file_name().cmp(&b.path().file_name()))
}

/// Filtre des entrées à ne pas parcourir.
pub(crate) struct Ignore {
    /// Chemins relatifs à la racine, séparateurs `/`, sans `/` final.
    entries: Vec<String>,
}

impl Ignore {
    pub(crate) fn new(entries: &[String]) -> Self {
        Self {
            entries: entries
                .iter()
                .map(|entry| entry.trim_matches('/').to_owned())
                .filter(|entry| !entry.is_empty())
                .collect(),
        }
    }

    fn matches(&self, relative: &Path) -> bool {
        if relative
            .file_name()
            .is_some_and(|name| ALWAYS_IGNORED.iter().any(|ignored| name == *ignored))
        {
            return true;
        }
        let relative = relative
            .components()
            .map(|c| c.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/");
        self.entries.iter().any(|entry| {
            relative == *entry
                || relative
                    .strip_prefix(entry.as_str())
                    .is_some_and(|rest| rest.starts_with('/'))
        })
    }
}

/// Parcourt la racine et construit l'arbre et les environnements.
pub(crate) fn load_tree(
    root: &Path,
    ignore: &Ignore,
) -> Result<(Vec<TreeNode>, Environments), LoadError> {
    let entries = sorted_entries(root).map_err(|source| LoadError::Io {
        path: root.to_path_buf(),
        source,
    })?;
    let mut children = Vec::new();
    let mut environments = Vec::new();
    for (name, kind) in entries {
        let relative = PathBuf::from(&name);
        if ignore.matches(&relative) {
            continue;
        }
        match kind {
            EntryKind::Dir if name == ENVIRONMENTS_DIR => {
                environments = load_environments(root, &relative);
            }
            EntryKind::Dir => children.push(load_folder(root, relative, ignore, 1)),
            EntryKind::File if name == COLLECTION_FILE || name == FOLDER_FILE => {}
            EntryKind::File if is_bru(&relative) => children.push(load_request(root, relative)),
            EntryKind::File => {}
        }
    }
    children.sort_by(compare);
    Ok((children, environments))
}

/// Charge `collection.bru` à la racine, s'il existe.
pub(crate) fn load_settings(root: &Path) -> Option<Result<FileMeta, ParseError>> {
    let path = root.join(COLLECTION_FILE);
    path.is_file()
        .then(|| read_bru(&path).map(FileMeta::from_ast))
}

fn load_folder(root: &Path, relative: PathBuf, ignore: &Ignore, depth: usize) -> TreeNode {
    if depth > MAX_DEPTH {
        return TreeNode::Error(ErrorNode {
            path: relative,
            error: ParseError::TooDeep { max: MAX_DEPTH },
        });
    }
    let absolute = root.join(&relative);
    let entries = match sorted_entries(&absolute) {
        Ok(entries) => entries,
        Err(source) => {
            return TreeNode::Error(ErrorNode {
                path: relative,
                error: ParseError::Io(source),
            });
        }
    };

    let mut meta = None;
    let mut children = Vec::new();
    for (name, kind) in entries {
        let child = relative.join(&name);
        if ignore.matches(&child) {
            continue;
        }
        match kind {
            EntryKind::Dir => children.push(load_folder(root, child, ignore, depth + 1)),
            EntryKind::File if name == FOLDER_FILE => {
                meta = Some(read_bru(&root.join(&child)).map(FileMeta::from_ast));
            }
            EntryKind::File if name == COLLECTION_FILE => {}
            EntryKind::File if is_bru(&child) => children.push(load_request(root, child)),
            EntryKind::File => {}
        }
    }
    children.sort_by(compare);

    let declared = match &meta {
        Some(Ok(meta)) => (meta.name.clone(), meta.seq),
        _ => (None, None),
    };
    let name = declared.0.unwrap_or_else(|| {
        relative
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default()
    });
    TreeNode::Folder(FolderNode {
        path: relative,
        name,
        seq: declared.1,
        meta,
        children,
    })
}

fn load_request(root: &Path, relative: PathBuf) -> TreeNode {
    let parsed = read_bru(&root.join(&relative))
        .and_then(|ast| RequestView::from_ast(&ast).map(|view| (view, ast)));
    match parsed {
        Ok((view, ast)) => TreeNode::Request(RequestNode {
            path: relative,
            view,
            ast: Some(ast),
        }),
        Err(error) => TreeNode::Error(ErrorNode {
            path: relative,
            error,
        }),
    }
}

fn load_environments(root: &Path, relative: &Path) -> Environments {
    let entries = match sorted_entries(&root.join(relative)) {
        Ok(entries) => entries,
        Err(source) => {
            return vec![Err(ErrorNode {
                path: relative.to_path_buf(),
                error: ParseError::Io(source),
            })];
        }
    };
    entries
        .into_iter()
        .filter(|(_, kind)| *kind == EntryKind::File)
        .map(|(name, _)| relative.join(name))
        .filter(|path| is_bru(path))
        .map(|path| {
            let name = path
                .file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
                .unwrap_or_default();
            match read_bru(&root.join(&path)) {
                Ok(ast) => Ok(Environment::from_ast(name, path, ast)),
                Err(error) => Err(ErrorNode { path, error }),
            }
        })
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EntryKind {
    File,
    Dir,
}

/// Entrées d'un répertoire triées par nom. Les liens symboliques vers des
/// répertoires sont écartés ; ceux vers des fichiers sont lus.
fn sorted_entries(dir: &Path) -> std::io::Result<Vec<(OsString, EntryKind)>> {
    let mut entries = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let kind = if file_type.is_symlink() {
            match fs::metadata(entry.path()) {
                Ok(meta) if meta.is_file() => EntryKind::File,
                _ => continue,
            }
        } else if file_type.is_dir() {
            EntryKind::Dir
        } else if file_type.is_file() {
            EntryKind::File
        } else {
            continue;
        };
        entries.push((entry.file_name(), kind));
    }
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(entries)
}

fn is_bru(path: &Path) -> bool {
    path.extension().is_some_and(|ext| ext == "bru")
}

fn read_bru(path: &Path) -> Result<BruFile, ParseError> {
    let bytes = fs::read(path).map_err(ParseError::Io)?;
    let source = String::from_utf8(bytes).map_err(|_| ParseError::InvalidUtf8)?;
    BruFile::parse(source)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ignore_matches_entries_and_always_ignored() {
        let ignore = Ignore::new(&["tmp/".to_owned(), "a/b".to_owned()]);
        assert!(ignore.matches(Path::new("tmp")));
        assert!(ignore.matches(Path::new("tmp/x.bru")));
        assert!(!ignore.matches(Path::new("tmpfile.bru")));
        assert!(ignore.matches(Path::new("a/b")));
        assert!(!ignore.matches(Path::new("a")));
        assert!(!ignore.matches(Path::new("a/bc")));
        assert!(ignore.matches(Path::new("deep/node_modules")));
        assert!(ignore.matches(Path::new(".git")));
    }
}
