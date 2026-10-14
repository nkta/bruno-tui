//! Abstraction de chargement d'une collection, indépendante du format.

use std::fs;
use std::path::{Path, PathBuf};

use super::config::{Presets, detect_format, find_opencollection_root, find_root, read_config};
use super::error::LoadError;
use super::opencollection::{self, COLLECTION_FILE};
use super::tree::{
    BruFormat, Collection, CollectionFormat, Ignore, LoadNotice, YmlFormat, load_settings,
    load_tree,
};

/// Charge une collection depuis un chemin.
///
/// Le chargement est synchrone et fait des lectures disque : l'appelant
/// l'exécute hors de la boucle d'événements (`tokio::task::spawn_blocking`).
pub trait CollectionLoader: Send + Sync {
    /// `path` peut désigner la racine ou n'importe quel fichier ou dossier
    /// de la collection.
    fn load(&self, path: &Path) -> Result<Collection, LoadError>;
}

/// Loader des collections Bruno au format `.bru`.
#[derive(Debug, Clone, Copy, Default)]
pub struct BruLoader;

impl CollectionLoader for BruLoader {
    fn load(&self, path: &Path) -> Result<Collection, LoadError> {
        load_bru(find_root(path)?)
    }
}

/// Loader des collections OpenCollection (YAML), en lecture seule.
#[derive(Debug, Clone, Copy, Default)]
pub struct OpenCollectionLoader;

impl CollectionLoader for OpenCollectionLoader {
    fn load(&self, path: &Path) -> Result<Collection, LoadError> {
        load_opencollection(find_opencollection_root(path)?)
    }
}

/// Loader par défaut : détecte le format comme Bruno 4
/// (`opencollection.yml` à la racine, sinon `bruno.json`) et délègue au
/// loader correspondant.
#[derive(Debug, Clone, Copy, Default)]
pub struct AutoLoader;

impl CollectionLoader for AutoLoader {
    fn load(&self, path: &Path) -> Result<Collection, LoadError> {
        match detect_format(path)? {
            (root, CollectionFormat::Bru) => load_bru(root),
            (root, CollectionFormat::OpenCollection) => load_opencollection(root),
        }
    }
}

fn directory_name(root: &Path) -> String {
    root.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn load_bru(root: PathBuf) -> Result<Collection, LoadError> {
    let config = read_config(&root)?;
    let walk = load_tree(&root, &Ignore::new(&config.ignore), &BruFormat)?;
    let mut notices = walk.notices;
    let settings = load_settings(&root, &BruFormat, &mut notices);
    let presets = Some(config.request_presets())
        .filter(|presets| *presets != Presets::default())
        .map(Box::new);
    let name = config.name.unwrap_or_else(|| directory_name(&root));
    Ok(Collection {
        settings,
        presets,
        name,
        format: CollectionFormat::Bru,
        tree: walk.tree,
        environments: walk.environments,
        notices,
        root,
    })
}

fn load_opencollection(root: PathBuf) -> Result<Collection, LoadError> {
    let path = root.join(COLLECTION_FILE);
    let bytes = fs::read(&path).map_err(|source| LoadError::Io {
        path: path.clone(),
        source,
    })?;
    let source = String::from_utf8_lossy(&bytes);
    let parsed = opencollection::parse_collection(path, &source)?;
    let config = parsed.value;
    let mut notices: Vec<LoadNotice> = parsed
        .notices
        .into_iter()
        .map(|message| LoadNotice {
            path: PathBuf::from(COLLECTION_FILE),
            message,
        })
        .collect();
    let walk = load_tree(&root, &Ignore::new(&config.ignore), &YmlFormat)?;
    notices.extend(walk.notices);
    let name = config.name.unwrap_or_else(|| directory_name(&root));
    Ok(Collection {
        settings: Some(Ok(config.meta)),
        presets: None,
        name,
        format: CollectionFormat::OpenCollection,
        tree: walk.tree,
        environments: walk.environments,
        notices,
        root,
    })
}
