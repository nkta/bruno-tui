//! Opérations sur les fichiers et dossiers d'une collection `.bru`
//! (`docs/edition-phase3.md`) : créer, renommer, cloner, réordonner,
//! déplacer, mettre à la corbeille.
//!
//! Comportement repris de Bruno desktop 4.2.1 (relevé dans `app.asar`) :
//! contenus écrits identiques à ceux de `jsonToBru` /
//! `jsonToCollectionBru`, suffixes ` 1`, ` 2`… en cas de collision. Les
//! fichiers existants ne sont jamais réécrits en entier : seules les
//! lignes `name:` et `seq:` de `meta` changent ([`super::meta`]).
//!
//! Garde-fous :
//! - toutes les écritures restent sous la racine de la collection (chemins
//!   relatifs sans `..`, liens symboliques résolus) ;
//! - un fichier modifié sur disque depuis son chargement est refusé ;
//! - rien n'est créé à moitié : fichier ou dossier construit sous un nom
//!   temporaire, puis renommé ; un nom pris n'est jamais écrasé ;
//! - aucune suppression définitive : la corbeille ([`super::trash`]) seule ;
//! - déplacer = un seul `rename` (jamais copie puis suppression), refusé
//!   sur un autre disque ou si l'élément contient un lien symbolique
//!   (`docs/edition-phase4.md` § 3).

use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use thiserror::Error;

use super::error::{EditError, WriteError};
use super::meta::{MetaUpdate, MissingMeta, rewrite_meta};
use super::names::{
    MAX_DUPLICATE_NAMES, NameError, suffixed_name, truncate_base_for_suffix, validate_name,
    validate_request_filename,
};
use super::trash::{Trash, TrashError};
use super::url_params::{UnsupportedUrl, bruno_url_params};
use super::{FileStamp, write_atomic};
use crate::collection::ast::METHODS;

/// Extension des requêtes.
pub const BRU_EXTENSION: &str = "bru";
/// Fichier de dossier.
pub const FOLDER_FILE: &str = "folder.bru";

/// Erreur d'une opération de fichier. Le disque reste dans un état propre.
#[derive(Debug, Error)]
pub enum FileOpError {
    #[error("{0}")]
    Name(#[from] NameError),
    #[error("{0}")]
    Edit(#[from] EditError),
    #[error("`{}` a été modifié sur disque depuis son chargement", .path.display())]
    Stale { path: PathBuf },
    #[error("chemin hors de la collection : `{}`", .path.display())]
    Outside { path: PathBuf },
    #[error("`{}` introuvable", .path.display())]
    NotFound { path: PathBuf },
    #[error("lien symbolique dans `{}` : clonage refusé", .path.display())]
    Symlink { path: PathBuf },
    #[error("lien symbolique dans `{}` : déplacement refusé", .path.display())]
    SymlinkMove { path: PathBuf },
    #[error("Impossible de déplacer : autre disque")]
    CrossDevice,
    #[error("Impossible de déplacer `{}` dans lui-même ou dans un de ses sous-dossiers", .path.display())]
    IntoItself { path: PathBuf },
    #[error("`{}` est déjà dans ce dossier", .path.display())]
    SameDirectory { path: PathBuf },
    #[error("`{}` ne peut pas être déplacé seul", .path.display())]
    NotMovable { path: PathBuf },
    #[error("{0}")]
    UnsupportedUrl(#[from] UnsupportedUrl),
    /// Nom de paramètre que `jsonToBru` mettrait entre guillemets (`:`,
    /// `"`, `{`, `}`, espace) : le TUI ne saurait pas le relire.
    #[error(
        "paramètre « {0} » : nom à écrire entre guillemets, non pris en charge (crée la requête sans, puis complète l'URL avec m)"
    )]
    QuotedParamName(String),
    #[error("URL sur plusieurs lignes")]
    MultilineUrl,
    #[error("méthode inconnue")]
    UnknownMethod,
    #[error("{0}")]
    Trash(#[from] TrashError),
    #[error("bruno.json : {0}")]
    Config(#[from] super::bruno_config::ConfigError),
    #[error("erreur d'entrée/sortie sur `{}` : {source}", .path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

impl From<WriteError> for FileOpError {
    fn from(error: WriteError) -> Self {
        match error {
            WriteError::Stale { path } => Self::Stale { path },
            WriteError::Edit(error) => Self::Edit(error),
            WriteError::Io { path, source } => Self::Io { path, source },
            WriteError::Config(error) => Self::Config(error),
        }
    }
}

fn io_error(path: &Path) -> impl FnOnce(io::Error) -> FileOpError + '_ {
    move |source| FileOpError::Io {
        path: path.to_path_buf(),
        source,
    }
}

/// Type d'une nouvelle requête (gRPC et WebSocket : lecture seule).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestKind {
    Http,
    Graphql,
}

impl RequestKind {
    /// Valeur de `meta.type`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Http => "http",
            Self::Graphql => "graphql",
        }
    }
}

/// Nouvelle requête, telle que saisie dans le popup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewRequest {
    /// Nom affiché (`meta.name`).
    pub name: String,
    /// Nom du fichier, sans `.bru`.
    pub filename: String,
    pub kind: RequestKind,
    /// Méthode en minuscules, l'une de `METHODS`.
    pub method: String,
    pub url: String,
    pub seq: u32,
}

/// Contenu d'une nouvelle requête : `stringifyRequest` de la requête par
/// défaut de Bruno 4.2.1, paramètres extraits de l'URL compris (fixtures
/// `writer-cases/files/new-request-*.bruno.bru`). Refus si l'URL sort de
/// ce que le TUI reproduit à l'identique.
pub fn new_request_content(request: &NewRequest) -> Result<String, FileOpError> {
    if request.url.contains(['\n', '\r']) {
        return Err(FileOpError::MultilineUrl);
    }
    let params = bruno_url_params(&request.url)?;
    let names = params
        .query
        .iter()
        .map(|param| param.name.as_str())
        .chain(params.path.iter().map(String::as_str));
    for name in names {
        // `getKeyString` de `@usebruno/lang`.
        if name.contains([':', '"', '{', '}', ' ']) {
            return Err(FileOpError::QuotedParamName(name.to_owned()));
        }
    }
    let mut out = format!(
        "meta {{\n  name: {name}\n  type: {kind}\n  seq: {seq}\n}}\n\n\
         {method} {{\n  url: {url}\n  body: none\n  auth: inherit\n}}\n\n",
        name = request.name,
        kind = request.kind.as_str(),
        seq = request.seq,
        method = request.method,
        url = request.url,
    );
    if !params.query.is_empty() {
        out.push_str("params:query {\n");
        for param in &params.query {
            let value = param.value.as_deref().unwrap_or_default();
            out.push_str(&format!("  {}: {value}\n", param.name));
        }
        out.push_str("}\n\n");
    }
    if !params.path.is_empty() {
        out.push_str("params:path {\n");
        for name in &params.path {
            out.push_str(&format!("  {name}: \n"));
        }
        out.push_str("}\n\n");
    }
    out.push_str("settings {\n  encodeUrl: true\n  forwardAuthorizationHeader: false\n}\n");
    Ok(out)
}

/// Contenu du `folder.bru` d'un nouveau dossier (`newFolder` de Bruno).
pub fn new_folder_content(name: &str, seq: u32) -> String {
    format!("meta {{\n  name: {name}\n  seq: {seq}\n}}\n\nauth {{\n  mode: inherit\n}}\n")
}

/// `folder.bru` que Bruno écrit quand il manque (renommer : `name` seul ;
/// réordonner, cloner : `name` et `seq`).
pub fn folder_meta_content(name: &str, seq: Option<u32>) -> String {
    match seq {
        Some(seq) => format!("meta {{\n  name: {name}\n  seq: {seq}\n}}\n"),
        None => format!("meta {{\n  name: {name}\n}}\n"),
    }
}

/// Racine canonique de la collection.
fn canonical_root(root: &Path) -> Result<PathBuf, FileOpError> {
    fs::canonicalize(root).map_err(io_error(root))
}

/// Chemin absolu de `relative` sous `root`, refusé s'il sort de la racine :
/// composants autres que des noms (`..`, `/`), ou lien symbolique résolu
/// hors de la racine (pour l'élément lui-même s'il existe, sinon pour son
/// parent).
pub fn inside(root: &Path, relative: &Path) -> Result<PathBuf, FileOpError> {
    if relative
        .components()
        .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(FileOpError::Outside {
            path: relative.to_path_buf(),
        });
    }
    let root = canonical_root(root)?;
    let absolute = root.join(relative);
    let resolved = match fs::canonicalize(&absolute) {
        Ok(real) => real,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            let parent = absolute.parent().unwrap_or(&root);
            fs::canonicalize(parent).map_err(io_error(parent))?
        }
        Err(error) => return Err(io_error(&absolute)(error)),
    };
    if !resolved.starts_with(&root) {
        return Err(FileOpError::Outside {
            path: relative.to_path_buf(),
        });
    }
    Ok(absolute)
}

/// Chemin relatif à `root` d'un chemin construit par `inside`.
fn relative_to(root: &Path, absolute: &Path) -> PathBuf {
    let root = fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    absolute
        .strip_prefix(&root)
        .map(Path::to_path_buf)
        .unwrap_or_else(|_| absolute.to_path_buf())
}

/// Lit `path` et vérifie qu'il n'a pas changé depuis son chargement
/// (`expected`, source d'origine). Retourne le texte et son empreinte.
fn read_fresh(path: &Path, expected: &str) -> Result<(String, FileStamp), FileOpError> {
    let stamp = FileStamp::capture(path).map_err(io_error(path))?;
    let text = fs::read_to_string(path).map_err(io_error(path))?;
    if text != expected {
        return Err(FileOpError::Stale {
            path: path.to_path_buf(),
        });
    }
    Ok((text, stamp))
}

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Nom temporaire caché, dans le même répertoire que `name`.
fn temp_name(dir: &Path, name: &str) -> PathBuf {
    let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    dir.join(format!(".{name}.tmp-{}-{counter}", std::process::id()))
}

/// Crée `path` avec `bytes` sans jamais écraser : fichier temporaire, puis
/// lien dur vers le nom final (échoue si le nom est pris). `Ok(false)` si
/// le nom est déjà pris.
fn create_file_no_replace(
    path: &Path,
    bytes: &[u8],
    permissions: Option<fs::Permissions>,
) -> Result<bool, FileOpError> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let temp = temp_name(dir, &name);
    let written = fs::write(&temp, bytes).and_then(|()| match permissions {
        Some(permissions) => fs::set_permissions(&temp, permissions),
        None => Ok(()),
    });
    if let Err(error) = written {
        let _ = fs::remove_file(&temp);
        return Err(io_error(path)(error));
    }
    let linked = fs::hard_link(&temp, path);
    let _ = fs::remove_file(&temp);
    match linked {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => Ok(false),
        Err(error) => Err(io_error(path)(error)),
    }
}

/// Premier nom libre (`base`, `base 1`…) pour un fichier créé par
/// `create`, qui retourne `Ok(false)` si le nom est pris.
fn first_created(
    dir: &Path,
    base: &str,
    extension: &str,
    mut create: impl FnMut(&Path) -> Result<bool, FileOpError>,
) -> Result<PathBuf, FileOpError> {
    let safe = truncate_base_for_suffix(base, extension);
    for n in 0..MAX_DUPLICATE_NAMES {
        let candidate = dir.join(suffixed_name(&safe, extension, n));
        if create(&candidate)? {
            return Ok(candidate);
        }
    }
    Err(NameError::TooManyDuplicates(base.to_owned()).into())
}

/// Réserve un nom de dossier libre (`base`, `base 1`…) en le créant vide.
fn reserve_dir(dir: &Path, base: &str) -> Result<PathBuf, FileOpError> {
    first_created(dir, base, "", |candidate| match fs::create_dir(candidate) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => Ok(false),
        Err(error) => Err(io_error(candidate)(error)),
    })
}

/// Crée une requête dans `dir` (relatif à la racine). Retourne le chemin
/// relatif du fichier créé, suffixé en cas de collision.
pub fn create_request(
    root: &Path,
    dir: &Path,
    request: &NewRequest,
) -> Result<PathBuf, FileOpError> {
    validate_request_filename(&request.filename)?;
    check_display_name(&request.name)?;
    if !METHODS.contains(&request.method.as_str()) {
        return Err(FileOpError::UnknownMethod);
    }
    let content = new_request_content(request)?;
    let dir = inside(root, dir)?;
    let created = first_created(&dir, &request.filename, BRU_EXTENSION, |candidate| {
        create_file_no_replace(candidate, content.as_bytes(), None)
    })?;
    Ok(relative_to(root, &created))
}

/// Nom affiché valide : non vide, une ligne, sans espace en bord.
fn check_display_name(name: &str) -> Result<(), FileOpError> {
    if name.is_empty() || name.trim() != name || name.contains(['\n', '\r']) {
        return Err(EditError::InvalidMetaName.into());
    }
    Ok(())
}

/// Crée un dossier `dirname` dans `dir`, avec son `folder.bru`.
pub fn create_folder(
    root: &Path,
    dir: &Path,
    name: &str,
    dirname: &str,
    seq: u32,
) -> Result<PathBuf, FileOpError> {
    validate_name(dirname)?;
    check_display_name(name)?;
    let dir = inside(root, dir)?;
    let created = reserve_dir(&dir, dirname)?;
    let content = new_folder_content(name, seq);
    match create_file_no_replace(&created.join(FOLDER_FILE), content.as_bytes(), None) {
        Ok(true) => Ok(relative_to(root, &created)),
        outcome => {
            // Dossier vide réservé par nous : retiré pour ne rien laisser.
            let _ = fs::remove_dir(&created);
            match outcome {
                Err(error) => Err(error),
                _ => Err(FileOpError::Io {
                    path: created.join(FOLDER_FILE),
                    source: io::Error::from(io::ErrorKind::AlreadyExists),
                }),
            }
        }
    }
}

/// Déplace `from` vers un nom libre dérivé de `base` dans le répertoire
/// `dir` (le sien pour un renommage, un autre pour un déplacement), sans
/// jamais écraser. Dans son propre répertoire, le nom actuel compte comme
/// libre (`getUniqueRenamePath`).
fn move_to_free_name(
    from: &Path,
    dir: &Path,
    base: &str,
    extension: &str,
) -> Result<PathBuf, FileOpError> {
    let is_dir = fs::symlink_metadata(from).map_err(io_error(from))?.is_dir();
    let safe = truncate_base_for_suffix(base, extension);
    for n in 0..MAX_DUPLICATE_NAMES {
        let candidate = dir.join(suffixed_name(&safe, extension, n));
        if candidate == from {
            return Ok(candidate);
        }
        let moved = if is_dir {
            // Réservation par un dossier vide, que `rename` remplace.
            match fs::create_dir(&candidate) {
                Ok(()) => fs::rename(from, &candidate).inspect_err(|_| {
                    let _ = fs::remove_dir(&candidate);
                }),
                Err(error) => Err(error),
            }
        } else {
            fs::hard_link(from, &candidate).and_then(|()| fs::remove_file(from))
        };
        match moved {
            Ok(()) => return Ok(candidate),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) if error.kind() == io::ErrorKind::CrossesDevices => {
                return Err(FileOpError::CrossDevice);
            }
            Err(error) => return Err(io_error(&candidate)(error)),
        }
    }
    Err(NameError::TooManyDuplicates(base.to_owned()).into())
}

/// Renomme une requête : `meta.name`, puis le fichier si `filename` (sans
/// `.bru`) est donné. `expected` est le source chargé.
pub fn rename_request(
    root: &Path,
    path: &Path,
    expected: &str,
    name: &str,
    filename: Option<&str>,
) -> Result<PathBuf, FileOpError> {
    check_display_name(name)?;
    if let Some(filename) = filename {
        validate_request_filename(filename)?;
    }
    let absolute = inside(root, path)?;
    let (text, stamp) = read_fresh(&absolute, expected)?;
    let update = MetaUpdate {
        name: Some(name.to_owned()),
        seq: None,
    };
    let rewritten = rewrite_meta(&text, &update, MissingMeta::Refuse)?;
    if rewritten != text {
        write_atomic(&absolute, &stamp, rewritten.as_bytes())?;
    }
    let target = match filename {
        Some(filename) => {
            let dir = absolute.parent().unwrap_or(&absolute);
            move_to_free_name(&absolute, dir, filename, BRU_EXTENSION)?
        }
        None => absolute,
    };
    Ok(relative_to(root, &target))
}

/// Renomme un dossier : `meta.name` de `folder.bru` (créé s'il manque),
/// puis le répertoire si `dirname` est donné. `expected` est le source
/// chargé de `folder.bru`, `None` s'il n'existait pas.
pub fn rename_folder(
    root: &Path,
    path: &Path,
    expected: Option<&str>,
    name: &str,
    dirname: Option<&str>,
) -> Result<PathBuf, FileOpError> {
    check_display_name(name)?;
    if let Some(dirname) = dirname {
        validate_name(dirname)?;
    }
    let absolute = inside(root, path)?;
    if !absolute.is_dir() {
        return Err(FileOpError::NotFound {
            path: path.to_path_buf(),
        });
    }
    let folder_file = absolute.join(FOLDER_FILE);
    let update = MetaUpdate {
        name: Some(name.to_owned()),
        seq: None,
    };
    write_folder_meta(
        &folder_file,
        expected,
        &update,
        folder_meta_content(name, None),
    )?;
    let target = match dirname {
        Some(dirname) => {
            let dir = absolute.parent().unwrap_or(&absolute);
            move_to_free_name(&absolute, dir, dirname, "")?
        }
        None => absolute,
    };
    Ok(relative_to(root, &target))
}

/// Écrit `update` dans `folder.bru` : réécriture chirurgicale s'il existe
/// (et n'a pas changé), sinon création avec `created`.
fn write_folder_meta(
    folder_file: &Path,
    expected: Option<&str>,
    update: &MetaUpdate,
    created: String,
) -> Result<(), FileOpError> {
    match expected {
        Some(expected) => {
            let (text, stamp) = read_fresh(folder_file, expected)?;
            let rewritten = rewrite_meta(&text, update, MissingMeta::CreateAtTop)?;
            if rewritten != text {
                write_atomic(folder_file, &stamp, rewritten.as_bytes())?;
            }
            Ok(())
        }
        None => {
            if create_file_no_replace(folder_file, created.as_bytes(), None)? {
                Ok(())
            } else {
                Err(FileOpError::Stale {
                    path: folder_file.to_path_buf(),
                })
            }
        }
    }
}

/// Clone une requête : copie à l'octet près sous `filename`, sauf
/// `meta.name` et `meta.seq`.
pub fn clone_request(
    root: &Path,
    path: &Path,
    expected: &str,
    name: &str,
    filename: &str,
    seq: u32,
) -> Result<PathBuf, FileOpError> {
    check_display_name(name)?;
    validate_request_filename(filename)?;
    let absolute = inside(root, path)?;
    let (text, _) = read_fresh(&absolute, expected)?;
    let update = MetaUpdate {
        name: Some(name.to_owned()),
        seq: Some(seq),
    };
    let content = rewrite_meta(&text, &update, MissingMeta::Refuse)?;
    let permissions = fs::metadata(&absolute)
        .map_err(io_error(&absolute))?
        .permissions();
    let dir = absolute.parent().unwrap_or(&absolute).to_path_buf();
    let created = first_created(&dir, filename, BRU_EXTENSION, |candidate| {
        create_file_no_replace(candidate, content.as_bytes(), Some(permissions.clone()))
    })?;
    Ok(relative_to(root, &created))
}

/// Clone un dossier récursivement sous `dirname` : copie à l'octet près,
/// sauf `meta.name` et `meta.seq` de son `folder.bru` (créé s'il manque).
pub fn clone_folder(
    root: &Path,
    path: &Path,
    name: &str,
    dirname: &str,
    seq: u32,
) -> Result<PathBuf, FileOpError> {
    check_display_name(name)?;
    validate_name(dirname)?;
    let source = inside(root, path)?;
    if !source.is_dir() {
        return Err(FileOpError::NotFound {
            path: path.to_path_buf(),
        });
    }
    let parent = source.parent().unwrap_or(&source).to_path_buf();
    let temp = temp_name(&parent, dirname);
    let built = copy_tree(&source, &temp).and_then(|()| {
        let folder_file = temp.join(FOLDER_FILE);
        let update = MetaUpdate {
            name: Some(name.to_owned()),
            seq: Some(seq),
        };
        if folder_file.exists() {
            let text = fs::read_to_string(&folder_file).map_err(io_error(&folder_file))?;
            let rewritten = rewrite_meta(&text, &update, MissingMeta::CreateAtTop)?;
            fs::write(&folder_file, rewritten).map_err(io_error(&folder_file))
        } else {
            fs::write(&folder_file, folder_meta_content(name, Some(seq)))
                .map_err(io_error(&folder_file))
        }
    });
    let placed = built.and_then(|()| {
        first_created(&parent, dirname, "", |candidate| {
            match fs::create_dir(candidate) {
                Ok(()) => match fs::rename(&temp, candidate) {
                    Ok(()) => Ok(true),
                    Err(error) => {
                        let _ = fs::remove_dir(candidate);
                        Err(io_error(candidate)(error))
                    }
                },
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => Ok(false),
                Err(error) => Err(io_error(candidate)(error)),
            }
        })
    });
    match placed {
        Ok(created) => Ok(relative_to(root, &created)),
        Err(error) => {
            // Copie temporaire, faite par nous : retirée en cas d'échec.
            if temp.exists() {
                let _ = fs::remove_dir_all(&temp);
            }
            Err(error)
        }
    }
}

/// Copie récursive de `from` vers `to` (qui ne doit pas exister). Les
/// liens symboliques sont refusés.
fn copy_tree(from: &Path, to: &Path) -> Result<(), FileOpError> {
    fs::create_dir(to).map_err(io_error(to))?;
    let entries = fs::read_dir(from).map_err(io_error(from))?;
    for entry in entries {
        let entry = entry.map_err(io_error(from))?;
        let source = entry.path();
        let target = to.join(entry.file_name());
        let kind = entry.file_type().map_err(io_error(&source))?;
        if kind.is_symlink() {
            return Err(FileOpError::Symlink { path: source });
        } else if kind.is_dir() {
            copy_tree(&source, &target)?;
        } else if kind.is_file() {
            fs::copy(&source, &target).map_err(io_error(&source))?;
        }
    }
    Ok(())
}

/// Nouveau `seq` d'un élément réordonné.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeqChange {
    /// Chemin relatif de la requête ou du dossier.
    pub path: PathBuf,
    pub is_folder: bool,
    pub seq: u32,
    /// Source chargé (requête, ou `folder.bru`) ; `None` pour un dossier
    /// sans `folder.bru`.
    pub expected: Option<String>,
}

/// Réécrit les `seq` (`resequence-items`) : seuls les fichiers dont le
/// `seq` change sont écrits ; un dossier sans `folder.bru` en reçoit un,
/// nommé d'après son répertoire, comme Bruno. Tous les contenus sont
/// calculés avant la première écriture.
pub fn resequence(root: &Path, changes: &[SeqChange]) -> Result<(), FileOpError> {
    enum Plan {
        Rewrite {
            path: PathBuf,
            stamp: FileStamp,
            bytes: String,
        },
        Create {
            path: PathBuf,
            bytes: String,
        },
    }
    let mut plans = Vec::new();
    for change in changes {
        let absolute = inside(root, &change.path)?;
        let update = MetaUpdate {
            name: None,
            seq: Some(change.seq),
        };
        if change.is_folder {
            let folder_file = absolute.join(FOLDER_FILE);
            match &change.expected {
                Some(expected) => {
                    let (text, stamp) = read_fresh(&folder_file, expected)?;
                    let bytes = rewrite_meta(&text, &update, MissingMeta::CreateAtTop)?;
                    if bytes != text {
                        plans.push(Plan::Rewrite {
                            path: folder_file,
                            stamp,
                            bytes,
                        });
                    }
                }
                None => {
                    let dirname = absolute
                        .file_name()
                        .map(|name| name.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    plans.push(Plan::Create {
                        path: folder_file,
                        bytes: folder_meta_content(&dirname, Some(change.seq)),
                    });
                }
            }
        } else {
            let expected = change.expected.as_deref().unwrap_or_default();
            let (text, stamp) = read_fresh(&absolute, expected)?;
            let bytes = rewrite_meta(&text, &update, MissingMeta::Refuse)?;
            if bytes != text {
                plans.push(Plan::Rewrite {
                    path: absolute,
                    stamp,
                    bytes,
                });
            }
        }
    }
    for plan in plans {
        match plan {
            Plan::Rewrite { path, stamp, bytes } => {
                write_atomic(&path, &stamp, bytes.as_bytes())?;
            }
            Plan::Create { path, bytes } => {
                if !create_file_no_replace(&path, bytes.as_bytes(), None)? {
                    return Err(FileOpError::Stale { path });
                }
            }
        }
    }
    Ok(())
}

/// Met une requête ou un dossier à la corbeille. Retourne l'emplacement
/// dans la corbeille.
pub fn trash_item(
    root: &Path,
    path: &Path,
    trash: &Trash,
    deletion_date: &str,
) -> Result<PathBuf, FileOpError> {
    if path.as_os_str().is_empty() {
        return Err(FileOpError::Outside {
            path: path.to_path_buf(),
        });
    }
    let absolute = inside(root, path)?;
    if fs::symlink_metadata(&absolute).is_err() {
        return Err(FileOpError::NotFound {
            path: path.to_path_buf(),
        });
    }
    Ok(trash.put(&absolute, deletion_date)?)
}

/// Refuse tout lien symbolique dans `path` (lui-même ou, pour un dossier,
/// son contenu récursif).
fn ensure_no_symlink(path: &Path) -> Result<(), FileOpError> {
    let kind = fs::symlink_metadata(path)
        .map_err(io_error(path))?
        .file_type();
    if kind.is_symlink() {
        return Err(FileOpError::SymlinkMove {
            path: path.to_path_buf(),
        });
    }
    if kind.is_dir() {
        for entry in fs::read_dir(path).map_err(io_error(path))? {
            ensure_no_symlink(&entry.map_err(io_error(path))?.path())?;
        }
    }
    Ok(())
}

/// Déplace une requête ou un dossier (`path`) dans le dossier `target_dir`
/// de la même collection (vide : la racine). Un seul `rename` : rien n'est
/// copié ni supprimé ; le `seq` et le contenu ne changent pas. Nom pris :
/// suffixe ` 1`, ` 2`… comme Bruno. `expected` donne le source chargé de
/// chaque fichier de l'élément : un fichier modifié sur disque le refuse.
/// Retourne le nouveau chemin relatif.
pub fn move_item(
    root: &Path,
    path: &Path,
    target_dir: &Path,
    expected: &[(PathBuf, String)],
) -> Result<PathBuf, FileOpError> {
    if path.as_os_str().is_empty() {
        return Err(FileOpError::Outside {
            path: path.to_path_buf(),
        });
    }
    let absolute = inside(root, path)?;
    let target = inside(root, target_dir)?;
    let metadata = fs::symlink_metadata(&absolute).map_err(|_| FileOpError::NotFound {
        path: path.to_path_buf(),
    })?;
    let name = display_name(&absolute);
    let (base, extension) = if metadata.is_dir() {
        (name, "")
    } else {
        // `bruno.json`, `collection.bru`, `folder.bru` ne sont jamais
        // déplacés seuls.
        let reserved = name == FOLDER_FILE || name == "collection.bru";
        match Path::new(&name).extension() {
            Some(extension) if extension == BRU_EXTENSION && !reserved => {
                (file_stem_of(&name), BRU_EXTENSION)
            }
            _ => {
                return Err(FileOpError::NotMovable {
                    path: path.to_path_buf(),
                });
            }
        }
    };
    ensure_no_symlink(&absolute)?;
    match fs::symlink_metadata(&target) {
        Ok(metadata) if metadata.is_dir() => {}
        Ok(metadata) if metadata.is_symlink() => {
            return Err(FileOpError::SymlinkMove { path: target });
        }
        _ => {
            return Err(FileOpError::NotFound {
                path: target_dir.to_path_buf(),
            });
        }
    }
    if target.starts_with(&absolute) {
        return Err(FileOpError::IntoItself {
            path: path.to_path_buf(),
        });
    }
    if absolute.parent() == Some(target.as_path()) {
        return Err(FileOpError::SameDirectory {
            path: path.to_path_buf(),
        });
    }
    for (file, source) in expected {
        read_fresh(&inside(root, file)?, source)?;
    }
    let moved = move_to_free_name(&absolute, &target, &base, extension)?;
    Ok(relative_to(root, &moved))
}

/// Nom de fichier sans son extension `.bru`.
fn file_stem_of(name: &str) -> String {
    Path::new(name)
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Opération demandée par l'application, exécutée hors de la boucle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileOp {
    CreateRequest {
        dir: PathBuf,
        request: NewRequest,
    },
    CreateFolder {
        dir: PathBuf,
        name: String,
        dirname: String,
        seq: u32,
    },
    RenameRequest {
        path: PathBuf,
        expected: String,
        name: String,
        filename: Option<String>,
    },
    RenameFolder {
        path: PathBuf,
        expected: Option<String>,
        name: String,
        dirname: Option<String>,
    },
    CloneRequest {
        path: PathBuf,
        expected: String,
        name: String,
        filename: String,
        seq: u32,
    },
    CloneFolder {
        path: PathBuf,
        name: String,
        dirname: String,
        seq: u32,
    },
    Resequence {
        changes: Vec<SeqChange>,
        /// Élément déplacé, à garder sélectionné.
        moved: PathBuf,
    },
    /// Déplacer vers un autre dossier de la collection
    /// (`docs/edition-phase4.md`).
    Move {
        path: PathBuf,
        /// Dossier cible, relatif à la racine ; vide : la racine.
        target_dir: PathBuf,
        /// Source chargé de chaque fichier de l'élément (chemin relatif).
        expected: Vec<(PathBuf, String)>,
        /// Frères restants du dossier source, renumérotés comme après une
        /// corbeille ; `Err` : renumérotation impossible (raison), le
        /// déplacement se fait quand même.
        renumber: Result<Vec<SeqChange>, String>,
    },
    Trash {
        path: PathBuf,
        /// Frères à renuméroter après la mise à la corbeille, comme Bruno ;
        /// `Err` : renumérotation impossible (raison), la corbeille se fait
        /// quand même.
        renumber: Result<Vec<SeqChange>, String>,
    },
}

/// Résultat d'une opération réussie.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileOpOutcome {
    /// Élément à sélectionner après rechargement (relatif à la racine) ;
    /// `None` après une mise à la corbeille.
    pub select: Option<PathBuf>,
    /// Statut à afficher.
    pub message: String,
}

fn display_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Exécute `op` dans la collection `root`. `deletion_date` sert à la
/// corbeille (heure locale `AAAA-MM-JJThh:mm:ss`).
pub fn execute(
    root: &Path,
    op: &FileOp,
    trash: impl FnOnce() -> Result<Trash, TrashError>,
    deletion_date: &str,
) -> Result<FileOpOutcome, FileOpError> {
    let done = |path: PathBuf, verb: &str| FileOpOutcome {
        message: format!("{verb} : {}", display_name(&path)),
        select: Some(path),
    };
    match op {
        FileOp::CreateRequest { dir, request } => {
            create_request(root, dir, request).map(|path| done(path, "Requête créée"))
        }
        FileOp::CreateFolder {
            dir,
            name,
            dirname,
            seq,
        } => create_folder(root, dir, name, dirname, *seq).map(|path| done(path, "Dossier créé")),
        FileOp::RenameRequest {
            path,
            expected,
            name,
            filename,
        } => rename_request(root, path, expected, name, filename.as_deref())
            .map(|path| done(path, "Renommé")),
        FileOp::RenameFolder {
            path,
            expected,
            name,
            dirname,
        } => rename_folder(root, path, expected.as_deref(), name, dirname.as_deref())
            .map(|path| done(path, "Renommé")),
        FileOp::CloneRequest {
            path,
            expected,
            name,
            filename,
            seq,
        } => clone_request(root, path, expected, name, filename, *seq)
            .map(|path| done(path, "Clone créé")),
        FileOp::CloneFolder {
            path,
            name,
            dirname,
            seq,
        } => clone_folder(root, path, name, dirname, *seq).map(|path| done(path, "Clone créé")),
        FileOp::Resequence { changes, moved } => {
            resequence(root, changes).map(|()| FileOpOutcome {
                select: Some(moved.clone()),
                message: "Ordre enregistré".to_owned(),
            })
        }
        FileOp::Move {
            path,
            target_dir,
            expected,
            renumber,
        } => {
            let moved = move_item(root, path, target_dir, expected)?;
            let destination = if target_dir.as_os_str().is_empty() {
                "la racine de la collection".to_owned()
            } else {
                target_dir.display().to_string()
            };
            let done = format!("Déplacé vers {destination} : {}", display_name(&moved));
            // Le déplacement est fait : un échec de renumérotation n'annule
            // rien, il est signalé.
            let message = match renumber {
                Ok(changes) => match resequence(root, changes) {
                    Ok(()) => done,
                    Err(error) => format!("{done} ; renumérotation des seq en échec : {error}"),
                },
                Err(reason) => format!("{done} ; seq non renumérotés : {reason}"),
            };
            Ok(FileOpOutcome {
                select: Some(moved),
                message,
            })
        }
        FileOp::Trash { path, renumber } => {
            let trash = trash()?;
            trash_item(root, path, &trash, deletion_date)?;
            let trashed = format!("Mis à la corbeille : {}", display_name(path));
            // La corbeille est faite : un échec de renumérotation n'annule
            // rien, il est signalé.
            let message = match renumber {
                Ok(changes) => match resequence(root, changes) {
                    Ok(()) => trashed,
                    Err(error) => format!("{trashed} ; renumérotation des seq en échec : {error}"),
                },
                Err(reason) => format!("{trashed} ; seq non renumérotés : {reason}"),
            };
            Ok(FileOpOutcome {
                select: None,
                message,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contents_follow_bruno_layout() {
        let request = NewRequest {
            name: "A".into(),
            filename: "A".into(),
            kind: RequestKind::Graphql,
            method: "post".into(),
            url: String::new(),
            seq: 1,
        };
        assert!(
            new_request_content(&request)
                .expect("contenu")
                .contains("post {\n  url: \n")
        );
        assert_eq!(folder_meta_content("A", None), "meta {\n  name: A\n}\n");
    }
}
