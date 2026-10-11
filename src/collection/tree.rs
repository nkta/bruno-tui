//! Parcours du disque et construction de l'arbre ordonné d'une collection.
//!
//! Lecture seule : seules des fonctions `std::fs` de lecture sont appelées.
//! Le parcours est commun aux deux formats ; la lecture de chaque fichier
//! est confiée à un [`SourceFormat`] (`.bru` ou OpenCollection YAML).

use std::cmp::Ordering;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use super::ast::BruFile;
use super::error::{LoadError, ParseError};
use super::opencollection;
use super::view::{Environment, FileMeta, RequestView};

/// Profondeur maximale de dossiers sous la racine.
pub const MAX_DEPTH: usize = 64;

const COLLECTION_FILE: &str = "collection.bru";
const FOLDER_FILE: &str = "folder.bru";
const ENVIRONMENTS_DIR: &str = "environments";
/// Toujours ignorés, à toute profondeur.
const ALWAYS_IGNORED: [&str; 2] = ["node_modules", ".git"];

/// Format source d'une collection, détecté comme Bruno 4 :
/// `opencollection.yml` à la racine, sinon `bruno.json`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CollectionFormat {
    /// Fichiers `.bru` (bru-lang), éditables.
    #[default]
    Bru,
    /// Fichiers `.yml` OpenCollection, en lecture seule : le writer ne
    /// connaît que bru-lang.
    OpenCollection,
}

impl CollectionFormat {
    /// Vrai si le TUI sait écrire les fichiers de ce format.
    pub fn is_editable(self) -> bool {
        self == Self::Bru
    }
}

/// Avis non bloquant émis au chargement : champ ignoré parce que le TUI ne
/// sait pas l'afficher. Le message cite une clé, jamais une valeur.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadNotice {
    /// Fichier concerné, relatif à la racine.
    pub path: PathBuf,
    pub message: String,
}

/// Collection chargée, indépendante du format source.
#[derive(Debug)]
pub struct Collection {
    /// Racine absolue (répertoire contenant `bruno.json` ou
    /// `opencollection.yml`).
    pub root: PathBuf,
    pub name: String,
    pub format: CollectionFormat,
    /// `collection.bru` ou `opencollection.yml`, s'il existe.
    pub settings: Option<Result<FileMeta, ParseError>>,
    /// Enfants de la racine, triés.
    pub tree: Vec<TreeNode>,
    /// Environnements triés par nom de fichier ; un fichier invalide donne
    /// une erreur à sa place.
    pub environments: Environments,
    /// Avis non bloquants (champs ignorés), dans l'ordre du parcours.
    pub notices: Vec<LoadNotice>,
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
    /// AST source ; absent pour un format autre que `.bru`.
    pub ast: Option<BruFile>,
}

#[derive(Debug)]
pub struct FolderNode {
    /// Chemin relatif à la racine.
    pub path: PathBuf,
    /// Nom déclaré dans `folder.bru`/`folder.yml`, sinon nom du répertoire.
    pub name: String,
    pub seq: Option<u32>,
    /// `folder.bru`/`folder.yml` : absent, valide ou en erreur.
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

/// Lecture des fichiers d'un format de collection ; le parcours du disque
/// est commun.
pub(crate) trait SourceFormat {
    /// Extension des requêtes et des environnements, sans point.
    fn extension(&self) -> &'static str;
    fn collection_file(&self) -> &'static str;
    fn folder_file(&self) -> &'static str;
    /// Requête ; `absolute` est le fichier, `relative` son chemin dans la
    /// collection.
    fn request(
        &self,
        absolute: &Path,
        relative: PathBuf,
        notices: &mut Vec<LoadNotice>,
    ) -> TreeNode;
    /// Métadonnées d'un dossier (`folder.*`) ou de la collection.
    fn meta(
        &self,
        absolute: &Path,
        relative: &Path,
        notices: &mut Vec<LoadNotice>,
    ) -> Result<FileMeta, ParseError>;
    fn environment(
        &self,
        absolute: &Path,
        name: String,
        relative: PathBuf,
        notices: &mut Vec<LoadNotice>,
    ) -> Result<Environment, ErrorNode>;
}

/// Format `.bru`.
pub(crate) struct BruFormat;

impl SourceFormat for BruFormat {
    fn extension(&self) -> &'static str {
        "bru"
    }

    fn collection_file(&self) -> &'static str {
        COLLECTION_FILE
    }

    fn folder_file(&self) -> &'static str {
        FOLDER_FILE
    }

    fn request(&self, absolute: &Path, relative: PathBuf, _: &mut Vec<LoadNotice>) -> TreeNode {
        let parsed =
            read_bru(absolute).and_then(|ast| RequestView::from_ast(&ast).map(|view| (view, ast)));
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

    fn meta(
        &self,
        absolute: &Path,
        _: &Path,
        _: &mut Vec<LoadNotice>,
    ) -> Result<FileMeta, ParseError> {
        read_bru(absolute).map(FileMeta::from_ast)
    }

    fn environment(
        &self,
        absolute: &Path,
        name: String,
        relative: PathBuf,
        _: &mut Vec<LoadNotice>,
    ) -> Result<Environment, ErrorNode> {
        match read_env_bru(absolute) {
            Ok(ast) => Ok(Environment::from_ast(name, relative, ast)),
            Err(error) => Err(ErrorNode {
                path: relative,
                error,
            }),
        }
    }
}

/// Format OpenCollection (YAML).
pub(crate) struct YmlFormat;

/// Rattache les avis d'un fichier à son chemin relatif.
fn attach(notices: &mut Vec<LoadNotice>, relative: &Path, messages: Vec<String>) {
    notices.extend(messages.into_iter().map(|message| LoadNotice {
        path: relative.to_path_buf(),
        message,
    }));
}

impl SourceFormat for YmlFormat {
    fn extension(&self) -> &'static str {
        opencollection::EXTENSION
    }

    fn collection_file(&self) -> &'static str {
        opencollection::COLLECTION_FILE
    }

    fn folder_file(&self) -> &'static str {
        opencollection::FOLDER_FILE
    }

    fn request(
        &self,
        absolute: &Path,
        relative: PathBuf,
        notices: &mut Vec<LoadNotice>,
    ) -> TreeNode {
        match read_text(absolute).and_then(|source| opencollection::parse_item(&source)) {
            Ok(parsed) => {
                attach(notices, &relative, parsed.notices);
                TreeNode::Request(RequestNode {
                    path: relative,
                    view: parsed.value,
                    ast: None,
                })
            }
            Err(error) => TreeNode::Error(ErrorNode {
                path: relative,
                error,
            }),
        }
    }

    fn meta(
        &self,
        absolute: &Path,
        relative: &Path,
        notices: &mut Vec<LoadNotice>,
    ) -> Result<FileMeta, ParseError> {
        let parsed = opencollection::parse_folder(&read_text(absolute)?)?;
        attach(notices, relative, parsed.notices);
        Ok(parsed.value)
    }

    fn environment(
        &self,
        absolute: &Path,
        name: String,
        relative: PathBuf,
        notices: &mut Vec<LoadNotice>,
    ) -> Result<Environment, ErrorNode> {
        let parsed = read_text(absolute)
            .and_then(|source| opencollection::parse_environment(name, relative.clone(), &source));
        match parsed {
            Ok(parsed) => {
                attach(notices, &relative, parsed.notices);
                Ok(parsed.value)
            }
            Err(error) => Err(ErrorNode {
                path: relative,
                error,
            }),
        }
    }
}

/// Arbre, environnements et avis produits par un parcours.
pub(crate) struct Walk {
    pub tree: Vec<TreeNode>,
    pub environments: Environments,
    pub notices: Vec<LoadNotice>,
}

/// Parcourt la racine et construit l'arbre et les environnements.
pub(crate) fn load_tree(
    root: &Path,
    ignore: &Ignore,
    format: &dyn SourceFormat,
) -> Result<Walk, LoadError> {
    let entries = sorted_entries(root).map_err(|source| LoadError::Io {
        path: root.to_path_buf(),
        source,
    })?;
    let mut walker = Walker {
        root,
        ignore,
        format,
        notices: Vec::new(),
    };
    let mut children = Vec::new();
    let mut environments = Vec::new();
    for (name, kind) in entries {
        let relative = PathBuf::from(&name);
        if ignore.matches(&relative) {
            continue;
        }
        match kind {
            EntryKind::Dir if name == ENVIRONMENTS_DIR => {
                environments = walker.environments(&relative);
            }
            EntryKind::Dir => children.push(walker.folder(relative, 1)),
            EntryKind::File if walker.is_meta_file(&name) => {}
            EntryKind::File if walker.is_source(&relative) => {
                children.push(walker.request(relative));
            }
            EntryKind::File => {}
        }
    }
    children.sort_by(compare);
    Ok(Walk {
        tree: children,
        environments,
        notices: walker.notices,
    })
}

/// Charge le fichier de collection (`collection.bru`, `opencollection.yml`)
/// à la racine, s'il existe.
pub(crate) fn load_settings(
    root: &Path,
    format: &dyn SourceFormat,
    notices: &mut Vec<LoadNotice>,
) -> Option<Result<FileMeta, ParseError>> {
    let relative = Path::new(format.collection_file());
    let path = root.join(relative);
    path.is_file()
        .then(|| format.meta(&path, relative, notices))
}

struct Walker<'a> {
    root: &'a Path,
    ignore: &'a Ignore,
    format: &'a dyn SourceFormat,
    notices: Vec<LoadNotice>,
}

impl Walker<'_> {
    fn is_meta_file(&self, name: &OsString) -> bool {
        name == self.format.collection_file() || name == self.format.folder_file()
    }

    fn is_source(&self, path: &Path) -> bool {
        path.extension()
            .is_some_and(|ext| ext == self.format.extension())
    }

    fn request(&mut self, relative: PathBuf) -> TreeNode {
        let absolute = self.root.join(&relative);
        self.format.request(&absolute, relative, &mut self.notices)
    }

    fn folder(&mut self, relative: PathBuf, depth: usize) -> TreeNode {
        if depth > MAX_DEPTH {
            return TreeNode::Error(ErrorNode {
                path: relative,
                error: ParseError::TooDeep { max: MAX_DEPTH },
            });
        }
        let absolute = self.root.join(&relative);
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
            if self.ignore.matches(&child) {
                continue;
            }
            match kind {
                EntryKind::Dir => children.push(self.folder(child, depth + 1)),
                EntryKind::File if name == self.format.folder_file() => {
                    let absolute = self.root.join(&child);
                    meta = Some(self.format.meta(&absolute, &child, &mut self.notices));
                }
                EntryKind::File if name == self.format.collection_file() => {}
                EntryKind::File if self.is_source(&child) => children.push(self.request(child)),
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

    fn environments(&mut self, relative: &Path) -> Environments {
        let entries = match sorted_entries(&self.root.join(relative)) {
            Ok(entries) => entries,
            Err(source) => {
                return vec![Err(ErrorNode {
                    path: relative.to_path_buf(),
                    error: ParseError::Io(source),
                })];
            }
        };
        let mut environments = Vec::new();
        for (name, kind) in entries {
            let path = relative.join(name);
            if kind != EntryKind::File || !self.is_source(&path) {
                continue;
            }
            let name = path
                .file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
                .unwrap_or_default();
            let absolute = self.root.join(&path);
            environments.push(
                self.format
                    .environment(&absolute, name, path, &mut self.notices),
            );
        }
        environments
    }
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

fn read_text(path: &Path) -> Result<String, ParseError> {
    let bytes = fs::read(path).map_err(ParseError::Io)?;
    String::from_utf8(bytes).map_err(|_| ParseError::InvalidUtf8)
}

fn read_bru(path: &Path) -> Result<BruFile, ParseError> {
    BruFile::parse(read_text(path)?)
}

fn read_env_bru(path: &Path) -> Result<BruFile, ParseError> {
    BruFile::parse_environment(read_text(path)?)
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
